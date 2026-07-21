# Signed Checkpoint and External Anti-Rollback Profile v0.1

**Status:** Draft interoperability profile  
**Tracking issue:** [LiminalDB #90](https://github.com/safal207/LiminalDB/issues/90)  
**Base ledger:** [LiminalDB PR #89](https://github.com/safal207/LiminalDB/pull/89), merged into `main`

## Purpose

The trustworthy-transition ledger v0.1 detects local corruption and semantic
inconsistency through:

- WAL CRC framing;
- a SHA-256 semantic event chain;
- digest-bound snapshots;
- snapshot-tail replay checked against full WAL replay.

Those controls cannot detect replacement of the entire local WAL and snapshot
set with a mutually consistent older copy. This profile adds a separate
checkpoint boundary:

```text
verified local ledger state
        ↓
signed checkpoint manifest
        ↓
optional externally trusted anchor
```

The two verification levels remain explicit:

```text
LOCAL_SIGNATURE_ONLY
EXTERNAL_ANCHOR_VERIFIED
```

A local signature proves that a trusted key signed exact checkpoint bytes. It
does not, by itself, prevent rollback of every local file, including local
checkpoint files and local key metadata.

## Signed manifest

The versioned manifest binds:

```text
schema
checkpoint_profile
ledger_profile
storage_root_identity
event_chain_head
last_sequence
wal_segment
wal_offset
projection_digest
snapshot_digest
signer_id
key_id
issued_at_ms
expires_at_ms
previous_checkpoint_ref
```

The manifest reference is:

```text
sha256:SHA256(CBOR(manifest_body))
```

The Ed25519 signature is calculated over the same deterministic CBOR bytes.
Changing any bound field invalidates either the manifest reference, signature,
or both.

## Ledger integration

`CheckpointLedgerExt::checkpoint_material` derives checkpoint material from:

- the current trustworthy-transition ledger head;
- current event count;
- the ledger's canonical full-state projection digest;
- `TransitionLedgerSnapshotInfo` returned by a current snapshot;
- a caller-supplied stable storage-root identity.

`TransitionLedgerSnapshotInfo` is an opaque capability outside the crate. Public
getters expose its path, offset, counts, and snapshot digest, but callers cannot
construct or alter its hidden state-binding fields.

Before checkpoint material is returned, the adapter requires all of the
following to match the currently opened ledger:

1. snapshot path;
2. event and projection counts;
3. event-chain head;
4. canonical projection digest, including sequence, head, projections, and
   record ownership.

A snapshot from another ledger is rejected even when its public counts match.
A snapshot becomes stale and is rejected after the ledger advances.

`storage_root_identity` is a SHA-256 reference supplied by deployment code. It
may identify a tenant, logical ledger, hardware-bound root, or another stable
namespace. It must not be a mutable filesystem path presented as an identity.

## Trusted key registry

Each trusted key record declares:

```text
signer_id
key_id
public_key_hex
valid_from_ms
valid_until_ms
revoked_at_ms
```

Verification uses the key identified by the manifest's exact signer and key IDs.
The checkpoint issuance time must fall inside the key validity interval and
before revocation.

Historical signatures issued before an effective revocation remain
cryptographically verifiable under this v0.1 model. Deployments requiring
retroactive distrust must remove the key from the trusted registry or publish a
higher-level policy that rejects its historical checkpoints.

## Key rotation

A checkpoint chain may rotate `key_id` while preserving `signer_id`.

The new checkpoint must:

- reference the previous checkpoint exactly;
- use a strictly greater event sequence;
- use a strictly later WAL position;
- not move issuance time backwards;
- contain a different event-chain head.

Both old and new public keys must be present in the verifier's trusted registry
for the complete historical chain to validate.

## External anchor contract

An external anchor is caller-trusted metadata:

```text
provider_profile
anchor_id
checkpoint_ref
storage_root_identity
event_chain_head
last_sequence
anchored_at_ms
```

The provider may be:

- an immutable deployment registry;
- a transparency log adapter;
- a Sigstore/Rekor integration;
- a hardware or cloud attestation service;
- another independently administered append-only system.

LiminalDB does not treat arbitrary local JSON as an external anchor. Trust in the
provider receipt is established by caller code outside this module.

## Anti-rollback verification

With an external anchor, verification requires:

1. every supplied checkpoint signature is valid;
2. checkpoint ancestry is exact and monotonic;
3. the latest supplied sequence is not older than the external anchor;
4. the anchored checkpoint is present in the supplied chain;
5. its storage identity, event-chain head, and sequence exactly match the anchor;
6. all later checkpoints descend from that anchored checkpoint.

The verifier distinguishes:

```text
EXTERNAL_ANCHOR_ROLLBACK
EXTERNAL_ANCHOR_FORK
EXTERNAL_ANCHOR_NOT_IN_CHAIN
```

A locally consistent older copy fails with `EXTERNAL_ANCHOR_ROLLBACK` when its
latest sequence is below the trusted anchor.

A different checkpoint at the anchor's sequence and storage identity fails with
`EXTERNAL_ANCHOR_FORK`.

## Deterministic fixture coverage

The checked-in fixture covers:

1. valid local signature;
2. wrong signer;
3. valid key rotation;
4. revoked key;
5. expired checkpoint;
6. forked ledger head against an anchor;
7. rollback to an older mutually consistent copy;
8. valid descendant of an external anchor.

The ledger integration tests additionally cover:

1. valid checkpoint material from the current snapshot;
2. rejection of a foreign snapshot with matching public counts;
3. rejection of a stale snapshot after the ledger advances.

Run:

```bash
cd liminal-db
cargo test -q -p liminal-store --no-fail-fast
```

The dedicated GitHub Actions gate also runs `cargo check -p liminal-store`.

## Error stability

`CheckpointError::code()` exposes stable machine-readable failure codes. Human
error text is descriptive and may evolve; integrations should branch on the
stable code.

## Boundaries

A valid signed and externally anchored checkpoint proves only that:

- the declared signer attested to exact checkpoint material;
- the supplied checkpoint chain descends from the trusted anchor;
- the local projection, snapshot digest, and event-chain head are bound to that
  attestation.

It does not prove:

- authorization policy correctness;
- that a tool observation was truthful;
- that a response-integrity verifier was correct;
- causal validity;
- continuation safety;
- signer operational security;
- external anchor provider correctness;
- distributed consensus.

The original independent transition verdicts remain unchanged.

## Merge order

1. PR #89 is merged;
2. this checkpoint PR is retargeted to `main`;
3. checkpoint conformance, workspace CI, Security Baseline, and the inherited
   cross-platform ledger matrix must pass on one exact head;
4. CodeRabbit and mandatory human/Codex review must complete on that exact head;
5. only then may this profile merge.
