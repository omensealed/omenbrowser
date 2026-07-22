# Reticulum-rs/LXMF 0.9.6 migration plan

Status date: 2026-07-21  
Current OMEN baseline: `d484724ff6e657531fd82ee98be8c5b670413354`
(`0.9.5-2`, exact Reticulum/LXMF 0.9.5 train)  
Target application release: `0.9.6-1` / tag `v0.9.6-1`  
Upstream tag: `v0.9.6`, annotated tag object
`e501ef828bc1ff029ec25610428cc667a3f3009a`, commit
`bb9200016d026357dd166ef03a05ffb75909bdd2`

This plan supersedes the proposed `v0.9.5-3` application release. The 0.9.5
baseline audit remains immutable evidence of the known-good starting behavior;
it is not rewritten as if those results were collected on 0.9.6.

## Admission decision

Admit the exact published 0.9.6 family as a coordinated successor train after
the current durable-mutation branch is at a clean, green rollback point. Do not
mix 0.9.5 and 0.9.6 upstream type identities, add a Git dependency, or update an
unrelated crate as collateral work.

Registry inspection confirms these packages are published at 0.9.6, retain
Rust 1.85, and retain the features OMEN currently selects:

| OMEN root | Package | Planned exact pin | Selected features |
| --- | --- | --- | --- |
| desktop | `reticulum-rs` | `=0.9.6` | defaults off; `core`, `transport` |
| desktop/server | `reticulum-rs-transport` | `=0.9.6` | upstream defaults (empty) |
| desktop | `reticulum-rs-rpc` | `=0.9.6` | current explicit boundary |
| desktop | `lxmf` | `=0.9.6` | defaults off; `wire` |
| desktop | `lxmf-sdk` | `=0.9.6` | defaults off; `std`, `sdk-async`, `rpc-backend` |

The standalone server remains an independent Cargo root and does not acquire
LXMF SDK/RPC or desktop dependencies. The private `omen-ifac-tcp` package keeps
its independent version unless its own API or wire behavior changes.

## Upstream 0.9.6 disposition

The official release describes 0.9.6 as stabilization/hardening rather than a
wire-version redesign. Relevant OMEN candidates are:

- adopt fallible `Identity::try_new_from_slices` at untrusted public-key
  admission boundaries while retaining typed internal conversions;
- adopt `WireMessage::try_message_id` anywhere encoding can fail, instead of
  allowing an encoding failure to collapse to an empty-payload hash;
- verify packet-cache-correlated receipt proofs against OMEN's existing
  peer-unconfirmed proof model; do not relabel transport proof as final LXMF
  delivery;
- regression-test link-context traffic after upstream changed fan-out helpers
  to each Link's bound interface and added structured `LinkSendReport` APIs;
- preserve and surface stricter SDK propagation-state decode errors rather than
  converting malformed authoritative state to absence;
- re-run shutdown/reconnect tests because interface workers now stop on closed
  receive queues and more task, stream, process, and storage failures propagate;
- test representative copies of any upstream-managed legacy message database
  before relying on its transactional migration changes.

No production dependency is added solely to expose a new 0.9.6 surface.
`lxmf-runtime`, embedded crates, ZeroMQ defaults, tools, and daemons retain their
existing dispositions.

## Gaps not closed by release notes or source inspection

These remain open until an OMEN regression proves otherwise:

- stock TCP client/server source still shows Packet-to-HDLC handling without
  the Python-compatible IFAC transform; retain the narrow project IFAC client
  and stock-server fail-closed policy;
- the UDP and Resource wire changes do not demonstrate a fix for OMEN's maximum
  Resource packet failure; rerun the exact two-process boundary test;
- `lxmf-sdk` with `rpc-backend` still depends on `rustls-pemfile 2.2.0`, so the
  existing advisory/maintenance disposition remains;
- Wayland/`wayland-scanner` is outside this upstream train and is unaffected;
- inbound plain Resource delivery gained stronger regression evidence, not a
  new OMEN application-admission callback or tighter pre-allocation limit;
- sender-side incremental Resource progress is not claimed by the release;
- stock IFAC, physical interfaces, public-network behavior, current Python,
  NomadNet, and third-party clients still require their own evidence.

## Execution units

### Unit 0 — freeze the 0.9.5 rollback point

Status: complete on 2026-07-21. The rollback baseline is commit `d484724`, and
CI run `29871754030` completed successfully. The planning-only successor commit
is `4e1c048`; it changes no product behavior or dependency resolution.

1. Record the final durable-mutation commit and passing CI URL.
2. Save root/server metadata and dependency trees under ignored evidence.
3. Run the canonical 0.9.5 desktop/server checks needed to distinguish migration
   failures from pre-existing ignored hardware/live tests.
4. Do not activate the OMENchat durable capability as part of the dependency
   bump; persistent intent/send/recovery remains a separate behavior unit.

Gate: clean tree, known-good commit, independent lockfiles, and explicit tests
not run.

### Unit 1 — exact dependency and application-version alignment

Status: complete locally on 2026-07-21; CI qualification remains pending until
the compiler-guided Unit 2 validation is batched with it. Both lockfiles resolve
one registry-sourced 0.9.6 train, active release/mixed-version checks identify
`0.9.6-1`, and canonical desktop plus both standalone-server profiles compile.

1. Set root and server application versions to `0.9.6-1` together.
2. Change only the five direct upstream family pins listed above to `=0.9.6`.
3. Resolve root and server lockfiles independently with `--precise 0.9.6`.
4. Verify no production Reticulum/LXMF 0.9.5 package remains and no registry/Git
   split or incompatible duplicate exists.
5. Update release/version verification, CI labels, packaging metadata, and
   mixed-version script expectations in the same unit.

Gate: both roots resolve one registry-sourced 0.9.6 train. Compilation failures
may remain only when recorded in the API ledger.

### Unit 2 — compiler-guided API migration

Status: complete locally on 2026-07-21. Canonical desktop, TUI, mock-runtime,
standalone headless, and standalone full suites pass; strict desktop, TUI, and
headless Clippy pass. Fallible 0.9.6 identity and message-ID APIs are used at
persisted/untrusted boundaries, with malformed-key regressions. Native-platform
CI and interop remain Unit 4 gates, not local compilation claims.

1. Run narrow checks from identity/core through transport, runtime, LXMF wire,
   SDK/RPC, omenchatd, GUI, and TUI.
2. Update `docs/migration/RETICULUM_RS_0_9_API_LEDGER.md` with every semantic
   change, not only compiler edits.
3. Prefer the new fallible identity and message-ID APIs at untrusted/fallible
   boundaries and add malformed-input regressions before removing compatibility
   calls.
4. Preserve project DTOs and runtime traits; upstream types do not enter UI or
   storage ownership layers.
5. Do not weaken error handling to retain 0.9.5 best-effort behavior where 0.9.6
   intentionally fails closed.

Gate: canonical desktop, mock, TUI, standalone headless, and standalone full
profiles compile and their unit/component tests pass with strict Clippy.

### Unit 3 — gap and opportunity qualification

Status: in progress. The maximum UDP Resource sentinel was run explicitly on
0.9.6 and remains known-red: upstream buffer 456 bytes versus a 483-byte maximum
type-one wire packet. It remains an accurately scoped upstream limitation, not
a reason for a local fork or weakened test. The complete immutable pinned-Python
lane passed on 2026-07-21 after its standalone relocation build was isolated
from canonical Cargo artifacts. One preceding run intermittently timed out in
the second network propagation stamp admission; the same case passed on the
immediate full rerun, so the flake remains disclosed and is not counted as two
independent clean qualifications. TCP IFAC, forged/stale proof handling,
propagation sync, deterministic stamp boundaries, ticket policy, direct stamp,
and 64-KiB direct Resource cases passed in both runs before or after that point
as applicable.

Run focused before/after tests for:

- direct/opportunistic packet proof correlation and stale/forged proof
  rejection;
- active-Link/bound-interface OMENchat and LXMF traffic;
- inbound plain Resource and maximum Resource boundary behavior;
- stock TCP IFAC negative evidence and project IFAC positive interop;
- malformed identity/hash and fallible LXMF message-ID paths;
- SDK propagation decode failures and event-stream reconnect/gap recovery;
- interface worker shutdown, link reuse, cancellation, and task/handle growth;
- upstream-managed database migration rollback on representative copies.

Only then update `docs/RETICULUM_TRANSPORT_API_GAP.md` entries to fixed,
retained, or superseded. No workaround is removed from release notes alone.

### Unit 4 — interoperability, performance, and release qualification

Status: in progress. The separately versioned current-Python drift lane was
advanced to the 2026-07-21 PyPI snapshot (RNS 1.4.0, LXMF 1.1.0, NomadNet
1.2.7) and passed its complete informational matrix. This does not replace the
immutable pinned-reference gate. Mixed-version, state-reopen, native packaging,
and before/after desktop/server resource measurements remain. The published
`v0.9.5-2` adjacent binary now passes bidirectional direct Link packets, 64-KiB
Resources, restart/state reopening, and both propagation directions. The
`0.9.6-1` sender to `0.9.5-2` recipient direction requires the documented
two-sync unknown-sender recovery without resending the logical mutation.
Adjacent OMENchat qualification also passes SQLite state reopening, live room
traffic, orderly server restart, and Resource-backed history in both client and
server directions. Adjacent propagation qualification also passes orderly node
restart, abrupt node crash with persisted queue recovery, and required
stamp/ticket wire handling through the explicit unknown-sender recovery path.
GitHub runs `29877719914` and `29877720971` pass at commit
`5905d0b`, covering
Linux quick checks, native Windows/Intel macOS/Apple Silicon macOS matrices,
pinned and current Python lanes, and the existing full `0.6.0-1` mixed matrix.
The expanded adjacent LXMF hosted matrix passes in Python interoperability run
`29884168183` at commit `8d9bcd5`. The adjacent OMENchat additions remain local
until the next interoperability checkpoint so ordinary development does not
dispatch the roughly half-hour workflow for each small commit.
An equivalent local release-profile resource comparison at commit `2721e96`
also passes: desktop idle CPU is effectively unchanged, RSS rises 0.47%,
private-dirty memory falls 0.03%, and the bounded omenchatd database and
backpressure fixtures retain their queue, latency, FD, cleanup, and memory
gates. Raw measurements remain under the ignored `target/` evidence root.

- Keep the pinned Python lane at the immutable upstream references used by
  0.9.6 (`RNS 15320e4...`, `LXMF 727830c...`, conformance `0319444...`).
- Run the separately versioned current-Python drift lane.
- Run NomadNet, direct/propagated LXMF, OMENchat, omenchatd restart, Resource,
  IFAC, mixed `0.9.5-2`/`0.9.6-1`, and state-reopen suites.
- Complete the longer task/link soaks and any hardware-dependent measurements;
  the equivalent short desktop, SQLite, and backpressure comparisons against
  the saved 0.9.5 baseline pass.
- Run native packaging only after Linux product and standalone gates pass;
  native Windows/macOS CI remains authoritative for those platforms.
- Update README claims only from collected OMEN evidence.

Gate: release checklist names every unavailable live/hardware test and no
unsupported upstream claim is promoted to an OMEN support claim.

## Rollback

The dependency/version alignment must remain one revertible unit. No OMEN
protocol, destination, configuration, identity path, or SQLite schema number is
changed merely because the application becomes 0.9.6-1. Preserve 0.9.5-2
binaries and representative state copies until reopen, mixed-version, and
package smoke gates pass. If 0.9.6 introduces a release blocker, revert the
manifest/lock/API unit and continue safe non-network work on the known-good
0.9.5 baseline; do not introduce a private upstream fork.

## Official upstream evidence

- <https://github.com/FreeTAKTeam/LXMF-rs/releases/tag/v0.9.6>
- <https://github.com/FreeTAKTeam/LXMF-rs/blob/v0.9.6/docs/release-notes-v0.9.6.md>
- <https://github.com/FreeTAKTeam/LXMF-rs/blob/v0.9.6/docs/status/v0.9.6-hardening-audit.md>
- <https://github.com/FreeTAKTeam/LXMF-rs/compare/v0.9.5...v0.9.6>
