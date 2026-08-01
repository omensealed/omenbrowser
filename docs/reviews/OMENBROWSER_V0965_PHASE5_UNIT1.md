# OMENbrowser v0.9.6-5 Phase 5 unit 1 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

Defined and tested a dormant bounded NomadNet update-pointer envelope. It does
not activate LXMF topics or any network, storage, browser, or UI behavior.

Phase 4 invitation sending remains blocked by its environment-bound live
capability gate. The reviewed execution directive permits continuing other
non-dependent work when an upstream/environment gate is documented, so this
unit starts only Phase 5's first rollout step and does not bypass Phase 4.

## Design and resource impact

- The JSON envelope is capped at 2 KiB and rejects unknown fields.
- Destination, path, revision/hash, title, publication, expiry, clock skew, and
  maximum lifetime are explicitly bounded and validated.
- The envelope carries no page body, request data, credentials, private identity,
  attachment, executable reference, or automatic-fetch policy.
- Deduplication has one stable destination/path/revision tuple.
- Publisher authentication remains separate transport evidence and is not
  inferred from payload fields.

No owner, queue, cache, retained history, worker, task, timer, subscription,
retry, fetch, disk write, or network operation was added. Existing inline LXMF
attachments and NomadNet behavior are unchanged.

## Files changed

- `src/browser/mod.rs`
- `src/browser/update_pointer.rs`
- `docs/TESTING.md`
- `docs/design/NOMADNET_LXMF_UPDATE_POINTER_CHECKPOINT.md`
- `docs/reviews/OMENBROWSER_V0965_PHASE5_UNIT1.md`

## Validation

Passed:

```text
cargo test --locked --no-default-features --features desktop-product \
  update_pointer --lib
cargo clippy --locked --no-default-features --features desktop-product \
  --all-targets -- -D warnings
cargo fmt --all --check
git diff --check
```

Three focused tests cover round-trip/deduplication, total and field bounds,
unknown fields, canonical destinations and paths, revision syntax, clock skew,
expiry ordering, maximum lifetime policy, and expiration.

Not run: live SDK topics, external daemon, Python interoperability, packaging,
native non-Linux platforms, or hardware peers. This unit has no runtime caller
and cannot establish any of those behaviors.

## Compatibility, rollback, and next gate

There is no wire compatibility claim because nothing publishes or accepts this
envelope in production. Remove the module exports and two documents to roll it
back; no data cleanup is required.

The next smallest Phase 5 unit is a pure bounded admission owner for followed
topics and authenticated/unverified pointer evidence. It must remain caller-
inert until backend topic capability negotiation is proven.
