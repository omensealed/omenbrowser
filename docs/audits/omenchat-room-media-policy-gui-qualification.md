# OMENchat Room Media-Policy GUI Qualification

Date: 2026-07-28

Baseline: `release/v0.9.6-4` at `e832f74`, plus this GUI unit.

## Scope and verdict

Verdict: the non-product current/current qualification build passes native
Linux Iced attachment acceptance, positive-ceiling rejection, and disabled
policy interaction against independently built omenchatd binaries.

This closes the local GUI attachment gate. It is not Windows/macOS,
physical-GPU, public-network, or production-activation evidence.

## Isolation and deterministic input

The harness builds:

```bash
cargo build --locked --no-default-features \
  --features desktop-product,omenchat-room-media-policy-qualification \
  --bin omenbrowser_rs
cargo build --locked --manifest-path src/server/Cargo.toml \
  --no-default-features \
  --features server-headless,omenchat-room-media-policy-qualification \
  --bin omenchatd
```

It runs three fresh browser/server homes with separate generated identities,
SQLite databases, upload roots, Reticulum storage, and dynamically allocated
loopback TCP ports. The desktop runs at 1400x900 under Xvfb/i3 with software
rendering. No maintainer identity, interface, message, upload, or configuration
root is read.

Two narrow hooks make the GUI interaction deterministic:

- automatic opening of `OMENBROWSER_QUALIFICATION_OMENCHAT_TARGET` is shared by
  the existing slow-mode and room-media qualification builds;
- `OMENBROWSER_QUALIFICATION_OMENCHAT_UPLOAD_PATH` supplies the file picker
  result only when `omenchat-room-media-policy-qualification` is compiled.

Both paths still use the normal Iced Open/Attach messages, asynchronous
completion, metadata preflight, client request, Reticulum Resource, server
admission, and durable upload implementation. Canonical product profiles do
not compile the picker hook, and product-feature verification rejects the
qualification feature.

## Cases and evidence

The machine-readable report was:

```json
{
  "accepted_upload_bytes": 65536,
  "accepted_upload_count": 1,
  "accepted_upload_file_exists": true,
  "disabled_upload_count": 0,
  "isolated_loopback": true,
  "native_linux_iced": true,
  "over_limit_upload_count": 0,
  "qualification_feature_only": true,
  "screenshots": {
    "disabled": true,
    "over-limit": true,
    "under-limit": true
  },
  "software_rendering": true,
  "status": "pass"
}
```

Accepted:

- negotiated room ceiling: 262,144 bytes;
- selected file: 65,536 bytes;
- GUI showed `Uploads ≤ 256.0 KiB · room policy`;
- the upload crossed the real Resource path and the timeline showed local
  cached-upload completion;
- SQLite contained one 65,536-byte upload row and its path was a regular file.

Over limit:

- negotiated room ceiling: 262,144 bytes;
- selected file: 300,000 bytes;
- metadata preflight occurred before file allocation or transport admission;
- GUI reported that 293.0 KiB exceeds the **room** file limit;
- SQLite and the upload filesystem remained empty.

Disabled:

- negotiated room upload policy: disabled;
- GUI showed `Uploads disabled · room policy`;
- the Attach control had no action and its tooltip read
  `Uploads disabled by room policy`;
- clicking it left SQLite and the upload filesystem empty.

The first visual run exposed one truthful-label defect: the effective room
ceiling was described as a server file limit. The admission calculation was
already correct. The helper now retains whether the binding ceiling came from
the room policy and presents `room file limit`; a focused unit test guards both
server and room labels.

## Commands and results

Passed:

```bash
bash -n scripts/run-omenchat-room-media-policy-gui-qualification.sh
shellcheck scripts/run-omenchat-room-media-policy-gui-qualification.sh

cargo test --locked --no-default-features \
  --features desktop-product,omenchat-room-media-policy-qualification \
  qualification_target_waits_for_runtime_then_uses_normal_open_path \
  --lib -- --nocapture
# 1 passed

cargo test --locked --no-default-features \
  --features desktop-product,omenchat-room-media-policy-qualification \
  room_media_policy_qualification_picker_requires_a_nonempty_explicit_path \
  --lib -- --nocapture
# 1 passed

cargo test --locked --no-default-features \
  --features desktop-product,omenchat-room-media-policy-qualification \
  omenchat_upload_file_limit_rejects_oversized_local_files \
  --lib -- --nocapture
# 1 passed

bash scripts/run-omenchat-room-media-policy-gui-qualification.sh \
  --evidence /tmp/omenchat-room-media-policy-gui-final
# pass

cargo test --locked --no-default-features \
  --features desktop-product,omenchat-room-media-policy-qualification --lib
# 1,530 passed; 31 ignored

cargo clippy --locked --no-default-features \
  --features desktop-product,omenchat-room-media-policy-qualification \
  --all-targets -- -D warnings
# pass

cargo fmt --all -- --check
git diff --check
bash scripts/verify-product-features.sh
# pass
```

## Resource, compatibility, and rollback

The qualification hooks add no product worker, timer, queue, cache, retry,
schema, protocol field, dependency, or recurring traffic. Each picker task
retains the existing single user-action ownership and bounded upload path.

The user-visible label correction changes no admission decision or wire byte.
Rollback removes the qualification hooks, script, label-source argument, and
this documentation. No identity, database, configuration, history, or upload
migration is required.

## Remaining evidence

- adjacent current/previous mixed-version shape qualification;
- explicit production activation and rollback review;
- later batched Python interoperability, Windows/macOS presentation, packaging,
  and physical-network evidence.

The separate optimized process observation is now complete. It found empty
transport/event and pending-resource queues, stable FD counts, no worker join
failures, and bounded shutdown around an accepted 64-KiB upload. See
`omenchat-room-media-policy-process-measurement.md`; its short loopback run is
not a long-duration leak or physical-GPU claim.

Screenshots remain in the caller-selected disposable evidence directory and
are not committed.

Not run in this unit: hosted CI, Python interoperability, native Windows/macOS
presentation or packaging, adjacent-version process compatibility, public
gateways, physical interfaces, physical GPU measurement, or a long-running
process soak. None is claimed as passed.
