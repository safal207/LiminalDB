# LiminalOSAI Governance Checkpoint Bridge v0.1

Tracking issue: #112

## Purpose

This bridge persists exact, digest-only LiminalOSAI Durable Governance transition envelopes into LiminalDB's existing trustworthy-transition ledger and produces a signed checkpoint over the resulting durable ledger state.

```text
LiminalOSAI durable governance transition
        ↓
strict transition envelope
        ↓
TrustworthyTransitionLedger append
        ↓
WAL durability + deterministic replay
        ↓
crash-safe snapshot
        ↓
CheckpointLedgerExt
        ↓
CheckpointSigner
        ↓
verify_signed_checkpoint
        ↓
digest-only bridge receipt
```

The bridge is an **evidence boundary**, not an authority boundary. LiminalOSAI remains responsible for capability grants, objective integrity, causal-risk decisions, runtime-world binding and durable generation/CAS coordination.

## Envelope

Schema:

`liminalosai-governance-transition-envelope-v0.1`

The envelope carries only bounded evidence:

- SHA-256 of the LiminalOSAI governance root identifier;
- transition kind (`initialize`, `reserve`, `commit`, `mutate`, `reconcile`);
- generation before/after;
- governance-world SHA-256 before/after;
- reservation SHA-256 when applicable;
- operation/effect-payload SHA-256 when applicable;
- upstream transition/commit/reconciliation receipt SHA-256;
- trusted capture timestamp.

The envelope itself is canonical-CBOR hashed and represented as a `sha256:` reference. Raw argv, environment, credentials, filesystem paths, objective text and hidden evaluator state are outside this contract.

## Ledger mapping

Each unique envelope is appended as one `Authorization` record under a unique transition ID derived from the envelope digest. Both `record_ref` and `payload_digest` equal the exact envelope reference.

This intentionally does not reinterpret the upstream decision. LiminalDB records that the exact digest-bound governance transition was presented to the bridge.

After append, the bridge writes the existing crash-safe trustworthy-transition snapshot. `TrustworthyTransitionLedger::open` already verifies snapshot-assisted replay against full WAL replay, so a later bridge invocation reopens and validates the durable prefix before appending the next governance transition.

## Signed checkpoint

The bridge then derives checkpoint material from the exact post-append ledger and snapshot:

- storage-root identity bound to the LiminalOSAI root SHA-256;
- event-chain head;
- last sequence;
- WAL segment and position;
- projection digest;
- snapshot digest.

The existing `CheckpointSigner` signs this material. Before a success bundle is returned, the bridge creates the corresponding trusted public-key record and calls `verify_signed_checkpoint` against the just-produced manifest.

The bridge receipt binds:

- envelope reference and all governance-generation/world fields;
- appended event hash;
- checkpoint reference;
- event-chain head and sequence;
- projection and snapshot digests;
- signer and key identities;
- verification status `LOCAL_SIGNATURE_VERIFIED`.

The signing seed/private key is never returned in the bundle.

## Conformance helper

The example `liminalosai_governance_checkpoint_bridge` reads one JSON envelope body from stdin and writes one JSON checkpoint bundle to stdout.

It requires:

- ledger root as its single CLI argument;
- `LIMINALDB_TEST_SIGNING_SEED_HEX`;
- `LIMINALDB_CHECKPOINT_ISSUED_AT_MS`.

The seed environment variable is explicitly a **test/conformance helper**. Production signing remains an external KMS/HSM/provider integration problem; this bridge does not claim production key custody.

## Restart evidence

Tests and CI append a `reserve` transition, drop the process, reopen the same ledger root, append the corresponding `commit`, and verify:

```text
sequence 1 → sequence 2
chain head changes
signed checkpoint verifies
exact root/world/generation bindings survive restart
```

Tampered generation semantics and tampered receipt fields are rejected.

## Nonclaims

This bridge does not provide:

- LiminalOSAI capability or effect authority;
- distributed consensus;
- correctness on hostile/network filesystems;
- semantic proof that the upstream governance decision was correct;
- automatic rollback of physical effects;
- production KMS/HSM custody;
- external anti-rollback unless a separately trusted external anchor is supplied through LiminalDB's existing checkpoint-chain API.

A signed checkpoint proves that LiminalDB durably committed the exact envelope under the declared local signer identity. It does not upgrade evidence into permission.

Refs: LiminalOSAI #169, LiminalOSAI #134, LiminalDB #112.
