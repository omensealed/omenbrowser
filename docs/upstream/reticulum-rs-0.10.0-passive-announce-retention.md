# Reticulum 0.10.0 passive announce retention

Reviewed: 2026-08-27

Scope: exact official crates.io `reticulum-rs-transport =0.10.0`, immutable
LXMF-rs tag `v0.10.0`, commit `5436ee715f94f81e18abb0808cfca52fcd7cc9bc`.

Official record: [issue #581](https://github.com/FreeTAKTeam/LXMF-rs/issues/581)
is open. [PR #582](https://github.com/FreeTAKTeam/LXMF-rs/pull/582) is open and
unmerged. No newer official release existed at review time.

## Evidence and impact

The upstream report and tagged source show that the private announce table is
not expired while transport is disabled. This source-level conclusion is
inferred for OMEN's exact dependency train; OMEN cannot inspect the private
table through a published API. OMEN's production role is passive
(`enable_transport = No`), so long-running announce-heavy nodes may exhibit RSS
growth. Process RSS is observational and is not an internal table count.

## Safe operation and boundary

Monitor repeated same-host RSS samples on long-running announce-heavy nodes.
Do not enable transport merely to avoid this symptom and do not automatically
restart a process to conceal it. OMEN carries no local patch, fork, vendor copy,
or `[patch.crates-io]` override.

A controlled manual lane must use temporary browser/server roots, transport
disabled, bounded duration, a named announce-producing topology, and reliable
child cleanup. Missing traffic, missing tools, early exit, or unavailable
network is `unavailable`, never `pass`. Because the private table has no public
accessor, v0.10.0-5 does not ship a placebo automated counter.

Removal condition: select an official published fixed release, rerun structural
source review, and record repeated bounded process evidence under real
announce-heavy traffic before promoting the capability marker.
