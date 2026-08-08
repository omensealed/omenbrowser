# v0.9.8-3 NomadNet responder execution record

Baseline: `v0.9.8-2` at
`e19b3ee726afaee4a5575062eaacd2ccce14a4bd`

## Reproduced behavior

An isolated current Rust browser sent a small direct request to a Rust quiet
portal containing an incompressible 32 KiB page. The old responder attempted a
direct packet for the 32,790-byte packed response, logged `OutOfMemory`, sent no
response Resource, and the browser timed out. This reproduced the request-
coupled response branch before production behavior changed.

## Implementation decision

Both request ingress paths now call one bounded responder. It validates the
16-byte request ID and path, reads and packs the response once in an awaited
blocking job, then selects direct packet or response Resource solely from the
complete packed envelope length. Selection uses public `PACKET_MDU`; no
negotiated-MTU private formula, retry, fallback, or second dispatch was added.

The complete envelope ceiling is 4 MiB. Portal contents and request data are not
logged. The Reticulum/LXMF train remains exact registry 0.9.8.

## Evidence

- Baseline root/server tests and quick release gate passed before production
  edits.
- The same isolated process smoke now returns the exact 32,768-byte page through
  a response Resource from a direct request.
- In-process Rust transport tests cover all four request/response primitive
  combinations, exact correlation and bytes, boundary selection, duplicate
  observation, and failure without fallback.
- The pinned Python RNS reference requester passes all four combinations against
  the Rust responder.
- Current Python RNS 1.4.0 passes the same reverse matrix. The complete current
  drift lane passes with LXMF 1.1.0 and NomadNet 1.2.7; the release-profile
  request measurements reported direct median/p95 38,732/40,898 microseconds
  and request-Resource median/p95 42,949/45,335 microseconds on this host.
- Full release checks, local Linux packaging and package smoke, standalone
  relocation, ARM64 Cross/QEMU tests and lifecycle packaging, current upload,
  continuous reconnect, and the maintained mixed 0.6.0-1/0.9.8-3 lanes pass.
- The deliberately ignored maximum-UDP sentinel remains failing at the known
  456-byte transmit-buffer versus 483-byte wire-packet boundary.

## Post-candidate routed Resource finding

After the responder and route-recovery changes passed hosted CI, isolated
attachment smokes separated direct/local behavior from routed behavior. Direct
transfers completed at 873 and 54,427 bytes. A fresh client over a multi-hop
TCP-gateway route completed 873 bytes but a 13,613-byte incompressible Resource
stalled after repeated Resource requests and reached the bounded retry limit.
The same failure occurred through either available gateway route, so route
selection alone was not the remaining cause.

The pinned Rust transport duplicate filter permits repeated Resource-request
packets but not repeated Resource-data packets; the installed Python Reticulum
reference permits both. No OMEN-level replay, fragmentation, dependency patch,
or fork was added. Publication is blocked until this routed retransmission
boundary is corrected and requalified, or a separately reviewed compatible
design is accepted.

Full release and external platform results are recorded in the release
checklist and final implementation report; unavailable lanes are not inferred.
