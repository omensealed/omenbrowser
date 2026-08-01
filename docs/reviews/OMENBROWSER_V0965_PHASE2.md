# OMENbrowser v0.9.6-5 Phase 2 evidence report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Status

The feasible local Phase 2 lanes pass. The phase is not fully closed because a
separately installed `reticulumd` 0.9.6 executable is unavailable, so daemon
disconnect/restart recovery cannot honestly be claimed from this host.

No production dependency, feature, binary path, queue, storage limit, wire
version, schema version, or product version changed. Test support remains in
scripts and existing test modules and therefore does not increase production
binary size.

## Evidence matrix

| Boundary | Current evidence | Result/classification |
| --- | --- | --- |
| Rust OMENbrowser client to Rust `omenchatd` | Isolated TCP loopback smoke with generated temporary identities and roots | Pass |
| Link replacement | One retained client observed closure, opened a new Link, rejoined, sent, and received its echo | Pass |
| Lost acknowledgement and exact replay | Reactions and revisions deliberately lost their acknowledgement, replayed the same durable identity, and reconciled authoritative inline/Resource state | Committed exactly once |
| Client restart uncertainty | Persisted uncertain intents are recovered identity-scoped and are never transmitted automatically | Recovered uncertain; explicit user action required |
| Server restart | Durable room text/reaction/revision executors replay the stored result without repeating the effect; the live smoke preserves the server destination across orderly restart | Pass |
| SQLite before/after commit | Child-process kill test distinguishes rolled-back in-flight work from one committed event | Safely absent or committed exactly once |
| Upload temporary-write/commit boundaries | Child-process kill matrix covers temporary write, sync, rename, directory sync, ledger commit, and recovery | Conservatively recovered |
| Incomplete Resource/upload | Link close and inbound Resource failure release owned pending offers while preserving the Link where safe | Rejected/cleaned with bounded state |
| Direct/Resource request selection | Golden byte/selector tests plus pre-cancel, timeout, cancellation, correlation, and no cross-primitive replay tests | Pass |
| Embedded SDK direct/propagated/ticket policy | Deterministic typed matrix | Pass |
| External RPC field boundary | Real published client against bounded loopback MessagePack capture | Reduced guarantees documented |
| External `reticulumd` disconnect/restart | Tool not installed; existing smoke refuses to invent startup flags and reports an explicit skip | Not run |
| Current Python RNS/LXMF/NomadNet | Existing pinned scripts and CI lanes; not rerun for this local slice | Not rerun |
| UDP maximum Resource | Current locked 0.9.6 known-red reproducer | Unsupported at that maximum boundary |
| Physical/hardware interfaces | No hardware attached | Not run |

## Live loopback run

Command:

```text
bash scripts/release-omenchat-smoke.sh \
  --browser-bin target/debug/omenbrowser_rs \
  --server-bin src/server/target/debug/omenchatd \
  --tcp 127.0.0.1:42428 \
  --path-wait 45 \
  --continuous-client-reconnect \
  --reaction-smoke \
  --revision-smoke \
  --out /tmp/omenbrowser-v0966-phase2
```

Result: pass.

The report proved:

- capability negotiation;
- reaction add, lost acknowledgement, exact replay, Resource snapshot, no-op,
  remove, and persisted intent;
- revision correction, lost acknowledgement, exact replay, Resource snapshot,
  tombstone, and persisted intent;
- orderly server stop;
- stable server destination after restart;
- Link close and replacement;
- session reconnect, message send, and echoed message;
- the same reaction and revision recovery matrix after replacement.

The script removed both generated identity/storage roots at completion. Only
bounded diagnostic outputs using a canned test message and public destination
remained under `/tmp`; no maintainer identity or normal application/server
state was accessed.

## Deterministic consolidated gate

`scripts/test-phase2-restart-evidence.sh` runs the narrow existing tests that
cover:

- client restart recovery without automatic transmission;
- canonical mutation identity persistence;
- external RPC field preservation/drop behavior;
- embedded direct/propagated/ticket mapping;
- pre-cancelled direct and request-Resource dispatch;
- SQLite and upload process-kill recovery;
- server-restart durable replay;
- replacement-Link replay;
- reaction/revision replay and conflict behavior;
- pending upload cleanup on Link or Resource failure.

It deliberately does not run Python environments, packaging, graphical smokes,
or the known-red maximum UDP Resource reproducer.

## Resource and compatibility impact

- No production code or dependency was added by this phase.
- The consolidated gate owns no persistent state and uses existing temporary
  roots.
- The live smoke has one bounded server process and one bounded client process,
  then joins them during cleanup.
- No automatic retry was added. Lost acknowledgements retain uncertain state
  until an explicit exact replay.
- No OMENchat wire/database change and no mixed-version behavior change.

## Open limitations

1. Install or otherwise provide the exact `reticulumd` 0.9.6 executable before
   running the external-daemon lifecycle lane. The repository will not install
   host tools or guess undocumented command-line arguments.
2. Current and pinned Python interoperability should be rerun at full release
   qualification, not for every small local patch.
3. Windows, macOS, Linux ARM64, package, display-server, and physical-interface
   evidence remains environment-bound.
4. The upstream UDP transmit buffer still cannot carry the maximum legal
   Resource wire packet; no incompatible application fragmentation was added.

## Next step

Phase 3 is safe to begin for capabilities already active in the authoritative
matrix. Work should start by reconciling replies/mentions end to end and fixing
only demonstrated gaps. External-daemon-specific behavior remains blocked
independently and must not delay unrelated local correctness work.
