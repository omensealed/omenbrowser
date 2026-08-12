# Dependency security

This document describes the current policy. Version-specific remediation
history remains in published release notes and Git history.

## Current advisory policy

Both independent lockfiles must pass `cargo audit` with zero vulnerabilities.
There are no accepted advisory IDs and `deny.toml` contains no advisory
ignores.

`scripts/verify-accepted-advisories.sh` additionally verifies the exact
reviewed Wayland build path:

```text
wayland-scanner 0.31.11 proc-macro -> quick-xml 0.41.0
```

The fixed `quick-xml` package has exactly that parent in the selected desktop
graph, repository Rust source does not import it, and standalone `omenchatd`
does not resolve it. The script fails if the advisory set grows, the dependency
path changes, or an old broad exception reappears.

Run:

```bash
bash scripts/verify-accepted-advisories.sh
cargo audit --locked
cargo audit --locked --file src/server/Cargo.lock
```

## Source and license policy

`cargo-deny` enforces the reviewed license set, registry sources, wildcard
requirements, and dependency bans for both Cargo roots:

```bash
cargo deny --locked --all-features check licenses bans sources
cargo deny --manifest-path src/server/Cargo.toml \
  --locked --all-features check licenses bans sources
```

Reticulum/LXMF dependencies must remain on the exact official registry train
reported by `scripts/verify-reticulum-train.sh`. Git dependencies, private
forks, vendored transport code, and `[patch.crates-io]` overrides are not
accepted release inputs.

## Feature reachability

A lockfile entry is not proof that code is active in a product. Security review
must record:

- exact package and version;
- reverse dependency path;
- selected product feature closure;
- runtime, build-time, test-only, or lock-only reachability;
- standalone-server presence;
- direct source imports;
- fixed version or upstream constraint.

Canonical product identities are checked by
`scripts/verify-product-features.sh` and
`scripts/verify-tui-dependencies.sh`. Do not remove a required product feature
or enable a dormant feature merely to alter an audit graph.

## Secret and input boundaries

Dependency checks do not replace application controls. Production paths must
retain bounded parsers, owner-only private storage, identity validation,
diagnostic redaction, explicit overload rejection, and the prohibition on
automatic replay after uncertain network dispatch.

## Updating dependencies

Use the narrowest compatible update that resolves a confirmed issue. Regenerate
the root and server lockfiles independently, inspect the complete diff, verify
product feature closures, and rerun audit/deny plus the relevant functional and
platform gates. Unrelated dependency movement is not part of a security patch.
