# v0.9.9-2 release qualification checklist

Target: `v0.9.9-2`

- [x] Exact official registry Reticulum/LXMF 0.9.9 train retained in both roots.
- [x] No Git, fork, vendor, private registry, or patch override added.
- [x] Documentation contradiction corrected and capability ledger verified.
- [x] Split metadata remains a normal passing regression.
- [x] Routed fragment-loss and maximum-UDP sentinels remain separately named,
      ignored, visible, and protected by negative fixture tests.
- [x] Unsupported external RPC guarantees reject before connection/dispatch.
- [x] Four-quadrant Request/Response exact-byte and one-dispatch test passes.
- [x] Root/server full tests and strict Clippy pass before the version bump.
- [x] Final candidate quick/full/package, smoke, Python, mixed-release,
      performance, rollback, and local ARM64 Cross/QEMU evidence recorded.
- [ ] Candidate-SHA native Windows/macOS and hosted workflow evidence recorded
      after explicit push authorization.

The remaining unchecked item is an external execution boundary, not permission
to weaken a release gate. See `V0_9_9_2_MAINTENANCE_EXECUTION.md` for exact
local results and unavailable hosted boundaries.
