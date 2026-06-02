# ActivityPub and Matrix Integration Plan

## Purpose

This document explains how LiminalDB can evolve from a local event-sourced runtime into a reusable memory layer for federated applications.

LiminalDB should not replace ActivityPub or Matrix. It should sit behind adapters and provide local event history, replay, validation records, and state projections.

## Integration model

```text
protocol adapter
  -> event normalization
  -> validation
  -> LiminalDB event envelope
  -> Mirror Timeline
  -> local projection
  -> optional outbound event
```

The adapter handles protocol details. LiminalDB handles event memory.

## ActivityPub path

ActivityPub has actors, activities, inboxes, outboxes, and objects.

A future adapter can normalize incoming activities into LiminalDB events. For example, an object creation can become a local event with actor, stream, type, payload hash, and validation result.

The useful first deliverable is not a full server. The useful first deliverable is a mapping note and a mock adapter that shows how an incoming activity becomes a local timeline event.

## Matrix path

Matrix has rooms, events, senders, timelines, and state changes.

A future adapter can normalize room events into LiminalDB streams. The room can become the stream. The sender can become the actor. The Matrix event can become an event envelope and then be appended to the Mirror Timeline.

The useful first deliverable is a mapping note and a mock adapter that shows how a room event becomes a local timeline event.

## First grant-stage deliverables

- Event envelope specification.
- Mock ActivityPub adapter.
- Mock Matrix adapter.
- Two-node local replication demo.
- Event replay demo.
- Rejection and audit trail demo.
- Privacy and payload minimization note.

## Two-node replication demo

```text
node A creates an event
node A appends it to Mirror Timeline
node B receives the normalized event
node B validates it
node B appends ACCEPT or REJECT to its own Mirror Timeline
node B updates local projection
```

Expected reviewer output:

```text
node-a: event created
node-a: mirror append OK
node-b: event received
node-b: validation ACCEPT
node-b: mirror append OK
node-b: projection updated
```

## Conflict handling

The first version should expose decisions explicitly:

- remote event accepted;
- remote event rejected;
- remote event held for application-level resolution;
- remote event superseded by newer local event;
- remote event stored but not projected.

Every decision should be represented as an event or audit record.

## Privacy design

The adapter should avoid blindly storing or forwarding full payloads.

Recommended principles:

- use payload hashes when possible;
- allow encrypted payload storage;
- keep private payloads local by default;
- support tombstones and redaction events;
- document replicated fields;
- avoid creating an accidental surveillance log.

## Non-goals for the initial grant

The initial grant should not promise a full production ActivityPub server, a full Matrix homeserver, global consensus, or replacement of existing databases.

The narrow target is stronger:

> Build a local-first event memory layer with documented adapter paths to ActivityPub and Matrix.
