use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

use crate::cluster_field::ClusterField;

/// Rolling snapshot used by the reflex loop.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReflexSnapshot {
    pub ts_ms: u64,
    pub yield_rate: f32,
    pub harmony: f32,
    pub latency_p95: f32,
    pub impact: f32,
}

impl ReflexSnapshot {
    pub fn delta(&self, other: &Self) -> MetricsDelta {
        MetricsDelta {
            yield_delta: self.yield_rate - other.yield_rate,
            harmony_delta: self.harmony - other.harmony,
            latency_delta: self.latency_p95 - other.latency_p95,
            impact_delta: self.impact - other.impact,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MetricsDelta {
    pub yield_delta: f32,
    pub harmony_delta: f32,
    pub latency_delta: f32,
    pub impact_delta: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReflexCfg {
    pub win_ms: u64,
    pub eps: f32,
    pub pivot_step: f32,
    pub hysteresis: f32,
    pub cooldown_ms: u64,
    pub min_gain: f32,
}

impl Default for ReflexCfg {
    fn default() -> Self {
        ReflexCfg {
            win_ms: 60_000,
            eps: 0.05,
            pivot_step: 0.1,
            hysteresis: 0.02,
            cooldown_ms: 15_000,
            min_gain: 0.02,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ReflexSignal {
    Ok,
    Plateau,
    DriftDown,
    Oscillation,
    Overload,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    MicroPivot,
    Defuse,
    Dampening,
    TargetShift,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "meta")]
pub enum Action {
    MicroPivot { variant: Option<String>, step: f32 },
    Defuse { pendulum: bool },
    Dampening { ttl_ms: u64, factor: f32 },
    TargetShift { load_delta: f32 },
}

impl Action {
    pub fn kind(&self) -> ActionKind {
        match self {
            Action::MicroPivot { .. } => ActionKind::MicroPivot,
            Action::Defuse { .. } => ActionKind::Defuse,
            Action::Dampening { .. } => ActionKind::Dampening,
            Action::TargetShift { .. } => ActionKind::TargetShift,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReflexReport {
    pub sig: ReflexSignal,
    pub delta: MetricsDelta,
    pub action: Option<Action>,
    pub took_ms: u32,
}

#[derive(Debug, Default)]
pub(crate) struct RingBuf<T> {
    limit: usize,
    inner: VecDeque<T>,
}

impl<T> RingBuf<T> {
    fn with_limit(limit: usize) -> Self {
        RingBuf {
            limit,
            inner: VecDeque::with_capacity(limit),
        }
    }

    fn push(&mut self, value: T) {
        if self.inner.len() == self.limit {
            self.inner.pop_front();
        }
        self.inner.push_back(value);
    }

    fn len(&self) -> usize {
        self.inner.len()
    }

    fn as_slices(&self) -> (&[T], &[T]) {
        self.inner.as_slices()
    }

    fn iter_rev(&self) -> impl Iterator<Item = &T> {
        self.inner.iter().rev()
    }
}

pub struct ReflexContext<'a> {
    pub cfg: &'a ReflexCfg,
    pub slo_latency_ms: f32,
    pub neighbors: Vec<(String, f32)>,
    pub now_ms: u64,
}

pub struct ReflexState {
    cfg: ReflexCfg,
    enabled: bool,
    window: RingBuf<ReflexSnapshot>,
    cooldown_until: HashMap<ActionKind, u64>,
    last_report: Option<ReflexReport>,
    _eps_counter: u64,
}

impl ReflexState {
    pub fn new(cfg: ReflexCfg) -> Self {
        let capacity = ((cfg.win_ms / 1_000).max(1)) as usize * 2;
        ReflexState {
            cfg,
            enabled: false,
            window: RingBuf::with_limit(capacity),
            cooldown_until: HashMap::new(),
            last_report: None,
            _eps_counter: 0,
        }
    }

    pub fn cfg(&self) -> &ReflexCfg {
        &self.cfg
    }

    pub fn cfg_mut(&mut self) -> &mut ReflexCfg {
        &mut self.cfg
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn push_snapshot(&mut self, snapshot: ReflexSnapshot) {
        self.window.push(snapshot);
    }

    pub fn cooldown_remaining(&self, kind: ActionKind, now_ms: u64) -> Option<u64> {
        self.cooldown_until
            .get(&kind)
            .and_then(|until| until.checked_sub(now_ms))
    }

    pub fn last_report(&self) -> Option<&ReflexReport> {
        self.last_report.as_ref()
    }

    pub fn evaluate(&self) -> ReflexSignal {
        evaluate(&self.window, &self.cfg)
    }

    pub fn next_report(&mut self, ctx: ReflexContext<'_>) -> Option<ReflexReport> {
        if !self.enabled {
            return None;
        }
        let signal = self.evaluate();
        let action = propose_action(signal, ctx, &mut self.cooldown_until);
        let mut iter = self.window.iter_rev();
        let delta = match (iter.next(), iter.next()) {
            (Some(latest), Some(prev)) => latest.delta(prev),
            _ => MetricsDelta::default(),
        };
        let report = ReflexReport {
            sig: signal,
            delta,
            action,
            took_ms: 0,
        };
        self.last_report = Some(report.clone());
        Some(report)
    }

    pub fn record_action(&mut self, kind: ActionKind, now_ms: u64, duration_ms: u64) {
        let until = now_ms.saturating_add(duration_ms);
        self.cooldown_until.insert(kind, until);
    }
}

pub(crate) fn evaluate(window: &RingBuf<ReflexSnapshot>, cfg: &ReflexCfg) -> ReflexSignal {
    if window.len() < 2 {
        return ReflexSignal::Ok;
    }
    let (front, back) = window.as_slices();
    let samples: Vec<&ReflexSnapshot> = front
        .iter()
        .chain(back.iter())
        .collect();
    if samples.len() < 2 {
        return ReflexSignal::Ok;
    }
    let first = samples.first().unwrap();
    let last = samples.last().unwrap();
    let duration = (last.ts_ms.saturating_sub(first.ts_ms)).max(1) as f32 / 1_000.0;
    let yield_grad = (last.yield_rate - first.yield_rate) / duration;
    let harmony_grad = (last.harmony - first.harmony) / duration;
    let hysteresis = cfg.hysteresis;
    if yield_grad.abs() < hysteresis && harmony_grad.abs() < hysteresis {
        return ReflexSignal::Plateau;
    }
    if last.yield_rate < first.yield_rate - cfg.min_gain
        && last.impact < first.impact - cfg.min_gain
    {
        return ReflexSignal::DriftDown;
    }
    let mut sign_changes = 0u32;
    let mut last_sign = 0i8;
    let mut overload = false;
    for pair in samples.windows(2) {
        if let [left, right] = pair {
            let grad = right.yield_rate - left.yield_rate;
            let sign = if grad > hysteresis { 1 } else if grad < -hysteresis { -1 } else { 0 };
            if sign != 0 {
                if last_sign != 0 && sign != last_sign {
                    sign_changes += 1;
                }
                last_sign = sign;
            }
            if right.latency_p95 > cfg.min_gain.max(1.0) * 300.0 {
                overload = true;
            }
        }
    }
    if overload {
        return ReflexSignal::Overload;
    }
    if sign_changes >= 3 {
        return ReflexSignal::Oscillation;
    }
    ReflexSignal::Ok
}

fn propose_action(
    signal: ReflexSignal,
    ctx: ReflexContext<'_>,
    cooldowns: &mut HashMap<ActionKind, u64>,
) -> Option<Action> {
    let now_ms = ctx.now_ms;
    let cooled = |kind: ActionKind, cooldowns: &HashMap<ActionKind, u64>| -> bool {
        cooldowns
            .get(&kind)
            .map(|until| *until <= now_ms)
            .unwrap_or(true)
    };
    match signal {
        ReflexSignal::Plateau => {
            if !cooled(ActionKind::MicroPivot, cooldowns) {
                return None;
            }
            let neighbor = ctx
                .neighbors
                .iter()
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(id, _)| id.clone());
            Some(Action::MicroPivot {
                variant: neighbor,
                step: ctx.cfg.pivot_step,
            })
        }
        ReflexSignal::DriftDown => {
            if !cooled(ActionKind::Defuse, cooldowns) {
                return None;
            }
            Some(Action::Defuse { pendulum: true })
        }
        ReflexSignal::Oscillation => {
            if !cooled(ActionKind::Dampening, cooldowns) {
                return None;
            }
            Some(Action::Dampening {
                ttl_ms: ctx.cfg.cooldown_ms,
                factor: 0.75,
            })
        }
        ReflexSignal::Overload => {
            if !cooled(ActionKind::TargetShift, cooldowns) {
                return None;
            }
            Some(Action::TargetShift { load_delta: -0.05 })
        }
        ReflexSignal::Ok => None,
    }
}

pub fn apply_action(field: &mut ClusterField, now_ms: u64, action: &Action) {
    match action {
        Action::MicroPivot { step, .. } => {
            let current = field.trs.target();
            let delta = (*step).clamp(-0.2, 0.2);
            field.trs.set_target((current + delta).clamp(0.2, 0.95));
        }
        Action::Defuse { .. } => {
            field.drain_route_bias(0.1);
        }
        Action::Dampening { factor, ttl_ms } => {
            field.apply_dampening(now_ms, *factor, *ttl_ms);
        }
        Action::TargetShift { load_delta } => {
            let current = field.trs.target();
            field.trs.set_target((current + load_delta).clamp(0.2, 0.95));
        }
    }
}

pub fn cooldown_for(action: &Action, cfg: &ReflexCfg) -> u64 {
    match action.kind() {
        ActionKind::MicroPivot => cfg.cooldown_ms,
        ActionKind::Defuse => cfg.cooldown_ms * 2,
        ActionKind::Dampening => cfg.cooldown_ms,
        ActionKind::TargetShift => cfg.cooldown_ms,
    }
}
