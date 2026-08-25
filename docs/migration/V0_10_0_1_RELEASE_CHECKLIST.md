# v0.10.0-1 release qualification checklist

Target: `v0.10.0-1`

- [x] Exact official registry Reticulum/LXMF 0.10.0 train retained in both roots.
- [x] No Git, fork, vendor, private registry, or patch override added.
- [x] Current documentation and capability ledger match final candidate evidence.
- [x] Split metadata passes as a normal regression on official 0.10.0.
- [x] Routed fragment-loss and maximum-UDP sentinels remain separately named,
      ignored, visible, and protected by negative fixture tests.
- [x] Unsupported external RPC guarantees reject before connection/dispatch.
- [x] Four-quadrant Request/Response exact-byte and one-dispatch test passes.
- [x] Root/server canonical products and strict Clippy pass on the candidate.
- [x] Reconnect failure matrix and 100+ cycle bounded soak pass.
- [x] Direct/local upload failure matrix and storage invariants pass.
- [x] Final candidate quick/full/package, smoke, Python, mixed-release,
      performance, rollback, and local ARM64 Cross/QEMU evidence recorded.
- [x] Candidate-SHA native Windows/macOS and hosted workflow evidence classified
      unavailable without push authorization; maintained static workflow checks
      pass and no native-platform result is inferred.

Unchecked items remain required unless the final evidence report classifies an
external lane as unavailable. Prior v0.9.9-2 evidence is baseline evidence only,
not proof for the 0.10.0-1 candidate.

Detailed evidence: `V0_10_0_1_RELEASE_EVIDENCE.md`.
