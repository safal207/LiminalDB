# Safe `ClusterField` Refactor Plan

**Status:** implementation plan  
**Updated:** 2026-07-26

## Problem

`liminal-core::ClusterField` currently coordinates or directly owns cell storage, pattern indexing, routing history, journal access, dream and synchrony state, TRS control, views, reflexes, important-cell state, Mirror Timeline data, the resonant model, Seed Garden, and the variant layer.

This makes the type a compatibility-critical god object:

- a small change can affect unrelated runtime behavior;
- focused unit tests require constructing the whole field;
- ownership and invariants are difficult to see;
- parallel development creates conflict pressure in one file;
- the biological metaphor can hide conventional subsystem boundaries.

The first refactor phase must improve internal structure without changing public behavior, serialized formats, event ordering, command output, or durability contracts.

## Target shape

```text
ClusterField facade
├── CellRegistry
│   ├── cells
│   ├── pattern index
│   ├── IDs
│   └── important-cell membership
├── PatternRouter
│   ├── hit windows
│   ├── route usage and bias
│   ├── recent routes
│   └── routing decisions
├── AdaptiveController
│   ├── TRS
│   ├── harmony tuning
│   ├── reflex engine and feedback
│   └── temporary stress effects
├── TimelineRuntime
│   ├── journal port
│   ├── Mirror epochs
│   ├── replay reports
│   └── event emission
└── GoalRuntime
    ├── Seed Garden
    ├── variant manifold
    ├── dream reports
    └── synchrony / resonant state
```

`ClusterField` remains the public facade during the pre-1.0 compatibility phase.

## Non-negotiable invariants

The refactor must not change:

1. public method names or return types in the first phase;
2. event ordering for the same deterministic input;
3. snapshot or journal serialization;
4. node, seed, epoch, or reflex identity generation;
5. LQL command semantics;
6. default TRS, reflex, dream, or synchrony configuration;
7. WebSocket or CLI output contracts;
8. trustworthy-transition storage behavior;
9. benchmark claims without new measurements;
10. merge authority or evidence-gate requirements.

## Phase 0 — characterization

Before moving state, add characterization tests for:

- cell spawn and pattern lookup;
- impulse routing and hit retention;
- tick lifecycle and division;
- TRS adjustment;
- reflex firing and cooldown;
- Mirror epoch append and replay;
- seed plant, grow, and harvest lifecycle;
- variant-to-seed conversion;
- snapshot round trip where applicable.

Use deterministic fixtures and compare complete observable outputs, not only one internal field.

Exit gate:

```text
existing behavior
+ characterization tests
+ no production claim change
→ refactor may begin
```

## Phase 1 — extract `CellRegistry`

Move only ownership and direct operations for:

- `cells`;
- `index`;
- `next_id`;
- `important_cells`.

Keep `ClusterField` methods as delegating wrappers.

Suggested API:

```rust
pub(crate) struct CellRegistry {
    cells: HashMap<NodeId, NodeCell>,
    index: HashMap<String, Vec<NodeId>>,
    next_id: NodeId,
    important: HashSet<NodeId>,
}

impl CellRegistry {
    pub(crate) fn spawn_with_pattern(&mut self, pattern: &str) -> NodeId;
    pub(crate) fn get(&self, id: NodeId) -> Option<&NodeCell>;
    pub(crate) fn get_mut(&mut self, id: NodeId) -> Option<&mut NodeCell>;
    pub(crate) fn candidates(&self, pattern: &str) -> &[NodeId];
}
```

Safety checks:

- preserve ID sequence;
- preserve pattern-index insertion order;
- do not expose mutable maps publicly;
- keep snapshot conversion explicit.

## Phase 2 — extract `PatternRouter`

Move:

- token hits;
- hit sequence;
- link and route usage;
- route scores and bias;
- recent-route retention;
- route selection calculations.

Inputs should be immutable registry views plus explicit time and impulse values. Mutation of cells remains outside the router.

Desired boundary:

```text
registry snapshot + impulse + routing state
→ route decision
→ caller applies cell mutation
→ caller records evidence
```

This prevents routing policy from silently committing domain changes.

## Phase 3 — extract `AdaptiveController`

Move TRS, harmony tuning, reflex state, and temporary stress effects behind one component.

The controller should produce explicit decisions rather than mutate unrelated runtime state directly.

```rust
pub(crate) struct AdaptiveDecision {
    pub harmony: HarmonyTuning,
    pub reflex_actions: Vec<ReflexAction>,
    pub tick_adjust_ms: i32,
}
```

Safety checks:

- same defaults;
- same numeric clamping;
- same event and journal deltas;
- deterministic tests for fixed observations.

## Phase 4 — extract `TimelineRuntime`

Move journal access, Mirror epochs, replay reports, and event-buffer responsibilities.

The timeline component records accepted domain events; it must not decide authorization or silently execute side effects.

Keep persistence failures explicit and fail closed where an acknowledgement implies durable recording.

## Phase 5 — extract `GoalRuntime`

Move Seed Garden, variants, dreams, synchrony, and resonant-model state.

This phase should happen last because these systems currently cross several runtime boundaries.

Separate:

- evaluation;
- proposed action;
- accepted mutation;
- recorded evidence.

A dream, variant, or seed must not gain external execution authority merely because it was persisted.

## Pull-request sequence

Use one narrow PR per phase:

1. `test: characterize cluster field behavior`
2. `refactor: extract cell registry`
3. `refactor: extract pattern router`
4. `refactor: extract adaptive controller`
5. `refactor: extract timeline runtime`
6. `refactor: extract goal runtime`

Each PR must:

- start from an exact reviewed base;
- avoid unrelated formatting churn;
- keep the public facade intact;
- run full root-workspace CI;
- rerun protocol or durability matrices when their paths or contracts are touched;
- document any evidence lane that was unavailable instead of representing it as passed.

## Review checklist

- [ ] Net diff matches the declared extraction only.
- [ ] No serialized schema changed.
- [ ] No public method changed unintentionally.
- [ ] Characterization tests remain green.
- [ ] Event ordering remains stable.
- [ ] Full root workspace compiles and tests.
- [ ] Clippy and formatting pass.
- [ ] No new production guarantee is introduced.
- [ ] Exact-head evidence is recorded.
- [ ] Human merge authority remains explicit.

## Rollback strategy

Every phase preserves `ClusterField` as a facade, so rollback is a normal PR revert without data migration. Do not combine structural extraction with snapshot-schema or journal-format changes.
