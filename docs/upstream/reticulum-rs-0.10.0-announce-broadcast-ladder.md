# Reticulum 0.10.0 announce broadcast ladder

Reviewed: 2026-08-27

Scope: exact official crates.io `reticulum-rs-transport =0.10.0`, immutable
LXMF-rs tag `v0.10.0`, commit `5436ee715f94f81e18abb0808cfca52fcd7cc9bc`.

Official record: [issue #578](https://github.com/FreeTAKTeam/LXMF-rs/issues/578)
is open. [PR #579](https://github.com/FreeTAKTeam/LXMF-rs/pull/579) and
[PR #580](https://github.com/FreeTAKTeam/LXMF-rs/pull/580) are open and
unmerged. No newer official release existed at review time.

## Evidence and impact

Official source review identifies three missing policy rungs: path expiry,
random-blob replacement, and emission-limit handling. That is source-backed but
not a claim that every OMEN topology reproduces every rung. OMEN may therefore
miss announce rebroadcast opportunities in affected routed topologies.

## Safe operation and boundary

Treat path/announce visibility as diagnostics, not delivery evidence. Do not
add a second dispatch, automatic replay, transport-role change, copied PR, or
local policy substitute. OMEN carries no local patch, fork, vendor copy, or
`[patch.crates-io]` override.

Removal condition: select an official published fixed release and pass exact
multi-node expiry, random-blob replacement, emission-limit, dedupe, and
one-dispatch qualification before promoting the capability marker.
