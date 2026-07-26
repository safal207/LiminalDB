use std::cmp::Ordering;
use std::slice;
use std::sync::atomic::AtomicU32;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use liminal_core::life_loop::run_loop;
use liminal_core::types::Hint;
use liminal_core::ClusterField;
use liminal_core::{
    detect_sync_groups, parse_lql, replay_epoch_by_id, run_collective_dream, AwakeningConfig,
    AwakeningReport, Influence, ReplayConfig, ResonantModel, Tension,
};
use liminal_store::{decode_delta, DiskJournal, Offset};
use serde_json::{json, Value as JsonValue};
use tokio::runtime::Builder;
use tokio::sync::Mutex;

use blake3::Hasher;
use liminal_bridge_net::noetic::consensus::{
    ConsensusEngine, NoeticEvent, Phase, Proposal, ProposalObject, VoteDecision,
};
use liminal_bridge_net::noetic::crdt::Hlc;
use liminal_bridge_net::noetic::state::{
    NoeticState, PolicyBundle, ResonantModelHeader, ResonantModelTopK, SeedHeader, SeedStatus,
    Summary,
};
use liminal_bridge_net::peers::{GossipManager, PeerId, PeerInfo};
use rand::rngs::OsRng;
use rand::RngCore;

use crate::protocol::{
    adjust_tick, event_from_field_log, event_from_hint, event_from_impulse_log, event_from_metrics,
    event_from_snapshot, BridgeConfig, IntrospectRequest, IntrospectTarget, NoeticObjectKind,
    Outbox, PeerSpec, ProtocolCommand, ProtocolMetrics, ProtocolPush,
};

struct BridgeState {
    runtime: tokio::runtime::Runtime,
    field: Arc<Mutex<ClusterField>>,
    outbox: Arc<StdMutex<Outbox>>,
    #[allow(dead_code)]
    journal: Option<Arc<DiskJournal>>,
    snapshot_cfg: Option<SnapshotConfig>,
    namespace: String,
    gossip: Arc<GossipManager>,
    noetic: StdMutex<NoeticRuntime>,
}

#[derive(Clone)]
struct SnapshotConfig {
    interval: Duration,
    max_wal_events: u64,
}

struct NoeticRuntime {
    local_peer: PeerId,
    local_credence: f32,
    state: NoeticState,
    consensus: ConsensusEngine,
}

impl NoeticRuntime {
    fn new(local_peer: PeerId) -> Self {
        let mut state = NoeticState::new(local_peer);
        let local_credence = 1.0;
        state.set_credence(local_peer, local_credence);
        NoeticRuntime {
            local_peer,
            local_credence,
            state,
            consensus: ConsensusEngine::new(0.6),
        }
    }

    fn tick_clock(&mut self) -> Hlc {
        let now = now_ms();
        self.state.clock.tick(now);
        self.state.clock
    }

    fn register_peer(&mut self, peer: PeerId, credence: f32) {
        self.state.set_credence(peer, credence);
    }

    fn set_quorum(&mut self, quorum: f32) -> f32 {
        self.consensus.set_quorum(quorum);
        self.consensus.quorum()
    }

    fn process_event(&mut self, event: NoeticEvent) -> Vec<JsonValue> {
        let mut events = Vec::new();
        if let Ok(json) = serde_json::to_value(&event) {
            events.push(json);
        }
        if matches!(event.meta.phase, Phase::Commit) {
            if let Some(proposal) = self.consensus.proposals().get(&event.meta.id).cloned() {
                self.apply_commit(&proposal, &mut events);
            }
        }
        events
    }

    fn apply_commit(&mut self, proposal: &Proposal, events: &mut Vec<JsonValue>) {
        let now = now_ms();
        self.state.clock = self
            .state
            .clock
            .merged(&proposal.clock, self.local_peer, now);
        match &proposal.object {
            ProposalObject::Model { header, topk } => {
                self.state.apply_model_header(header.clone());
                self.state
                    .update_model_topk(topk.entries.clone(), topk.updated);
            }
            ProposalObject::Seed { header, status } => {
                self.state.apply_seed(header.clone());
                self.state
                    .update_seed_status(status.clone(), proposal.clock);
            }
            ProposalObject::Policy { bundle } => {
                self.state.update_policy(bundle.clone());
            }
        }
        let summary = self.state.summary();
        events.push(build_merge_event(&summary));
    }

    fn local_peer(&self) -> PeerId {
        self.local_peer
    }

    fn local_credence(&self) -> f32 {
        self.local_credence
    }
}

static STATE: OnceLock<BridgeState> = OnceLock::new();

impl BridgeState {
    fn new(config: BridgeConfig) -> Result<Self, String> {
        if config.tick_ms == 0 {
            return Err("tick_ms must be greater than zero".into());
        }
        let store_path = config.store_path.clone();
        let interval_secs = config.snap_interval.unwrap_or(60).max(1);
        let max_wal = config.snap_maxwal.unwrap_or(5_000).max(1);
        let snapshot_cfg = store_path.as_ref().map(|_| SnapshotConfig {
            interval: Duration::from_secs(interval_secs as u64),
            max_wal_events: max_wal as u64,
        });
        let runtime = Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let (field, journal) = if let Some(path) = store_path.clone() {
            let journal = Arc::new(DiskJournal::open(path).map_err(|e| e.to_string())?);
            let (mut field, replay_offset) = if let Some((seed, offset)) =
                journal.load_latest_snapshot().map_err(|e| e.to_string())?
            {
                (seed.into_field(), offset)
            } else {
                (ClusterField::new(), Offset::start())
            };
            let mut stream = journal
                .stream_from(replay_offset)
                .map_err(|e| e.to_string())?;
            while let Some(record) = stream.next() {
                let bytes = record.map_err(|e| e.to_string())?;
                let delta = decode_delta(&bytes).map_err(|e| e.to_string())?;
                field.apply_delta(&delta);
            }
            if field.cells.is_empty() {
                field.add_root("liminal/root");
            }
            field.set_journal(journal.clone());
            (field, Some(journal))
        } else {
            let mut field = ClusterField::new();
            field.add_root("liminal/root");
            (field, None)
        };
        let field = Arc::new(Mutex::new(field));
        let outbox = Arc::new(StdMutex::new(Outbox::default()));
        let tick_ms = Arc::new(AtomicU32::new(config.tick_ms));

        let namespace = "liminal".to_string();
        let local_peer = generate_peer_id();
        let gossip = Arc::new(GossipManager::new(local_peer));
        let noetic_runtime = NoeticRuntime::new(local_peer);
        let local_info = PeerInfo {
            id: local_peer,
            url: "local".into(),
            namespace: namespace.clone(),
            credence: noetic_runtime.local_credence(),
            last_seen_ms: now_ms(),
        };
        gossip.register_peer(local_info);

        let state = BridgeState {
            runtime,
            field: field.clone(),
            outbox: outbox.clone(),
            journal,
            snapshot_cfg,
            namespace,
            gossip,
            noetic: StdMutex::new(noetic_runtime),
        };

        state.start_life_loop(config.tick_ms, outbox.clone(), tick_ms);

        if let (Some(journal), Some(cfg)) = (state.journal.clone(), state.snapshot_cfg.clone()) {
            state.spawn_snapshot_loop(journal, cfg, outbox);
        }

        Ok(state)
    }

    fn start_life_loop(
        &self,
        initial_tick: u32,
        outbox: Arc<StdMutex<Outbox>>,
        tick_ms: Arc<AtomicU32>,
    ) {
        let metrics_box = outbox.clone();
        let events_box = outbox.clone();
        let hints_box = outbox.clone();
        let events_tick = tick_ms.clone();
        let hints_tick = tick_ms.clone();
        let field = self.field.clone();

        self.runtime.spawn(run_loop(
            field,
            initial_tick as u64,
            move |metrics| {
                let mut guard = metrics_box.lock().unwrap();
                let proto = ProtocolMetrics::from_core(metrics);
                let event = event_from_metrics(&proto, 1_000);
                guard.set_metrics(proto.clone());
                guard.push_event(event);
            },
            move |event| {
                if let Some(proto) = event_from_field_log(
                    event,
                    events_tick.load(std::sync::atomic::Ordering::Relaxed),
                ) {
                    let mut guard = events_box.lock().unwrap();
                    guard.push_event(proto);
                }
            },
            move |hint: &Hint| {
                let new_tick = adjust_tick(&hints_tick, hint);
                let event = event_from_hint(hint, new_tick);
                let mut guard = hints_box.lock().unwrap();
                guard.push_event(event);
            },
        ));
    }

    fn spawn_snapshot_loop(
        &self,
        journal: Arc<DiskJournal>,
        cfg: SnapshotConfig,
        outbox: Arc<StdMutex<Outbox>>,
    ) {
        let field = self.field.clone();
        self.runtime.spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(5));
            let mut last_snapshot = tokio::time::Instant::now();
            loop {
                ticker.tick().await;
                if journal.delta_since_snapshot() >= cfg.max_wal_events
                    || last_snapshot.elapsed() >= cfg.interval
                {
                    let snapshot_result = {
                        let guard = field.lock().await;
                        journal.write_snapshot(&*guard)
                    };
                    match snapshot_result {
                        Ok(info) => {
                            last_snapshot = tokio::time::Instant::now();
                            if let Err(err) = journal.run_gc(info.offset) {
                                eprintln!("snapshot GC failed: {err}");
                            }
                            if let Ok(mut out) = outbox.lock() {
                                out.push_event(event_from_snapshot(info.id));
                            }
                        }
                        Err(err) => eprintln!("snapshot failed: {err}"),
                    }
                }
            }
        });
    }

    fn handle_peer_add(&self, spec: PeerSpec) -> Result<Vec<JsonValue>, String> {
        let namespace = spec.namespace.unwrap_or_else(|| self.namespace.clone());
        let credence = spec.credence.clamp(0.0, 1.0);
        let peer_id = derive_peer_id(&spec.url, &namespace);
        let info = PeerInfo {
            id: peer_id,
            url: spec.url,
            namespace,
            credence,
            last_seen_ms: now_ms(),
        };
        self.gossip.register_peer(info.clone());
        if let Ok(mut guard) = self.noetic.lock() {
            guard.register_peer(peer_id, credence);
        }
        let peer_json = serde_json::to_value(info).map_err(|e| e.to_string())?;
        Ok(vec![json!({
            "ev": "peers",
            "meta": {
                "action": "add",
                "peer": peer_json,
            }
        })])
    }

    fn handle_peer_list(&self) -> Vec<JsonValue> {
        let peers: Vec<JsonValue> = self
            .gossip
            .peers()
            .into_iter()
            .map(|info| serde_json::to_value(info).unwrap_or(JsonValue::Null))
            .collect();
        vec![json!({
            "ev": "peers",
            "meta": {
                "action": "list",
                "peers": peers,
            }
        })]
    }

    fn handle_noetic_quorum(&self, quorum: f32) -> Vec<JsonValue> {
        let new_quorum = {
            let mut guard = self.noetic.lock().expect("noetic mutex poisoned");
            guard.set_quorum(quorum.clamp(0.0, 1.0))
        };
        vec![json!({
            "ev": "noetic",
            "meta": {
                "phase": "quorum",
                "q": new_quorum,
            }
        })]
    }

    fn handle_noetic_propose(
        &self,
        obj: NoeticObjectKind,
        data: JsonValue,
    ) -> Result<Vec<JsonValue>, String> {
        let mut guard = self.noetic.lock().expect("noetic mutex poisoned");
        let mut events = Vec::new();
        match obj {
            NoeticObjectKind::Model => {
                let local_peer = guard.local_peer();
                let local_credence = guard.local_credence();
                let clock = guard.tick_clock();
                let (header, topk) = build_model_proposal(&data, local_peer, clock)?;
                let event = guard
                    .consensus
                    .propose_model(local_peer, header, topk, clock);
                let proposal_id = event.meta.id;
                events.extend(guard.process_event(event));
                let vote_clock = guard.tick_clock();
                if let Some(vote_event) = guard.consensus.vote(
                    proposal_id,
                    local_peer,
                    VoteDecision::Approve,
                    local_credence,
                    vote_clock,
                ) {
                    events.extend(guard.process_event(vote_event));
                }
            }
            NoeticObjectKind::Seed => {
                let local_peer = guard.local_peer();
                let local_credence = guard.local_credence();
                let clock = guard.tick_clock();
                let (header, status) = build_seed_proposal(&data, local_peer, clock)?;
                let event = guard
                    .consensus
                    .propose_seed(local_peer, header, status, clock);
                let proposal_id = event.meta.id;
                events.extend(guard.process_event(event));
                let vote_clock = guard.tick_clock();
                if let Some(vote_event) = guard.consensus.vote(
                    proposal_id,
                    local_peer,
                    VoteDecision::Approve,
                    local_credence,
                    vote_clock,
                ) {
                    events.extend(guard.process_event(vote_event));
                }
            }
            NoeticObjectKind::Policy => {
                let local_peer = guard.local_peer();
                let local_credence = guard.local_credence();
                let clock = guard.tick_clock();
                let bundle = build_policy_bundle(&data, clock)?;
                let event = guard.consensus.propose_policy(local_peer, bundle, clock);
                let proposal_id = event.meta.id;
                events.extend(guard.process_event(event));
                let vote_clock = guard.tick_clock();
                if let Some(vote_event) = guard.consensus.vote(
                    proposal_id,
                    local_peer,
                    VoteDecision::Approve,
                    local_credence,
                    vote_clock,
                ) {
                    events.extend(guard.process_event(vote_event));
                }
            }
        }
        Ok(events)
    }
}

/// Initializes the global bridge from a CBOR configuration buffer.
///
/// # Safety
///
/// When `len` is non-zero, `cfg_cbor` must be non-null, properly aligned, and
/// point to `len` readable bytes that remain valid for the duration of this call.
#[no_mangle]
pub unsafe extern "C" fn liminal_init(cfg_cbor: *const u8, len: usize) -> bool {
    if cfg_cbor.is_null() || len == 0 {
        return false;
    }
    let bytes = unsafe { slice::from_raw_parts(cfg_cbor, len) };
    let config: BridgeConfig = match serde_cbor::from_slice(bytes) {
        Ok(cfg) => cfg,
        Err(_) => return false,
    };
    let state = match BridgeState::new(config) {
        Ok(state) => state,
        Err(_) => return false,
    };
    STATE.set(state).is_ok()
}

/// Pushes one CBOR-encoded protocol message into the bridge.
///
/// # Safety
///
/// When `len` is non-zero, `msg_cbor` must be non-null, properly aligned, and
/// point to `len` readable bytes that remain valid for the duration of this call.
#[no_mangle]
pub unsafe extern "C" fn liminal_push(msg_cbor: *const u8, len: usize) -> usize {
    if msg_cbor.is_null() || len == 0 {
        return 0;
    }
    let bytes = unsafe { slice::from_raw_parts(msg_cbor, len) };
    let message: ProtocolPush = match serde_cbor::from_slice(bytes) {
        Ok(msg) => msg,
        Err(_) => return 0,
    };

    let Some(state) = STATE.get() else {
        return 0;
    };

    match message {
        ProtocolPush::Impulse(impulse) => {
            let core_impulse = impulse.to_core();
            let logs = state.runtime.block_on({
                let field = state.field.clone();
                async move {
                    let mut guard = field.lock().await;
                    guard.route_impulse(core_impulse)
                }
            });

            if let Ok(mut outbox) = state.outbox.lock() {
                for log in logs {
                    if let Some(event) = event_from_impulse_log(&log) {
                        outbox.push_event(event);
                    }
                }
            }
        }
        ProtocolPush::Command(command) => match command {
            ProtocolCommand::TrsSet { cfg } => {
                let event_string = state.runtime.block_on({
                    let field = state.field.clone();
                    async move {
                        let mut guard = field.lock().await;
                        guard.set_trs_config(cfg)
                    }
                });
                if let Ok(mut outbox) = state.outbox.lock() {
                    if let Some(event) = event_from_field_log(&event_string, 1_000) {
                        outbox.push_event(event);
                    }
                }
            }
            ProtocolCommand::TrsTarget { value } => {
                let event_string = state.runtime.block_on({
                    let field = state.field.clone();
                    async move {
                        let mut guard = field.lock().await;
                        guard.set_trs_target(value)
                    }
                });
                if let Ok(mut outbox) = state.outbox.lock() {
                    if let Some(event) = event_from_field_log(&event_string, 1_000) {
                        outbox.push_event(event);
                    }
                }
            }
            ProtocolCommand::Lql { query } => {
                let mut events: Vec<String> = Vec::new();
                match parse_lql(&query) {
                    Ok(ast) => {
                        let outcome = state.runtime.block_on({
                            let field = state.field.clone();
                            async move {
                                let mut guard = field.lock().await;
                                guard.exec_lql(ast)
                            }
                        });
                        match outcome {
                            Ok(result) => {
                                events.extend(result.events);
                            }
                            Err(err) => {
                                events.push(
                                    json!({
                                        "ev": "lql",
                                        "meta": {
                                            "error": err.to_string(),
                                            "query": query,
                                        }
                                    })
                                    .to_string(),
                                );
                            }
                        }
                    }
                    Err(err) => {
                        events.push(
                            json!({
                                "ev": "lql",
                                "meta": {
                                    "error": err.to_string(),
                                    "query": query,
                                }
                            })
                            .to_string(),
                        );
                    }
                }
                if let Ok(mut outbox) = state.outbox.lock() {
                    for event_string in events {
                        if let Some(event) = event_from_field_log(&event_string, 1_000) {
                            outbox.push_event(event);
                        }
                    }
                }
            }
            ProtocolCommand::DreamNow => {
                let report_opt = state.runtime.block_on({
                    let field = state.field.clone();
                    async move { field.lock().await.run_dream_cycle() }
                });
                if let Ok(mut outbox) = state.outbox.lock() {
                    match report_opt {
                        Some(report) => {
                            let summary = format!(
                                "DREAM strengthened={} weakened={} pruned={} rewired={} protected={} took={}ms",
                                report.strengthened,
                                report.weakened,
                                report.pruned,
                                report.rewired,
                                report.protected,
                                report.took_ms
                            );
                            if let Some(event) = event_from_field_log(&summary, 1_000) {
                                outbox.push_event(event);
                            }
                            let payload = json!({
                                "ev": "dream",
                                "meta": {
                                    "strengthened": report.strengthened,
                                    "weakened": report.weakened,
                                    "pruned": report.pruned,
                                    "rewired": report.rewired,
                                    "protected": report.protected,
                                    "took_ms": report.took_ms
                                }
                            })
                            .to_string();
                            if let Some(event) = event_from_field_log(&payload, 1_000) {
                                outbox.push_event(event);
                            }
                        }
                        None => {
                            let msg = "DREAM no-op (not enough activity)";
                            if let Some(event) = event_from_field_log(msg, 1_000) {
                                outbox.push_event(event);
                            }
                        }
                    }
                }
            }
            ProtocolCommand::DreamSet { cfg } => {
                let updated = state.runtime.block_on({
                    let field = state.field.clone();
                    let cfg = cfg.clone();
                    async move {
                        let mut guard = field.lock().await;
                        guard.set_dream_config(cfg);
                        guard.dream_config()
                    }
                });
                if let Ok(mut outbox) = state.outbox.lock() {
                    let payload = json!({
                        "ev": "dream_config",
                        "meta": {
                            "min_idle_s": updated.min_idle_s,
                            "window_ms": updated.window_ms,
                            "strengthen_top_pct": updated.strengthen_top_pct,
                            "weaken_bottom_pct": updated.weaken_bottom_pct,
                            "protect_salience": updated.protect_salience,
                            "adreno_protect": updated.adreno_protect,
                            "max_ops_per_cycle": updated.max_ops_per_cycle
                        }
                    })
                    .to_string();
                    if let Some(event) = event_from_field_log(&payload, 1_000) {
                        outbox.push_event(event);
                    }
                }
            }
            ProtocolCommand::DreamGet => {
                let cfg = state.runtime.block_on({
                    let field = state.field.clone();
                    async move { field.lock().await.dream_config() }
                });
                if let Ok(mut outbox) = state.outbox.lock() {
                    let payload = json!({
                        "ev": "dream_config",
                        "meta": {
                            "min_idle_s": cfg.min_idle_s,
                            "window_ms": cfg.window_ms,
                            "strengthen_top_pct": cfg.strengthen_top_pct,
                            "weaken_bottom_pct": cfg.weaken_bottom_pct,
                            "protect_salience": cfg.protect_salience,
                            "adreno_protect": cfg.adreno_protect,
                            "max_ops_per_cycle": cfg.max_ops_per_cycle
                        }
                    })
                    .to_string();
                    if let Some(event) = event_from_field_log(&payload, 1_000) {
                        outbox.push_event(event);
                    }
                }
            }
            ProtocolCommand::SyncNow => {
                let report_opt = state.runtime.block_on({
                    let field = state.field.clone();
                    async move {
                        let mut guard = field.lock().await;
                        let cfg = guard.sync_config();
                        let now_ms = guard.now_ms;
                        let groups = detect_sync_groups(&*guard, &cfg, now_ms);
                        if groups.is_empty() {
                            None
                        } else {
                            Some(run_collective_dream(&mut *guard, &groups, &cfg, now_ms))
                        }
                    }
                });
                if let Ok(mut outbox) = state.outbox.lock() {
                    match report_opt {
                        Some(report) => {
                            let summary = format!(
                                "COLLECTIVE_DREAM groups={} shared={} aligned={} protected={} took={}ms",
                                report.groups, report.shared, report.aligned, report.protected, report.took_ms
                            );
                            if let Some(event) = event_from_field_log(&summary, 1_000) {
                                outbox.push_event(event);
                            }
                            let payload = json!({
                                "ev": "collective_dream",
                                "meta": {
                                    "groups": report.groups,
                                    "shared": report.shared,
                                    "aligned": report.aligned,
                                    "protected": report.protected,
                                    "took_ms": report.took_ms
                                }
                            })
                            .to_string();
                            if let Some(event) = event_from_field_log(&payload, 1_000) {
                                outbox.push_event(event);
                            }
                        }
                        None => {
                            let msg = "COLLECTIVE_DREAM no-op (no qualifying groups)";
                            if let Some(event) = event_from_field_log(msg, 1_000) {
                                outbox.push_event(event);
                            }
                        }
                    }
                }
            }
            ProtocolCommand::SyncSet { cfg } => {
                let updated = state.runtime.block_on({
                    let field = state.field.clone();
                    let cfg = cfg.clone();
                    async move {
                        let mut guard = field.lock().await;
                        guard.set_sync_config(cfg);
                        guard.sync_config()
                    }
                });
                if let Ok(mut outbox) = state.outbox.lock() {
                    let payload = json!({
                        "ev": "sync_config",
                        "meta": {
                            "phase_len_ms": updated.phase_len_ms,
                            "phase_gap_ms": updated.phase_gap_ms,
                            "cooccur_threshold": updated.cooccur_threshold,
                            "max_groups": updated.max_groups,
                            "share_top_k": updated.share_top_k,
                            "weight_xfer": updated.weight_xfer
                        }
                    })
                    .to_string();
                    if let Some(event) = event_from_field_log(&payload, 1_000) {
                        outbox.push_event(event);
                    }
                }
            }
            ProtocolCommand::SyncGet => {
                let cfg = state.runtime.block_on({
                    let field = state.field.clone();
                    async move { field.lock().await.sync_config() }
                });
                if let Ok(mut outbox) = state.outbox.lock() {
                    let payload = json!({
                        "ev": "sync_config",
                        "meta": {
                            "phase_len_ms": cfg.phase_len_ms,
                            "phase_gap_ms": cfg.phase_gap_ms,
                            "cooccur_threshold": cfg.cooccur_threshold,
                            "max_groups": cfg.max_groups,
                            "share_top_k": cfg.share_top_k,
                            "weight_xfer": cfg.weight_xfer
                        }
                    })
                    .to_string();
                    if let Some(event) = event_from_field_log(&payload, 1_000) {
                        outbox.push_event(event);
                    }
                }
            }
            ProtocolCommand::AwakenSet { cfg } => {
                let event = state.runtime.block_on({
                    let field = state.field.clone();
                    let cfg = cfg.clone();
                    async move {
                        let mut guard = field.lock().await;
                        guard.set_awakening_config(cfg.clone());
                        awaken_config_event(&cfg, "set")
                    }
                });
                if let Ok(mut outbox) = state.outbox.lock() {
                    push_json_event(&mut outbox, event);
                }
            }
            ProtocolCommand::AwakenGet => {
                let event = state.runtime.block_on({
                    let field = state.field.clone();
                    async move {
                        let cfg = field.lock().await.awakening_config();
                        awaken_config_event(&cfg, "cfg")
                    }
                });
                if let Ok(mut outbox) = state.outbox.lock() {
                    push_json_event(&mut outbox, event);
                }
            }
            ProtocolCommand::MirrorTimeline { top } => {
                let mut timeline = state.runtime.block_on({
                    let field = state.field.clone();
                    async move { field.lock().await.mirror_timeline() }
                });
                if let Some(limit) = top.map(|v| v as usize) {
                    if limit == 0 {
                        timeline.epochs.clear();
                    } else if timeline.epochs.len() > limit {
                        let remove = timeline.epochs.len() - limit;
                        timeline.epochs.drain(0..remove);
                    }
                }
                if let Ok(mut outbox) = state.outbox.lock() {
                    let payload = json!({
                        "ev": "mirror.timeline",
                        "meta": {
                            "built_ms": timeline.built_ms,
                            "epochs": timeline.epochs,
                        }
                    });
                    push_json_event(&mut outbox, payload);
                }
            }
            ProtocolCommand::MirrorInfluencers { k } => {
                let limit = k.unwrap_or(10).min(u32::MAX) as usize;
                let epochs = state.runtime.block_on({
                    let field = state.field.clone();
                    async move { field.lock().await.mirror_top_influencers(limit) }
                });
                if let Ok(mut outbox) = state.outbox.lock() {
                    let payload = json!({
                        "ev": "mirror.influencers",
                        "meta": {"epochs": epochs},
                    });
                    push_json_event(&mut outbox, payload);
                }
            }
            ProtocolCommand::MirrorReplay { epoch_id, cfg } => {
                let cfg_value: ReplayConfig = cfg.unwrap_or_default();
                let report = state.runtime.block_on({
                    let field = state.field.clone();
                    let cfg_clone = cfg_value.clone();
                    async move {
                        let mut guard = field.lock().await;
                        replay_epoch_by_id(&mut *guard, epoch_id, &cfg_clone)
                    }
                });
                if let Ok(mut outbox) = state.outbox.lock() {
                    let payload = json!({
                        "ev": "replay",
                        "meta": {
                            "epoch_id": report.epoch_id,
                            "mode": report.mode,
                            "applied": report.applied,
                            "predicted_gain": report.predicted_gain,
                            "took_ms": report.took_ms,
                            "cfg": cfg_value,
                        }
                    });
                    push_json_event(&mut outbox, payload);
                }
            }
            ProtocolCommand::PeerAdd { peer } => {
                let result = state.handle_peer_add(peer);
                if let Ok(mut outbox) = state.outbox.lock() {
                    match result {
                        Ok(events) => {
                            for event in events {
                                push_json_event(&mut outbox, event);
                            }
                        }
                        Err(err) => {
                            push_json_event(
                                &mut outbox,
                                json!({
                                    "ev": "peers",
                                    "meta": {
                                        "action": "error",
                                        "reason": err,
                                    }
                                }),
                            );
                        }
                    }
                }
            }
            ProtocolCommand::PeerList => {
                if let Ok(mut outbox) = state.outbox.lock() {
                    for event in state.handle_peer_list() {
                        push_json_event(&mut outbox, event);
                    }
                }
            }
            ProtocolCommand::NoeticPropose { obj, data } => {
                let result = state.handle_noetic_propose(obj, data);
                if let Ok(mut outbox) = state.outbox.lock() {
                    match result {
                        Ok(events) => {
                            for event in events {
                                push_json_event(&mut outbox, event);
                            }
                        }
                        Err(err) => {
                            push_json_event(
                                &mut outbox,
                                json!({
                                    "ev": "noetic",
                                    "meta": {
                                        "phase": "error",
                                        "reason": err,
                                    }
                                }),
                            );
                        }
                    }
                }
            }
            ProtocolCommand::NoeticQuorum { q } => {
                if let Ok(mut outbox) = state.outbox.lock() {
                    for event in state.handle_noetic_quorum(q) {
                        push_json_event(&mut outbox, event);
                    }
                }
            }
            ProtocolCommand::Introspect(IntrospectRequest { target, top }) => {
                let event = state.runtime.block_on({
                    let field = state.field.clone();
                    async move {
                        let mut guard = field.lock().await;
                        build_introspect_event(&mut *guard, target, top)
                    }
                });
                if let Ok(mut outbox) = state.outbox.lock() {
                    push_json_event(&mut outbox, event);
                }
            }
        },
    }

    len
}

fn push_json_event(outbox: &mut Outbox, payload: JsonValue) {
    let serialized = payload.to_string();
    if let Some(event) = event_from_field_log(&serialized, 1_000) {
        outbox.push_event(event);
    }
}

fn awaken_config_event(cfg: &AwakeningConfig, action: &str) -> JsonValue {
    let cfg_json = serde_json::to_value(cfg).unwrap_or(JsonValue::Null);
    json!({
        "ev": "awaken",
        "meta": {
            "action": action,
            "cfg": cfg_json
        }
    })
}

fn awaken_report_event(report: &AwakeningReport) -> JsonValue {
    json!({
        "ev": "awaken",
        "meta": {
            "action": "now",
            "applied": report.applied,
            "protected": report.protected,
            "avg_tension": report.avg_tension,
            "tick_adjust_ms": report.tick_adjust_ms,
            "energy_delta": report.energy_delta
        }
    })
}

fn ensure_resonant_model(field: &mut ClusterField) -> ResonantModel {
    field
        .rebuild_resonant_model()
        .or_else(|| field.resonant_model().cloned())
        .unwrap_or_default()
}

fn build_introspect_event(
    field: &mut ClusterField,
    target: IntrospectTarget,
    top: Option<u32>,
) -> JsonValue {
    let limit = top.unwrap_or(5).max(1).min(32) as usize;
    match target {
        IntrospectTarget::Awaken => awaken_report_event(&field.awaken_now()),
        IntrospectTarget::Model => {
            let model = ensure_resonant_model(field);
            json!({
                "ev": "introspect",
                "meta": {
                    "target": "model",
                    "coherence": model.coherence,
                    "edges": model.edges.len(),
                    "influences": model.influences.len(),
                    "tensions": model.tensions.len(),
                    "top_nodes": top_nodes(field, limit),
                    "requested_top": limit as u32
                }
            })
        }
        IntrospectTarget::Influence => {
            let model = ensure_resonant_model(field);
            json!({
                "ev": "introspect",
                "meta": {
                    "target": "influence",
                    "items": top_influences(&model, limit),
                    "requested_top": limit as u32
                }
            })
        }
        IntrospectTarget::Tension => {
            let model = ensure_resonant_model(field);
            json!({
                "ev": "introspect",
                "meta": {
                    "target": "tension",
                    "items": top_tensions(&model, limit),
                    "requested_top": limit as u32
                }
            })
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn generate_peer_id() -> PeerId {
    let mut rng = OsRng;
    let mut bytes = [0u8; 32];
    rng.fill_bytes(&mut bytes);
    PeerId::new(bytes)
}

fn derive_peer_id(url: &str, namespace: &str) -> PeerId {
    let mut hasher = Hasher::new();
    hasher.update(url.as_bytes());
    hasher.update(namespace.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(digest.as_bytes());
    PeerId::new(bytes)
}

fn build_merge_event(summary: &Summary) -> JsonValue {
    json!({
        "ev": "crdt_merge",
        "meta": {
            "peer": summary.peer,
            "digest": summary.digest,
            "models": summary.model_count,
            "epochs": summary.epoch_count,
            "seeds": summary.seed_count,
            "topk_clock": {
                "peer": summary.topk_clock.node,
                "phys_ms": summary.topk_clock.phys_ms,
                "log": summary.topk_clock.log,
            },
            "policy_clock": {
                "peer": summary.policy_clock.node,
                "phys_ms": summary.policy_clock.phys_ms,
                "log": summary.policy_clock.log,
            },
            "credence_sum": summary.credence_sum,
        }
    })
}

fn build_model_proposal(
    data: &JsonValue,
    local_peer: PeerId,
    clock: Hlc,
) -> Result<(ResonantModelHeader, ResonantModelTopK), String> {
    let model: ResonantModel = serde_json::from_value(data.clone()).map_err(|e| e.to_string())?;
    let serialized = serde_json::to_vec(&model).map_err(|e| e.to_string())?;
    let mut hasher = Hasher::new();
    hasher.update(&serialized);
    let digest = hasher.finalize().to_hex().to_string();
    let header = ResonantModelHeader {
        id: digest.clone(),
        version: clock,
        hash: digest,
        source: local_peer,
    };
    let entries = model_entries(&model);
    let topk = ResonantModelTopK::new(entries, clock);
    Ok((header, topk))
}

fn model_entries(model: &ResonantModel) -> Vec<String> {
    let mut entries = Vec::new();
    for edge in model.edges.iter().take(8) {
        entries.push(format!(
            "edge:{}->{}:{:.3}:{:.3}",
            edge.from, edge.to, edge.weight, edge.coherence
        ));
    }
    for influence in model.influences.iter().take(8) {
        entries.push(format!(
            "influence:{}->{}:{:.3}:{:.3}",
            influence.source, influence.sink, influence.weight, influence.coherence
        ));
    }
    for tension in model.tensions.iter().take(8) {
        entries.push(format!(
            "tension:{}:{:.3}:{:.3}",
            tension.node, tension.magnitude, tension.relief
        ));
    }
    entries
}

fn build_seed_proposal(
    data: &JsonValue,
    local_peer: PeerId,
    clock: Hlc,
) -> Result<(SeedHeader, SeedStatus), String> {
    let id = data
        .get("id")
        .and_then(|v| {
            v.as_str()
                .map(|s| s.to_string())
                .or_else(|| v.as_u64().map(|v| v.to_string()))
        })
        .ok_or_else(|| "seed.id missing".to_string())?;
    let epoch = data.get("epoch").and_then(|v| v.as_u64()).unwrap_or(0);
    let status = parse_seed_status(data)?;
    let header = SeedHeader {
        id,
        epoch,
        version: clock,
        source: local_peer,
    };
    Ok((header, status))
}

fn parse_seed_status(data: &JsonValue) -> Result<SeedStatus, String> {
    let status = data
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("pending")
        .to_ascii_lowercase();
    match status.as_str() {
        "pending" => Ok(SeedStatus::Pending),
        "active" => {
            let started = data
                .get("started_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or_else(now_ms);
            Ok(SeedStatus::Active {
                started_ms: started,
            })
        }
        "committed" => {
            let finished = data
                .get("finished_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or_else(now_ms);
            Ok(SeedStatus::Committed {
                finished_ms: finished,
            })
        }
        "failed" => {
            let error = data
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown failure")
                .to_string();
            Ok(SeedStatus::Failed { error })
        }
        "observed" => {
            let note = data
                .get("note")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Ok(SeedStatus::Observed { note })
        }
        other => Err(format!("unsupported seed status: {other}")),
    }
}

fn build_policy_bundle(data: &JsonValue, clock: Hlc) -> Result<PolicyBundle, String> {
    let payload = data.clone();
    let serialized = serde_json::to_vec(&payload).map_err(|e| e.to_string())?;
    let mut hasher = Hasher::new();
    hasher.update(&serialized);
    let hash = hasher.finalize().to_hex().to_string();
    Ok(PolicyBundle {
        version: clock,
        hash,
        payload,
    })
}

fn top_nodes(field: &ClusterField, limit: usize) -> Vec<JsonValue> {
    let mut nodes: Vec<_> = field.cells.values().collect();
    nodes.sort_by(|a, b| {
        b.salience
            .partial_cmp(&a.salience)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
    nodes.truncate(limit);
    nodes
        .into_iter()
        .map(|cell| {
            json!({
                "id": cell.id,
                "pattern": cell.seed.core_pattern,
                "salience": cell.salience,
                "energy": cell.energy
            })
        })
        .collect()
}

fn top_influences(model: &ResonantModel, limit: usize) -> Vec<JsonValue> {
    let mut influences: Vec<Influence> = model.influences.clone();
    influences.sort_by(|a, b| {
        b.weight
            .abs()
            .partial_cmp(&a.weight.abs())
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.source.cmp(&b.source))
    });
    influences.truncate(limit);
    influences
        .into_iter()
        .map(|inf| {
            json!({
                "source": inf.source,
                "sink": inf.sink,
                "weight": inf.weight,
                "coherence": inf.coherence
            })
        })
        .collect()
}

fn top_tensions(model: &ResonantModel, limit: usize) -> Vec<JsonValue> {
    let mut tensions: Vec<Tension> = model.tensions.clone();
    tensions.sort_by(|a, b| {
        b.magnitude
            .partial_cmp(&a.magnitude)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.node.cmp(&b.node))
    });
    tensions.truncate(limit);
    tensions
        .into_iter()
        .map(|ten| {
            json!({
                "node": ten.node,
                "magnitude": ten.magnitude,
                "relief": ten.relief
            })
        })
        .collect()
}

/// Copies the next CBOR-encoded package into a caller-owned output buffer.
///
/// # Safety
///
/// When `cap` is non-zero, `out` must be non-null, properly aligned, and point
/// to `cap` writable bytes. The writable range must not overlap bridge-owned
/// memory and must remain valid for the duration of this call.
#[no_mangle]
pub unsafe extern "C" fn liminal_pull(out: *mut u8, cap: usize) -> usize {
    if out.is_null() || cap == 0 {
        return 0;
    }
    let Some(state) = STATE.get() else {
        return 0;
    };

    let mut guard = match state.outbox.lock() {
        Ok(guard) => guard,
        Err(_) => return 0,
    };

    let Some(package) = guard.take() else {
        return 0;
    };

    let encoded = match serde_cbor::to_vec(&package) {
        Ok(bytes) => bytes,
        Err(_) => {
            guard.restore(package);
            return 0;
        }
    };

    if encoded.len() > cap {
        guard.restore(package);
        return 0;
    }

    unsafe {
        std::ptr::copy_nonoverlapping(encoded.as_ptr(), out, encoded.len());
    }
    encoded.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{BridgeConfig, ProtocolImpulse, ProtocolPackage};

    #[test]
    fn ffi_flow() {
        let cfg = BridgeConfig {
            tick_ms: 200,
            store_path: None,
            snap_interval: None,
            snap_maxwal: None,
            ws_port: None,
            ws_format: None,
        };
        let cfg_bytes = serde_cbor::to_vec(&cfg).unwrap();
        assert!(unsafe { liminal_init(cfg_bytes.as_ptr(), cfg_bytes.len()) });

        let impulse = ProtocolImpulse {
            kind: 0,
            pattern: "cpu/load".into(),
            strength: 0.8,
            ttl_ms: 900,
            tags: vec!["test".into()],
        };
        let imp_bytes = serde_cbor::to_vec(&impulse).unwrap();
        assert_eq!(
            unsafe { liminal_push(imp_bytes.as_ptr(), imp_bytes.len()) },
            imp_bytes.len()
        );

        std::thread::sleep(std::time::Duration::from_millis(250));

        let mut buffer = vec![0u8; 1024];
        let written = unsafe { liminal_pull(buffer.as_mut_ptr(), buffer.len()) };
        assert!(written > 0);
        let package: ProtocolPackage = serde_cbor::from_slice(&buffer[..written]).unwrap();
        assert!(package.metrics.is_some() || !package.events.is_empty());
    }
}
