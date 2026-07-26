mod protocol;

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, Stream, StreamExt};
use serde_json::{json, Value as JsonValue};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, Mutex, Notify};
use tokio::time::{sleep, Instant};
use tokio_tungstenite::{connect_async, MaybeTlsStream};
use tungstenite::protocol::Message;
use url::Url;

use protocol::{CommandEnvelope, EventEnvelope};

type WsStream = tokio_tungstenite::WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Debug, Clone)]
pub struct BackoffPolicy {
    pub min: Duration,
    pub max: Duration,
    pub factor: f32,
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self {
            min: Duration::from_millis(250),
            max: Duration::from_secs(8),
            factor: 2.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClientOptions {
    pub url: Url,
    pub key_id: Option<String>,
    pub secret: Option<String>,
    pub namespace: Option<String>,
    pub reconnect: bool,
    pub backoff: BackoffPolicy,
    pub buffer_limit: usize,
    pub heartbeat: Duration,
    pub qps_limit: u32,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            url: Url::parse("ws://localhost:7777/ws").expect("valid default url"),
            key_id: None,
            secret: None,
            namespace: None,
            reconnect: true,
            backoff: BackoffPolicy::default(),
            buffer_limit: 256,
            heartbeat: Duration::from_secs(15),
            qps_limit: 50,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClientMetrics {
    pub queue_depth: usize,
    pub sent_commands: u64,
    pub received_events: u64,
    pub reconnect_attempts: u64,
    pub state: ClientState,
}

impl Default for ClientMetrics {
    fn default() -> Self {
        Self {
            queue_depth: 0,
            sent_commands: 0,
            received_events: 0,
            reconnect_attempts: 0,
            state: ClientState::Disconnected,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientState {
    Connecting,
    Connected,
    Disconnected,
}

pub struct LiminalClient {
    opts: ClientOptions,
    queue: Arc<Mutex<VecDeque<CommandEnvelope>>>,
    notify: Arc<Notify>,
    metrics: Arc<Mutex<ClientMetrics>>,
    event_tx: broadcast::Sender<EventEnvelope>,
}

impl LiminalClient {
    pub async fn connect(opts: ClientOptions) -> anyhow::Result<Self> {
        let queue = Arc::new(Mutex::new(VecDeque::with_capacity(opts.buffer_limit)));
        let metrics = Arc::new(Mutex::new(ClientMetrics::default()));
        let notify = Arc::new(Notify::new());
        let (event_tx, _) = broadcast::channel::<EventEnvelope>(opts.buffer_limit);

        let client = Self {
            opts: opts.clone(),
            queue: queue.clone(),
            notify: notify.clone(),
            metrics: metrics.clone(),
            event_tx: event_tx.clone(),
        };

        tokio::spawn(run_connection(opts, queue, notify, metrics, event_tx));
        Ok(client)
    }

    pub async fn send(&self, command: CommandEnvelope) {
        let mut queue = self.queue.lock().await;
        if queue.len() >= self.opts.buffer_limit {
            queue.pop_front();
        }
        queue.push_back(command);
        self.metrics.lock().await.queue_depth = queue.len();
        self.notify.notify_one();
    }

    pub async fn metrics(&self) -> ClientMetrics {
        self.metrics.lock().await.clone()
    }

    pub fn events(&self) -> impl Stream<Item = EventEnvelope> {
        let rx = self.event_tx.subscribe();
        tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|item| async move { item.ok() })
    }

    pub async fn auth(&self) {
        if let (Some(key_id), Some(secret)) = (&self.opts.key_id, &self.opts.secret) {
            self.send(CommandEnvelope::AuthCommand(protocol::AuthCommand {
                key_id: key_id.clone(),
                namespace: self.opts.namespace.clone(),
                op: "auth".to_string(),
                secret: secret.clone(),
            }))
            .await;
        }
    }

    pub async fn lql(&self, query: &str, parameters: Option<JsonValue>) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let command = CommandEnvelope::LqlCommand(protocol::LqlCommand {
            id: id.clone(),
            op: "lql".into(),
            parameters,
            query: query.to_string(),
        });
        self.send(command).await;
        id
    }

    pub async fn subscribe(&self, pattern: &str, options: Option<JsonValue>) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let command = CommandEnvelope::SubscribeCommand(protocol::SubscribeCommand {
            id: id.clone(),
            op: "subscribe".into(),
            options,
            pattern: pattern.to_string(),
        });
        self.send(command).await;
        id
    }

    pub async fn unsubscribe(&self, id: &str) {
        self.send(CommandEnvelope::UnsubscribeCommand(
            protocol::UnsubscribeCommand {
                id: id.to_string(),
                op: "unsubscribe".into(),
            },
        ))
        .await;
    }

    pub async fn intent_text(&self, text: &str, lang: Option<&str>) {
        self.send(CommandEnvelope::IntentTextCommand(
            protocol::IntentTextCommand {
                lang: lang.map(|s| s.to_string()),
                op: "intent.text".into(),
                text: text.to_string(),
            },
        ))
        .await;
    }

    pub async fn intent(&self, kind: &str, payload: JsonValue) {
        self.send(CommandEnvelope::IntentInvokeCommand(
            protocol::IntentInvokeCommand {
                kind: kind.to_string(),
                op: "intent".into(),
                payload,
            },
        ))
        .await;
    }

    pub async fn seed_plant(&self, spec: JsonValue) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        self.send(CommandEnvelope::SeedPlantCommand(
            protocol::SeedPlantCommand {
                id: id.clone(),
                op: "seed.plant".into(),
                spec,
            },
        ))
        .await;
        id
    }

    pub async fn seed_abort(&self, id: &str, reason: Option<&str>) {
        self.send(CommandEnvelope::SeedAbortCommand(
            protocol::SeedAbortCommand {
                id: id.to_string(),
                op: "seed.abort".into(),
                reason: reason.map(|s| s.to_string()),
            },
        ))
        .await;
    }

    pub async fn seed_garden(&self) {
        self.send(CommandEnvelope::SeedGardenCommand(
            protocol::SeedGardenCommand {
                op: "seed.garden".into(),
            },
        ))
        .await;
    }

    pub async fn dream_now(&self, intensity: Option<f64>) {
        self.send(CommandEnvelope::DreamNowCommand(
            protocol::DreamNowCommand {
                intensity,
                op: "dream.now".into(),
            },
        ))
        .await;
    }

    pub async fn sync_now(&self, scope: Option<&str>) {
        self.send(CommandEnvelope::SyncNowCommand(protocol::SyncNowCommand {
            op: "sync.now".into(),
            scope: scope.map(|s| s.to_string()),
        }))
        .await;
    }

    pub async fn awaken_now(&self) {
        self.send(CommandEnvelope::AwakenNowCommand(
            protocol::AwakenNowCommand {
                op: "awaken.now".into(),
            },
        ))
        .await;
    }

    pub async fn mirror_timeline(&self, from: &str, to: &str) {
        let range = json!({
            "from": from,
            "to": to,
        });
        self.send(CommandEnvelope::MirrorTimelineCommand(
            protocol::MirrorTimelineCommand {
                op: "mirror.timeline".into(),
                range,
            },
        ))
        .await;
    }

    pub async fn mirror_replay(&self, cursor: Option<&str>) {
        self.send(CommandEnvelope::MirrorReplayCommand(
            protocol::MirrorReplayCommand {
                cursor: cursor.map(|s| s.to_string()),
                op: "mirror.replay".into(),
            },
        ))
        .await;
    }

    pub async fn noetic_propose(&self, proposal: JsonValue) {
        self.send(CommandEnvelope::NoeticProposeCommand(
            protocol::NoeticProposeCommand {
                op: "noetic.propose".into(),
                proposal,
            },
        ))
        .await;
    }

    pub async fn peers_add(&self, peer: JsonValue) {
        self.send(CommandEnvelope::PeerAddCommand(protocol::PeerAddCommand {
            op: "peers.add".into(),
            peer,
        }))
        .await;
    }

    pub async fn peers_list(&self) {
        self.send(CommandEnvelope::PeerListCommand(
            protocol::PeerListCommand {
                op: "peers.list".into(),
            },
        ))
        .await;
    }
}

async fn run_connection(
    opts: ClientOptions,
    queue: Arc<Mutex<VecDeque<CommandEnvelope>>>,
    notify: Arc<Notify>,
    metrics: Arc<Mutex<ClientMetrics>>,
    event_tx: broadcast::Sender<EventEnvelope>,
) {
    let mut backoff = opts.backoff.min;
    loop {
        {
            let mut guard = metrics.lock().await;
            guard.state = ClientState::Connecting;
        }
        match connect_async(opts.url.clone()).await {
            Ok((mut ws, _)) => {
                {
                    let mut guard = metrics.lock().await;
                    guard.state = ClientState::Connected;
                }
                if let (Some(key_id), Some(secret)) = (&opts.key_id, &opts.secret) {
                    let auth = CommandEnvelope::AuthCommand(protocol::AuthCommand {
                        key_id: key_id.clone(),
                        namespace: opts.namespace.clone(),
                        op: "auth".into(),
                        secret: secret.clone(),
                    });
                    if send_command(&mut ws, auth, &metrics).await.is_err() {
                        continue;
                    }
                }
                let heartbeat = opts.heartbeat;
                let mut last_send = Instant::now();
                let throttle = if opts.qps_limit > 0 {
                    Some(Duration::from_secs_f64(1.0 / opts.qps_limit as f64))
                } else {
                    None
                };
                loop {
                    tokio::select! {
                        _ = sleep(heartbeat) => {
                            let ping = json!({"version": "1.0.0", "command": {"op": "echo"}});
                            if ws.send(Message::Text(ping.to_string())).await.is_err() {
                                break;
                            }
                        }
                        _ = notify.notified() => {
                            let mut guard = queue.lock().await;
                            while let Some(cmd) = guard.pop_front() {
                                drop(guard);
                                if let Some(interval) = throttle {
                                    let elapsed = last_send.elapsed();
                                    if elapsed < interval {
                                        sleep(interval - elapsed).await;
                                    }
                                }
                                if send_command(&mut ws, cmd, &metrics).await.is_err() {
                                    break;
                                }
                                last_send = Instant::now();
                                guard = queue.lock().await;
                            }
                        }
                        msg = ws.next() => {
                            match msg {
                                Some(Ok(Message::Text(text))) => {
                                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                                        if let Some(event) = value.get("event") {
                                            if let Ok(event) = serde_json::from_value::<EventEnvelope>(event.clone()) {
                                                let _ = event_tx.send(event);
                                                metrics.lock().await.received_events += 1;
                                            }
                                        }
                                    }
                                }
                                Some(Ok(Message::Binary(_))) => {}
                                Some(Ok(Message::Ping(payload))) => {
                                    let _ = ws.send(Message::Pong(payload)).await;
                                }
                                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => {
                                    break;
                                }
                                _ => {}
                            }
                        }
                    }
                }
                backoff = opts.backoff.min;
            }
            Err(err) => {
                tracing::warn!(error = %err, "failed to connect to liminaldb");
                metrics.lock().await.reconnect_attempts += 1;
            }
        }

        metrics.lock().await.state = ClientState::Disconnected;
        if !opts.reconnect {
            break;
        }
        sleep(backoff).await;
        backoff = opts.backoff.max.min(Duration::from_secs_f32(
            backoff.as_secs_f32() * opts.backoff.factor,
        ));
    }
}

async fn send_command(
    ws: &mut WsStream,
    command: CommandEnvelope,
    metrics: &Arc<Mutex<ClientMetrics>>,
) -> anyhow::Result<()> {
    let payload = json!({
        "version": "1.0.0",
        "command": command,
    });
    ws.send(Message::Text(payload.to_string())).await?;
    let mut guard = metrics.lock().await;
    guard.sent_commands += 1;
    Ok(())
}

pub mod protocol_types {
    pub use crate::protocol::*;
}
