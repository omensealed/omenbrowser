# OMENchat Room Media-Policy Adjacent Qualification

Date: 2026-07-28

Baseline: `release/v0.9.6-4` at `9e9c90d`, plus this compatibility unit.

Adjacent release: immutable `v0.9.6-3` commit
`414d8eafd1a845a986032bad993ac9c09cc378e4`.

## Scope and verdict

Verdict: the room-media-policy adjacent shape gate passes in both process
directions, the current server preserves legacy per-Link upload admission, and
current/current policy projection survives an orderly server restart.

Both the working tree and adjacent tag still report application version
`0.9.6-3` because the planned `0.9.6-4` version change has not occurred. The
gate distinguishes them by the immutable adjacent Git commit and independently
archived/build source, not by version text alone.

## Matrix

### Current qualification client to adjacent server

The current client was built with
`desktop-product,omenchat-room-media-policy-qualification`; it therefore
requested the cumulative room-media-policy capability set. The immutable
adjacent server:

- completed runtime startup, Link open, session open, join, ordinary room
  message, and echo;
- returned the legacy four-field room projection;
- did not negotiate announcement rooms, moderation audit, or room media policy;
- projected no announcement bits or room upload ceiling.

This proves the strict current parser accepts the exact adjacent legacy shape
without fabricating capability evidence.

### Adjacent client to current qualification server

The immutable adjacent client completed runtime startup, Link open, session
open, join, ordinary room message, and echo against a current server built
with `server-headless,omenchat-room-media-policy-qualification`.

The adjacent parser is permissive, so this is ordinary live compatibility
evidence. Exact legacy shaping and admission on the current server are guarded
separately at the captured production Link dispatcher.

### Simultaneous current-server Links

One current server with a configured 262,144-byte room ceiling handled two
authenticated Links:

- the legacy Link received a four-field room with no policy, slow-mode, or
  upload-ceiling projection;
- the negotiated Link received the seven-field media-policy room;
- a 307,200-byte legacy `UploadOffer` was accepted under the unchanged
  server-wide 524,288-byte limit;
- the same-sized negotiated offer received the typed room-ceiling rejection;
- identity replacement removed the negotiated binding;
- Link close released the accepted legacy pending offer.

This exercises production frame dispatch and per-Link shaping/admission. It
does not send an adjacent-binary Resource body.

### Current/current restart

The current qualification client/server process smoke required, on both the
initial and replacement Links:

- durable, announcement-room, slow-mode, and room-media-policy capabilities;
- announcement policy bit zero, independently reflecting the ordinary room
  configuration;
- a projected 262,144-byte upload ceiling;
- ordinary message completion;
- stable server destination through orderly restart.

## Machine-readable result

```json
{
  "adjacent_client_current_server_ordinary_traffic": true,
  "adjacent_commit": "414d8eafd1a845a986032bad993ac9c09cc378e4",
  "adjacent_release": "v0.9.6-3",
  "capability_fabricated_for_adjacent_peer": false,
  "current_client_adjacent_server_legacy_four_field": true,
  "current_current_initial_media_policy_shape": true,
  "current_current_replacement_link_media_policy_shape": true,
  "current_server_legacy_and_media_policy_shaping_regression": true,
  "current_server_legacy_upload_admission_preserved": true,
  "isolated_loopback": true,
  "moderation_audit_fabricated_for_adjacent_peer": false,
  "room_media_policy_fabricated_for_adjacent_peer": false,
  "status": "pass"
}
```

## Commands and results

Passed:

```bash
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features \
  --features server-headless,omenchat-room-media-policy-qualification \
  live::tests::room_media_policy_qualification_shapes_and_admits_per_authenticated_link \
  --lib -- --exact --nocapture
# 1 passed

bash scripts/run-omenchat-room-shape-compatibility.sh \
  --report /tmp/omenchat-room-media-policy-adjacent-final.json
# pass

bash -n scripts/run-mixed-0-6-0-9-omenchat-live.sh \
  scripts/run-omenchat-room-shape-compatibility.sh
shellcheck scripts/run-mixed-0-6-0-9-omenchat-live.sh \
  scripts/run-omenchat-room-shape-compatibility.sh
# pass
```

The first full process run exposed one verifier error: it coupled accepted
announcement capability with an enabled announcement-room bit. The room was
ordinary, so the correct independent evidence was capability accepted, bit
zero, media-policy capability accepted, and upload ceiling present. The
assertion was corrected and the complete matrix reran successfully.

## Compatibility, storage, resources, and rollback

No protocol byte, operation, capability label, schema, configuration default,
identity, history, upload record, dependency, worker, queue, timer, cache, or
retry behavior changed. The generic mixed harness gained optional current
feature selectors so the same immutable process gate can test dormant
capabilities without changing canonical product builds.

All roots and Reticulum storage are temporary and isolated. The immutable
adjacent source is exported with `git archive`; no worktree checkout or user
state is touched.

Rollback removes the feature selectors, media-policy assertions, expanded
captured-Link regression, and this document. No persistent-data rollback is
required.

## Remaining evidence

- explicit production activation and rollback review;
- adjacent-binary attachment/Resource traffic if compatibility claims expand
  beyond the legacy room shape and ordinary message behavior proven here;
- later batched Python interoperability, native Windows/macOS presentation,
  packaging lifecycle, public-network, physical-interface, and physical-GPU
  evidence.
