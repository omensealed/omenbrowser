# OMENchat Room Media-Policy Process Measurement

Date: 2026-07-28

Baseline: `release/v0.9.6-4` at `f390ea2`, plus this measurement unit.

## Scope and verdict

Verdict: the optimized native Linux qualification build passed a bounded
before/after process observation around one accepted 64-KiB GUI attachment.
The server reported the expected inbound Resource, empty transport/event and
pending-resource queues, no worker join failures, and bounded desktop/server
shutdown.

This is one short Xvfb/software-rendered loopback observation. It is not a
long-running leak test, a physical-GPU result, a public-network result, or a
hardware-independent performance threshold.

## Reproduction

```bash
OMENCHAT_ROOM_MEDIA_POLICY_WARMUP_SECONDS=5 \
OMENCHAT_ROOM_MEDIA_POLICY_SAMPLE_SECONDS=30 \
bash scripts/run-omenchat-room-media-policy-gui-qualification.sh \
  --evidence /tmp/omenchat-room-media-policy-gui-process-final
```

Setting the sample duration to zero, its default, preserves the quick debug
GUI qualification. A positive duration:

- requires Linux `/proc`;
- builds independently optimized root and standalone-server binaries;
- warms the accepted case for the requested bounded interval;
- collects five settled samples before the Attach action;
- waits for the normal durable upload commit;
- collects the requested number of one-second samples afterward;
- records CPU, RSS, private dirty memory, threads, and file descriptors;
- retains the server's production 30-second queue/link telemetry; and
- times the existing Alt-F4 and SIGTERM drain paths.

Both duration inputs accept only integers from zero through 300 seconds. The
script owns both processes and the sampling loop, imposes the existing
eight-second shutdown ceiling, and removes all isolated state on exit. It adds
no product worker, timer, queue, cache, or recurring traffic.

## Observation

Environment:

- application/server version: `0.9.6-3` before the planned release version
  change;
- target: `x86_64-unknown-linux-gnu`;
- compiler: Rust 1.97.0;
- build: optimized release profile with the non-product room-media-policy
  qualification feature;
- display: Xvfb/i3 with software rendering;
- transport: one isolated loopback TCP client/server path;
- input: one 65,536-byte attachment under a negotiated 262,144-byte ceiling.

Process observation:

| Metric | Before upload | After durable upload |
|---|---:|---:|
| browser CPU median / p95 | 0.964% / 5.759% | 0.972% / 4.801% |
| server CPU median / p95 | 0.000% / 0.960% | 0.000% / 0.976% |
| browser RSS median / p95 | 230,200 / 230,208 KiB | 234,864 / 235,188 KiB |
| server RSS median / p95 | 13,412 / 13,412 KiB | 16,124 / 16,124 KiB |
| browser private dirty median / p95 | 47,028 / 47,060 KiB | 51,808 / 52,252 KiB |
| server private dirty median / p95 | 4,684 / 4,684 KiB | 7,344 / 7,344 KiB |
| browser threads median / p95 | 101 / 101 | 99 / 100 |
| server threads median / p95 | 31 / 31 | 31 / 31 |
| browser FDs median / p95 | 62 / 62 | 62 / 62 |
| server FDs median / p95 | 15 / 15 | 15 / 15 |

The last post-upload RSS sample was 4,980 KiB above the last pre-upload browser
sample and 2,712 KiB above the server sample. That one-time step is consistent
with initializing the attachment/Resource path, but this short run cannot
classify retained allocator pages or prove absence of later growth. Thread and
FD counts did not grow.

Server telemetry after acceptance reported:

- one active Link;
- one 64.0-KiB inbound Resource and one accepted upload offer;
- zero pending Resources and pending uploads;
- zero transport/event queue items and bytes;
- zero queue rejections;
- database worker in-flight count zero;
- seven completed database operations with 1,201-us average and 3,763-us
  maximum observed latency;
- no protocol errors or replay collisions.

Shutdown:

- desktop: 135 ms with `desktop shutdown drained successfully`;
- omenchatd: 53 ms;
- worker join timeouts/failures: zero;
- final transport/event/log queue items and bytes: zero.

## Compatibility, storage, and rollback

The harness changes no wire field, capability, schema, configuration default,
identity ownership, storage location, admission decision, or product feature.
It observes the same accepted GUI path already qualified in
`omenchat-room-media-policy-gui-qualification.md`.

Rollback removes the opt-in environment parsing, sampler/report helpers, this
document, and the testing references. No data migration or user cleanup is
required.

## Remaining evidence

- adjacent current/previous mixed-version shape qualification;
- explicit product capability activation and rollback review;
- a longer repeated-transfer soak if field evidence or subsequent changes
  suggest retained growth;
- later batched Python interoperability, native Windows/macOS presentation,
  package lifecycle, public-network, physical-interface, and physical-GPU
  evidence.
