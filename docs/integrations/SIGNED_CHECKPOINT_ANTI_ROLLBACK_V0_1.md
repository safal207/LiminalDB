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
Key activation, expiry, and revocation are evaluated at caller-trusted
verification time (`now_ms`), not at issuer-selected `issued_at_ms`.

`issued_at_ms` is signed declarative metadata. It must not be later than trusted
verification time, but it does not by itself prove when the checkpoint existed
or establish precedence in the outside world. This prevents a compromised key
from bypassing expiry or revocation by backdating a newly signed checkpoint.

Without an independently trusted timestamp or external receipt for each
checkpoint, v0.1 fails closed: a key that is expired or revoked at verification
time cannot validate historical checkpoints. A future profile may preserve
historical verification by binding each checkpoint to an OTS, transparency-log,
or equivalent provider receipt whose time the signer cannot choose.

## Key rotation

A checkpoint chain may rotate `key_id` while preserving `signer_id`.

The new checkpoint must:

- reference the previous checkpoint exactly;
- use a strictly greater event sequence;
- use a strictly later WAL position;
- not move issuance time backwards;
- contain a different event-chain head.

Both old and new public keys must be present and active at trusted verification
time for the complete chain to validate under v0.1. Preserving history after an
old key expires or is revoked requires the future trusted-receipt profile
described above.

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
4. backdated checkpoint from a revoked key;
5. backdated checkpoint from an expired key;
6. checkpoint claiming a future issuance time;
7. expired checkpoint;
8. forked ledger head against an anchor;
9. rollback to an older mutually consistent copy;
10. valid descendant of an external anchor.

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
- trusted issuance time or world-time precedence from `issued_at_ms` alone;
- external anchor provider correctness;
- distributed consensus.

The original independent transition verdicts remain unchanged.

## Merge order and review-quota fallback

1. PR #89 is merged;
2. this checkpoint PR is retargeted to `main`;
3. checkpoint conformance, workspace CI, Security Baseline, and the inherited
   cross-platform ledger matrix pass on one exact head;
4. the preferred independent lane is a CodeRabbit review on that exact head;
5. when CodeRabbit returns an authenticated quota or rate-limit signal instead
   of a review, that external lane is recorded as `WAIVED_QUOTA`, never as a
   successful independent review;
6. the unchanged exact head is then reviewed by a role-separated GPT panel for
   temporal trust, cryptography and key lifecycle, causal anti-rollback,
   adversarial semantics, and CI/scope consistency;
7. the panel is explicitly one model operating under independent role contracts;
   it is advisory and cannot approve, execute, or merge;
8. any P0-P2 role finding requires a fix and a complete exact-head rerun;
9. merge requires no unresolved review threads, a clean role-panel verdict,
   green exact-head gates, and an explicit maintainer/human D6 decision.
