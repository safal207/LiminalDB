# Federated Event-Sourcing Alignment

## Summary

LiminalDB is positioned as a reusable event-sourced memory layer for local-first and federated applications.

The core idea is simple:

```text
state is not only stored;
state is remembered as a sequence of events.
```

For federated systems, this matters because independent nodes need to exchange, replay, audit, prune, and reconcile state over time.

## Why event sourcing matters for federation

Federated applications operate across independent servers and user-controlled environments.

That makes state synchronization harder than in a centralized service.

A federated system needs to answer questions such as:

- What happened on this node?
- Which event came first?
- Which remote event was accepted?
- Which event was rejected or ignored?
- Can a node recover state from history?
- Can a user or admin inspect why state changed?
- Can old events be compacted without losing meaning?

Event sourcing provides a natural basis for these questions.

Instead of only storing the latest state, the system stores the decisions and transitions that produced that state.

## LiminalDB primitives

Current LiminalDB primitives already point in this direction:

- **Mirror Timeline** — append-only event history for replay and audit;
- **Seed Garden** — task and goal lifecycle over time;
- **Impulses** — signals that affect state;
- **Cells** — reactive runtime actors that respond to signals;
- **Reflexes** — feedback rules that react to stress and state changes;
- **TRS / PID control** — adaptive feedback to changing runtime conditions.

These concepts can be grounded into a practical federated event model.

## Proposed federated event envelope

A future grant deliverable should define a stable event envelope:

```json
{
  "event_id": "evt_...",
  "node_id": "node_...",
  "actor_id": "actor_...",
  "stream_id": "stream_...",
  "event_type": "seed.planted",
  "causal_parent": "evt_...",
  "logical_time": 42,
  "created_at": "2026-06-03T00:00:00Z",
  "payload_hash": "sha256:...",
  "payload": {},
  "signature": "..."
}
```

The envelope should support:

- local replay;
- remote replication;
- tamper-evident audit;
- minimal disclosure;
- pruning and compaction;
- eventual integration with federated protocols.

## Federation semantics

LiminalDB should not assume a single global authority.

The replication model should support:

- local-first writes;
- remote event ingestion;
- per-stream authorization;
- event validation;
- duplicate detection;
- causal ordering;
- conflict handling;
- rejection records for invalid remote events.

A node should be able to say:

```text
I received event X.
I validated it against policy Y.
I accepted or rejected it for reason Z.
I recorded that decision in the local timeline.
```

## Relationship to CRDTs

CRDTs are important for conflict-free replicated data.

LiminalDB does not need to claim that every data type is a CRDT from day one.

A practical first step is to separate:

- event transport;
- event validation;
- event ordering;
- application-specific merge rules;
- audit of merge decisions.

Future versions can add CRDT-backed data types where they fit naturally.

## Relationship to ActivityPub and Matrix

ActivityPub and Matrix already define important federation mechanisms.

LiminalDB should not replace them.

Instead, it can provide a local state and event-memory layer behind adapters:

```text
ActivityPub / Matrix event
  -> adapter normalizes event
  -> LiminalDB validates and records event
  -> Mirror Timeline persists decision
  -> local application state updates
  -> optional outbound federation event
```

## Privacy and minimization

Federated systems can easily over-replicate data.

LiminalDB should treat privacy as a design constraint:

- store hashes or commitments where possible;
- keep private payloads local unless replication is required;
- support encrypted payloads;
- allow event pruning and compaction;
- separate metadata from sensitive content;
- document what is replicated and why.

## What the grant should deliver

A strong grant outcome would include:

- event envelope specification;
- replication model document;
- local replay demo;
- remote mock-node replication demo;
- ActivityPub mapping note;
- Matrix mapping note;
- privacy and pruning design;
- tests for duplicate and invalid event handling;
- reviewer runbook.

## Non-goals

The project should not claim to replace:

- Mastodon;
- Matrix Synapse;
- existing databases;
- Kafka or NATS;
- IPFS;
- full CRDT frameworks.

The narrower claim is stronger:

> LiminalDB provides an event-sourced memory layer that can become useful infrastructure for local-first and federated applications.
