# OMENchat slow-mode resource qualification

Date: 2026-07-28

Baseline: `release/v0.9.6-4` at `989fcad`, plus this measurement extension

Verdict: the isolated release-profile current/current slow-mode scenario held
one real Reticulum Link, left the bounded server transport/event queues empty,
drained all observed server work without timeout or failure, and closed both
processes normally. The short local sample found no activation-specific
resource blocker. It is an evidence point, not a universal performance
threshold or physical-GPU result.

## Reproduction

From the repository root on Linux:

```bash
OMENCHAT_SLOW_MODE_WARMUP_SECONDS=10 \
OMENCHAT_SLOW_MODE_SAMPLE_SECONDS=30 \
  bash scripts/run-omenchat-slow-mode-gui-qualification.sh \
  --evidence /tmp/omenchat-slow-mode-measurement
```

Setting a nonzero sample duration changes both isolated qualification builds to
the release profile. The duration is bounded to 300 seconds. The harness
samples both real process IDs once per second through Linux `/proc`; it does
not include Xvfb or i3 in the application figures. A duration of at least 30
seconds additionally requires an omenchatd native stats interval containing
one active Link and zero queued transport/event payloads, followed by a clean
drain line with zero queue occupancy and no worker join failure.

The existing functional gate still runs after sampling: the GUI sends two
messages, SQLite must contain only the first, and the rejected second draft
must remain exact. The normal Alt-F4 and SIGTERM paths are timed rather than
replaced.

## Local result

Environment:

- optimized root `desktop-product` plus the non-product slow-mode
  qualification feature;
- optimized independent `server-headless` plus its qualification feature;
- software-rendered Iced at 1400x900 under Xvfb/i3;
- 10-second post-connection warmup and 30 one-second samples;
- Rust 1.97.0, `x86_64-unknown-linux-gnu`.

Process summary:

| Metric | OMENbrowser | omenchatd |
| --- | ---: | ---: |
| CPU median | 0.000% | 0.000% |
| CPU p95 | 5.829% | 1.926% |
| RSS median | 233,716 KiB | 14,964 KiB |
| RSS p95 | 234,820 KiB | 14,964 KiB |
| private dirty p95 | 50,324 KiB | 6,160 KiB |
| threads p95 | 100 | 31 |
| file descriptors p95 | 62 | 15 |
| final-minus-first RSS | +1,100 KiB | +1,296 KiB |

Shutdown:

- desktop ordered shutdown: 186 ms;
- omenchatd SIGTERM drain: 53 ms;
- server worker join timeouts: 0;
- server worker join failures: 0.

The 30-second omenchatd observation reported:

```text
active_links=1 links_opened=1 links_closed=0
transport=items:0 bytes:0 oldest_ms:0 rejected:0
events=items:0 bytes:0 oldest_ms:0 rejected:0
db-worker: in_flight=0 completed=38 rejected=0 latency_max_us=3118
logs=items:1 bytes:695 dropped:0 priority_dropped:0 write_failed:0
```

After the functional send/rejection sequence, orderly shutdown reported the
same zero transport/event occupancy, zero log occupancy, and 41 completed
database-worker operations. SQLite contained exactly:

```json
{"room_message_count":1,"messages":[[1,"qualification-first-message"]]}
```

The preserved draft was exactly `qualification-second-message`.

## Interpretation

The older 0.9.5 Phase 0 desktop sample used an idle root without an active
OMENchat Link, so its 223,844 KiB median RSS and 3.934% CPU p95 are contextual,
not a like-for-like regression baseline. This live scenario adds a native
Reticulum route, an authenticated OMENchat Link, server policy projection, and
periodic ping work. Median CPU remained zero, queue ownership stayed bounded,
and the short RSS series did not show monotonic unbounded growth. The sample is
too short to prove long-term absence of growth; release soak and field evidence
remain relevant.

The client structured log contained 14 existing Debug-level LXMF decode
rejections while context-zero OMENchat traffic was also routed to OMENchat.
They did not cause protocol errors, queue growth, warning/error admission, or
shutdown residue in this run. This is not introduced by slow mode, but it is a
useful future routing/log-noise efficiency investigation. The isolated root
also logged the known harmless startup warning that its default `mock.page`
tab is not a native Reticulum address.

## Safety, compatibility, and rollback

The measurement mode:

- creates fresh browser and server roots and separate identities;
- contacts only its loopback TCP server;
- adds no dependency, production feature, schema, protocol field, worker,
  timer, queue, cache, or persistent setting;
- leaves the default fast GUI qualification behavior unchanged; and
- stores raw samples, summaries, logs, screenshots, database observation, and
  copied draft only below the caller-selected evidence directory.

Rollback is a source revert of the optional sampling branch and this
documentation. No state migration is involved.

## Limits and next decision

- Physical GPU activity was not measured; software rendering is not a proxy.
- This is one local 30-second sample, not a cross-platform or low-core result.
- Client-side Reticulum queue occupancy is not currently exported as a direct
  live process metric. Server transport/event/database/log queues are
  authoritative and were observed.
- No hosted CI, Python interoperability, package, public-network, or hardware
  result is claimed.

The remaining slow-mode gate is the explicit product activation and rollback
decision, followed by the normal batched release-candidate matrix.
