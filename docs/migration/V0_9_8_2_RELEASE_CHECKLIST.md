# v0.9.8-2 release qualification checklist

Target: `v0.9.8-2`

Released baseline: `v0.9.8-1` /
`c7cc638fed88dbb8bf20375b3dfc34b8b872df58`

## Release scope

- [x] Stale native NomadNet Link setup expires its cached route.
- [x] Recovery emits one path-discovery request before application dispatch.
- [x] Automatic retry is limited to pre-dispatch Link setup.
- [x] Request-send/response-wait uncertainty requires explicit manual Retry.
- [x] Live isolated native NomadNet fetch returns the exact page.
- [x] Exact official registry Reticulum/LXMF 0.9.8 remains unchanged.
- [x] No protocol or persistent-state migration exists.
- [x] Maximum-UDP Resource sentinel remains independently visible.

## Qualification

- [x] Focused stale-path, retry-state, and native request tests.
- [x] Formatting and strict desktop Clippy.
- [x] Root/server quick canonical checks and focused release tests.
- [ ] Full release check and package candidate.
- [ ] Hosted CI and Python interoperability.
- [ ] Native Windows/macOS and Linux package gates.
- [ ] Reviewed candidate merged to `main`.
- [ ] Annotated tag and published release.

## External boundaries

- [ ] Upstream maximum-UDP Resource boundary fixed and sentinel passes.
- [ ] Stock IFAC parity proven sufficiently to remove the local adapter.
- [ ] Physical interface/radio testing.

Tagging and publication require all applicable release gates to pass.
