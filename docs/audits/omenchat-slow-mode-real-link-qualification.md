# OMENchat slow-mode real-Link qualification

Date: 2026-07-28

Baseline: `release/v0.9.6-4` at `3f3d025`

Status: current/current isolated real-Link gate passed; production activation
remains off

## Scope and activation boundary

This unit adds the explicit, non-product Cargo feature
`omenchat-slow-mode-qualification` to OMENbrowser and standalone omenchatd. It
activates the already bounded and deterministic slow-mode request, projection,
admission, and typed rejection paths only for the qualification binaries.

Neither `desktop-product`, `desktop-product-static-media`, `server-headless`,
nor `server-full` enables this feature. `scripts/verify-product-features.sh`
fails if it enters a canonical browser or server product. Managed Reticulum
runtime ownership, identities, state roots, wire version, and database schema
are unchanged.

## Process gate

`scripts/run-omenchat-slow-mode-qualification.sh` builds both roots with the
qualification feature and delegates to the existing isolated release smoke
harness. The bounded scenario:

1. creates fresh temporary browser and server roots;
2. proves room maintenance is refused while omenchatd owns the database;
3. configures the lobby with a 30-second interval while the server is stopped;
4. negotiates durable mutations plus `room-slow-mode-v1` over a real Reticulum
   Link and verifies the six-field room catalog;
5. commits one room message;
6. stops and restarts omenchatd orderly with the same destination;
7. reuses the persistent browser identity/client-instance state over a new Link;
8. observes typed error 1017 and proves the second message was not committed;
9. waits for the bounded interval to expire; and
10. reconnects and proves a new publication is admitted.

The final local run passed:

```text
bash scripts/release-omenchat-smoke.sh \
  --browser-bin target/debug/omenbrowser_rs \
  --server-bin src/server/target/debug/omenchatd \
  --tcp 127.0.0.1:42701 \
  --path-wait 15 \
  --slow-mode-rejection-smoke \
  --out /tmp/omenbrowser-slow-mode-qualification-6

outcome: pass
slow_mode_seconds: 30
restart_destination_stable: 1
restart_stop: orderly
```

The checked-in wrapper also passed and wrote this machine-readable result:

```text
bash scripts/run-omenchat-slow-mode-qualification.sh \
  --report /tmp/omenchat-slow-mode-qualification-report.json

initial_commit: true
replacement_link_typed_rejection: true
expiry_readmission: true
server_destination_stable: true
status: pass
```

All roots and identities used by the gate are temporary and isolated. The
script removes them on exit and retains no payload-bearing queue, cache, worker,
timer, or retry loop.

## Defects found by the process gate

The first qualification run caught a client state bug: the feature build added
the capability to `SessionOpen` but did not retain the corresponding pending
request marker. The server correctly sent a six-field catalog, while the client
attempted to decode it as the older shape. The request marker now follows the
same lifecycle as the advertised capability.

The next run caught an error-contract bug: slow-mode rejection used generic
`RateLimited` (1008), despite the shared protocol reserving
`SlowModeActive` (1017). All slow-mode deadline and saturation branches now emit
1017. Ordinary message-rate exhaustion remains 1008. Focused server tests pin
that distinction across exact replay, restart, legacy publication, and
disable/reenable boundaries.

## Compatibility and remaining gates

Four- and five-field production shapes remain byte-compatible and are covered
by the existing exact codec/session tests. A qualification client or server is
not a release artifact. Reverting this unit removes the feature and process
gate without migrating storage.

Still required before product activation:

- [x] run an adjacent released-binary four-/five-field process matrix (see
  `omenchat-room-shape-adjacent-qualification.md`);
- [x] prove a connected qualification client receives a shaped room delta after
  the live server commits a policy change (see
  `omenchat-slow-mode-live-delta-qualification.md`);
- [x] preserve the Iced composer draft after a typed slow-mode rejection without
  overwriting newer input (see
  `omenchat-slow-mode-draft-recovery-qualification.md`);
- [x] observe the policy/rejection flow in the real GUI (see
  `omenchat-slow-mode-gui-qualification.md`);
- [x] record client/server CPU, RSS, link, queue, and shutdown measurements
  (see `omenchat-slow-mode-resource-qualification.md`); and
- make an explicit activation and rollback decision.

No hosted CI, Python interoperability, package build, public network peer, or
physical interface result is claimed by this isolated current/current gate.

## Product validation

Canonical product profiles remained qualification-free and passed:

```text
bash scripts/verify-product-features.sh
product feature verification: pass (desktop-product)

cargo test --locked --no-default-features --features desktop-product
root library: 1,504 passed; 0 failed; 31 ignored
all integration and binary targets: passed

cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full
536 passed; 0 failed; 11 ignored

cargo clippy --locked --no-default-features \
  --features desktop-product --all-targets -- -D warnings
passed

cargo clippy --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full --all-targets -- -D warnings
passed

cargo fmt --all --check
git diff --check
bash -n scripts/release-omenchat-smoke.sh \
  scripts/run-omenchat-slow-mode-qualification.sh
passed
```

The ignored tests are the repository's explicit measurement, soak,
multiprocess, hardware, and known upstream maximum-Resource cases. No new test
was ignored by this unit. Hosted CI and packaging were intentionally not
triggered for this local qualification slice; those expensive gates remain
batched for the release checkpoint.
