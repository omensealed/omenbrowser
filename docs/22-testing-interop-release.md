# 22 — Testing, Interop, and Release Gates

This document defines the final quality gates for OMENbrowser_rs.

## Test categories

### Unit tests

Required for:

- address parsing;
- Micron parsing/rendering;
- browser session history/cache/partials;
- message store;
- directory service;
- settings/identity storage;
- interface config rendering;
- input buffer;
- mouse hit mapping;
- plugin manifest/capabilities;
- error redaction.

### Service tests

Required for:

- browser service with mock runtime;
- messaging service with mock runtime;
- directory + runtime announce flow;
- diagnostics snapshots;
- interface apply behavior;
- cache cleanup/statistics.

### Feature compile tests

Required commands:

```bash
cargo check
cargo test
cargo check --features live-reticulum
cargo test --features live-reticulum
cargo check --features live-lxmf
cargo test --features live-lxmf
```

If live feature tests require local Reticulum interfaces or unavailable native APIs, keep compile tests and document integration test requirements.

### Snapshot tests

Use fixtures for:

- Micron normal pages;
- OMEN art-heavy pages;
- half-block image-like rows;
- 40-column pages;
- 60-column pages;
- 71-column NomadNet-style pages;
- 80-column terminal pages;
- malformed markup preservation;
- controls/links hit regions.

### Interop tests

Where possible, compare behavior to archived Python OMENbrowser.

For native live Reticulum/LXMF validation, use the command runbook in:

```text
docs/24-native-live-runbook.md
```

For a shorter private-alpha checklist that can be handed to outside testers, use:

```text
docs/27-alpha-test-runbook.md
```

Interop areas:

- identity display hash;
- request-data key names;
- address normalization;
- Micron parser output;
- partial descriptor parsing;
- cache TTL parsing;
- LXMF message field mapping;
- directory/trust levels;
- interface config output.

## Manual test matrix

### Mock mode

- start app;
- open mock index;
- create/close browser tabs;
- navigate back/forward/reload;
- download mock file;
- create conversation;
- send mock direct message;
- send mock propagated message;
- sync mock inbound messages;
- open directory entry;
- view diagnostics;
- toggle settings/interfaces where implemented;
- quit cleanly.

### Live mode

- create managed identity;
- attach existing identity;
- start with TCP client interface;
- start with TCP server interface if supported;
- start with I2P profile if supported;
- start with RNode profile if supported;
- receive announces;
- request path to known destination;
- fetch a page;
- follow link;
- submit field/form;
- partial refresh;
- send direct LXMF;
- send propagated LXMF;
- receive LXMF;
- inspect diagnostics;
- shutdown cleanly.

## CI gates

Minimum CI:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo check --no-default-features
```

Optional feature CI:

```bash
cargo check --features live-reticulum
cargo check --features live-lxmf
```

If live crates are unstable, mark CI failures clearly and avoid blocking mock/default builds until native APIs stabilize.

## Release artifacts

Initial Linux release artifacts:

- source tarball;
- binary tarball;
- install script if needed;
- sample config;
- README quickstart;
- changelog;
- license file.

Later:

- AppImage;
- deb/rpm packages;
- Arch PKGBUILD;
- shell completions;
- man page.

## CLI behavior

Expected flags:

```text
omenbrowser-rs
  --config-dir <path>
  --data-dir <path>
  --reticulum-config-dir <path>
  --mock-runtime
  --live-runtime
  --log-level <level>
  --export-diagnostics <path>
  --no-plugins
  --theme <name>
```

Do not make live runtime the only way to start. Mock runtime is valuable for tests and offline UI development.

## Terminal recovery

Release gate:

- alternate screen exits correctly;
- raw mode disabled on panic/error;
- mouse capture disabled on exit;
- cursor restored;
- logs show crash details.

Add a panic hook or terminal guard drop behavior if not already present.

## Version and diagnostics

Diagnostics should include:

- app version;
- git commit if available;
- build feature flags;
- runtime mode;
- live crate versions if available;
- OS/terminal info;
- config/data paths with redaction policy;
- identity display hash;
- interface summary;
- message/directory/cache counts;
- last errors.

## Definition of release candidate

A release candidate must:

- pass default CI;
- start in mock mode;
- run without terminal corruption;
- support basic browsing/messaging in mock mode;
- compile live feature(s) or document exact blocker;
- include migration notes from Python OMENbrowser;
- include known limitations;
- preserve user data safety.
