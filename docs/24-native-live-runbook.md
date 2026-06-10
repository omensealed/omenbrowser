# 24 - Native Live Runbook

This runbook is for validating native Rust Reticulum/LXMF behavior against real peers. It assumes the default mock runtime already passes tests and focuses only on live `native-network` commands.

The current live goal is not broad release certification. The goal is to collect precise evidence for the next patch target:

- NomadNet page fetch: config, identity, path, link, request framing, response wait, or decode.
- LXMF delivery: local announce, peer identity, peer path, packet build/send, packet proof, or inbound reply.

## Required Inputs

Prepare these values before running commands:

```text
<app-root>                     temporary OMENbrowser_rs app data directory for this run
<identity-file>                Reticulum identity file to attach
<reticulum-config-dir>         Reticulum config directory, if using an existing config
<tcp-host:tcp-port>            reachable Reticulum TCP peer endpoint
<nomadnet-destination-hash>    32 hex character NomadNet node destination hash
<nomadnet-path>                page path, normally / for first validation
<lxmf-peer-destination-hash>   32 hex character lxmf.delivery destination hash
<known-destinations-file>      optional Python/RNS-compatible storage/known_destinations
```

Use a throwaway `<app-root>` when possible:

```bash
mkdir -p /tmp/omenbrowser-rs-live
```

## Output Rules

For `--stdout` commands:

- stdout is pretty JSON and can be redirected to a file.
- stderr contains the short human-readable summary.
- The summary includes outcome, stage, reason, and next step.
- Add `--suggest-shell` when you want stderr to also include shell-escaped one-line versions of the report's `suggested_commands`.
- If `--bundle-report <dir>` is also used, the bundle path is printed to stderr so stdout remains JSON.

Recommended pattern:

```bash
cargo run --features native-network -- <command args> \
  > /tmp/omenbrowser-rs-live/report.json \
  2> /tmp/omenbrowser-rs-live/report.summary.txt
```

Inspect the summary first:

```bash
cat /tmp/omenbrowser-rs-live/report.summary.txt
```

Then inspect the JSON fields named in the summary:

```bash
jq '.classification' /tmp/omenbrowser-rs-live/report.json
```

For issue reports, prefer `--bundle-report <dir>` because it writes one timestamped directory with:

- `bundle.json`: bundle schema/version, command kind, creation time, and file list.
- `report.json`: the structured smoke or LXMF interop report.
- `summary.txt`: the same human-readable summary printed for `--stdout`.
- `command.json`: command kind, redacted argv, local overrides, and creation time.
- `environment.json`: app version, target OS/arch, active cargo features, and non-secret environment hints.
- `logs.json`: up to 50 recent structured log entries from `logs/omenbrowser_rs*.jsonl`, with message bodies and path-like CLI values redacted.

Example:

```bash
cargo run --features native-network -- \
  --native-smoke <nomadnet-destination-hash>:<nomadnet-path> \
  --backend reticulum \
  --app-root <app-root> \
  --identity <identity-file> \
  --tcp-client <tcp-host:tcp-port> \
  --warm-path \
  --path-wait 10 \
  --stdout \
  --bundle-report /tmp/omenbrowser-rs-live/bundles \
  > /tmp/omenbrowser-rs-live/report.json \
  2> /tmp/omenbrowser-rs-live/report.summary.txt
```

The bundle metadata redacts path values such as identity, Reticulum config, known destinations, app root, output path, and bundle root. It keeps only path hints such as file names and whether the path was absolute.

## Native Runtime Startup

Before debugging a specific NomadNet page or LXMF peer, verify that the configured native runtime can start and report interface/network state:

```bash
cargo run --features native-network -- \
  --native-startup \
  --backend reticulum \
  --app-root <app-root> \
  --identity <identity-file> \
  --tcp-client <tcp-host:tcp-port> \
  --stdout \
  --bundle-report /tmp/omenbrowser-rs-live/bundles \
  > /tmp/omenbrowser-rs-live/startup.json \
  2> /tmp/omenbrowser-rs-live/startup.summary.txt
```

This command starts the selected runtime through the same `NetworkRuntime` boundary used by the TUI, collects `runtime_status_after`, `interface_stats`, and `network_snapshot`, then stops cleanly.

## NomadNet Receive/Path Readiness

Run preflight before live fetch or LXMF delivery. This validates CLI/config inputs and writes the same bundle-compatible report shape without starting a page fetch or LXMF send.
It also performs a bounded transport-startup check and then stops the runtime, so inspect `.stages[] | select(.stage == "transport_startup")` before moving to live traffic.
Use `--preflight-wait <ms>` to increase the runtime event collection window on slow peers; the default is 250 ms.
When compiled with `native-rns-net` or `native-network`, preflight also semantically parses `--known-destinations` and reports whether the target destination identity is present.
The preflight report includes `.suggested_commands[]` as redacted argv arrays. Use those as templates for the next dry probe, live fetch, or LXMF interop command.
Native smoke and LXMF interop reports also include `.suggested_commands[]`, so inspect those after a live failure before changing code.
The JSON argv arrays are canonical. `--suggest-shell` only renders copy/paste-oriented shell command lines in stderr and `summary.txt`.

```bash
cargo run --features native-network -- \
  --native-preflight <nomadnet-destination-hash>:<nomadnet-path> \
  --backend reticulum \
  --app-root <app-root> \
  --identity <identity-file> \
  --tcp-client <tcp-host:tcp-port> \
  --known-destinations <known-destinations-file> \
  --preflight-wait 1000 \
  --stdout \
  --bundle-report /tmp/omenbrowser-rs-live/bundles \
  > /tmp/omenbrowser-rs-live/preflight.json \
  2> /tmp/omenbrowser-rs-live/preflight.summary.txt
```

To include LXMF peer input validation:

```bash
cargo run --features native-network -- \
  --native-preflight <nomadnet-destination-hash>:<nomadnet-path> \
  --send-lxmf-smoke <lxmf-peer-destination-hash> \
  --backend reticulum \
  --app-root <app-root> \
  --identity <identity-file> \
  --tcp-client <tcp-host:tcp-port> \
  --stdout
```

## One-Step Native Validation

After startup and preflight are clean, `--native-validate` is the shortest command for the real NomadNet page-loading path. It selects the Reticulum backend by default, requests path warmup for 10 seconds unless `--path-wait` is supplied, runs the live probe, and calls the normal `fetch_page` path.

```bash
cargo run --features native-network -- \
  --native-validate <nomadnet-destination-hash>:<nomadnet-path> \
  --app-root <app-root> \
  --identity <identity-file> \
  --tcp-client <tcp-host:tcp-port> \
  --known-destinations <known-destinations-file> \
  --stdout \
  --bundle-report /tmp/omenbrowser-rs-live/bundles \
  > /tmp/omenbrowser-rs-live/native-validate.json \
  2> /tmp/omenbrowser-rs-live/native-validate.summary.txt
```

Add `--send-lxmf-smoke <lxmf-peer-destination-hash> --lxmf-wait 30` to include direct LXMF send-and-wait evidence in the same report.

Start with a dry smoke report. This does not fetch a page.

```bash
cargo run --features native-network -- \
  --native-smoke <nomadnet-destination-hash>:<nomadnet-path> \
  --backend reticulum \
  --app-root <app-root> \
  --identity <identity-file> \
  --tcp-client <tcp-host:tcp-port> \
  --warm-path \
  --path-wait 10 \
  --stdout \
  > /tmp/omenbrowser-rs-live/nomadnet-dry.json \
  2> /tmp/omenbrowser-rs-live/nomadnet-dry.summary.txt
```

If using an existing Reticulum config instead of a direct TCP override:

```bash
cargo run --features native-network -- \
  --native-smoke <nomadnet-destination-hash>:<nomadnet-path> \
  --backend reticulum \
  --app-root <app-root> \
  --identity <identity-file> \
  --reticulum-config <reticulum-config-dir> \
  --warm-path \
  --path-wait 10 \
  --stdout \
  > /tmp/omenbrowser-rs-live/nomadnet-dry.json \
  2> /tmp/omenbrowser-rs-live/nomadnet-dry.summary.txt
```

Optional known-destinations preload:

```bash
cargo run --features native-network -- \
  --native-smoke <nomadnet-destination-hash>:<nomadnet-path> \
  --backend reticulum \
  --app-root <app-root> \
  --identity <identity-file> \
  --tcp-client <tcp-host:tcp-port> \
  --known-destinations <known-destinations-file> \
  --warm-path \
  --path-wait 10 \
  --stdout \
  > /tmp/omenbrowser-rs-live/nomadnet-known-destinations.json \
  2> /tmp/omenbrowser-rs-live/nomadnet-known-destinations.summary.txt
```

Key fields:

- `.classification.outcome`
- `.classification.stage`
- `.classification.next_step`
- `.verdicts.config`
- `.verdicts.runtime_startup`
- `.verdicts.destination_identity`
- `.verdicts.path_discovery`
- `.verdicts.destination_inspection`
- `.path_warmup.wait`
- `.dry_run_page_probe.report.steps`
- `.known_destinations_preload.loaded`

Patch target mapping:

- `classification.stage = config`: feature/config wiring or CLI invocation.
- `classification.stage = runtime_startup`: native runtime startup, identity path, or interface config.
- `classification.stage = address_parse`: browser address parsing or command input.
- `classification.stage = destination_identity`: known-destinations loading or announce ingestion.
- `classification.stage = path_discovery`: request_path/path event routing.
- `classification.stage = destination_inspection`: inspection state/key store/path state mismatch.

## NomadNet Live Probe

Run this only after dry readiness is either passing or blocked at a stage you intentionally want to inspect.

```bash
cargo run --features native-network -- \
  --native-smoke <nomadnet-destination-hash>:<nomadnet-path> \
  --backend reticulum \
  --app-root <app-root> \
  --identity <identity-file> \
  --tcp-client <tcp-host:tcp-port> \
  --known-destinations <known-destinations-file> \
  --warm-path \
  --path-wait 10 \
  --live \
  --stdout \
  > /tmp/omenbrowser-rs-live/nomadnet-live-probe.json \
  2> /tmp/omenbrowser-rs-live/nomadnet-live-probe.summary.txt
```

Key fields:

- `.classification`
- `.live_page_probe.report.steps`
- `.live_stage_subreport`
- `.verdicts.link_setup`
- `.verdicts.request_send`
- `.verdicts.response_wait`
- `.verdicts.response_decode`

Patch target mapping:

- `classification.stage = link_setup`: `rns-net` link creation, identity/key mapping, or destination aspect mismatch.
- `classification.stage = request_send`: NomadNet request path/data frame construction.
- `classification.stage = response_wait`: remote node availability, request timeout, or response callback routing.
- `classification.stage = response_decode`: response body extraction, bytes/string handling, or Micron decode assumptions.

## NomadNet Live Fetch

This exercises the normal runtime `fetch_page` path after the probe stages.

```bash
cargo run --features native-network -- \
  --native-smoke <nomadnet-destination-hash>:<nomadnet-path> \
  --backend reticulum \
  --app-root <app-root> \
  --identity <identity-file> \
  --tcp-client <tcp-host:tcp-port> \
  --known-destinations <known-destinations-file> \
  --warm-path \
  --path-wait 10 \
  --live \
  --fetch-page \
  --stdout \
  > /tmp/omenbrowser-rs-live/nomadnet-live-fetch.json \
  2> /tmp/omenbrowser-rs-live/nomadnet-live-fetch.summary.txt
```

Key fields:

- `.classification`
- `.live_fetch_readiness_retry`
- `.live_fetch.ok`
- `.live_fetch.stage_hint`
- `.live_fetch.error`
- `.live_fetch.markup_bytes`
- `.live_fetch.body_preview_lines`

Passing condition:

```text
.classification.outcome = "pass"
.classification.stage = "live_fetch"
.live_fetch.ok = true
```

If it passes, compare the rendered page in the TUI against Python OMENbrowser behavior.

## LXMF Receive-Only / Local Announce

This announces local `lxmf.delivery` and waits for LXMF/proof-related events. It does not send a peer-visible message.

```bash
cargo run --features native-network -- \
  --lxmf-interop \
  --backend reticulum \
  --app-root <app-root> \
  --identity <identity-file> \
  --tcp-client <tcp-host:tcp-port> \
  --stdout \
  > /tmp/omenbrowser-rs-live/lxmf-receive-only.json \
  2> /tmp/omenbrowser-rs-live/lxmf-receive-only.summary.txt
```

Longer wait:

```bash
cargo run --features native-network -- \
  --lxmf-wait 30 \
  --backend reticulum \
  --app-root <app-root> \
  --identity <identity-file> \
  --tcp-client <tcp-host:tcp-port> \
  --stdout \
  > /tmp/omenbrowser-rs-live/lxmf-receive-only-wait30.json \
  2> /tmp/omenbrowser-rs-live/lxmf-receive-only-wait30.summary.txt
```

Key fields:

- `.classification`
- `.local`
- `.local_announce`
- `.wait.status`
- `.wait.inbound_messages`
- `.wait.events`

Patch target mapping:

- `classification.outcome = blocked` with local announce failure: native identity loading, local `lxmf.delivery` registration, proof key loading, or announce call.
- `classification.outcome = timeout`: event routing or no remote LXMF traffic during wait.
- `wait.inbound_messages > 0`: inbound decode and runtime event routing are working.

## LXMF Explicit Send-And-Wait

This sends a real labeled LXMF smoke message. Only run it with a peer you control or have permission to message.

```bash
cargo run --features native-network -- \
  --lxmf-interop \
  --send-lxmf-smoke <lxmf-peer-destination-hash> \
  --backend reticulum \
  --app-root <app-root> \
  --identity <identity-file> \
  --tcp-client <tcp-host:tcp-port> \
  --stdout \
  > /tmp/omenbrowser-rs-live/lxmf-send-wait.json \
  2> /tmp/omenbrowser-rs-live/lxmf-send-wait.summary.txt
```

With longer wait:

```bash
cargo run --features native-network -- \
  --lxmf-wait 30 \
  --send-lxmf-smoke <lxmf-peer-destination-hash> \
  --backend reticulum \
  --app-root <app-root> \
  --identity <identity-file> \
  --tcp-client <tcp-host:tcp-port> \
  --stdout \
  > /tmp/omenbrowser-rs-live/lxmf-send-wait30.json \
  2> /tmp/omenbrowser-rs-live/lxmf-send-wait30.summary.txt
```

Key fields:

- `.classification`
- `.readiness_probe.ready_to_send`
- `.readiness_retry`
- `.send.ok`
- `.send.stage_hint`
- `.send.message_id`
- `.wait.proof_match_state`
- `.wait.inbound_reply_match_state`
- `.wait.packet_proofs`
- `.failure_hints`

Passing conditions:

```text
.classification.outcome = "pass"
```

And at least one of:

```text
.wait.proof_match_state = "matched_packet_proof"
.wait.inbound_reply_match_state = "matched_peer_reply"
```

Patch target mapping:

- `readiness_probe.ready_to_send = false` and `readiness_retry.followup_ready_to_send = false`: peer identity/path readiness.
- `send.ok = false` with `send.stage_hint = source_identity`: identity loading/signing.
- `send.ok = false` with `send.stage_hint = peer_identity`: peer announce/known-destinations loading.
- `send.ok = false` with `send.stage_hint = path_discovery`: peer path request/event routing.
- `send.ok = false` with `send.stage_hint = packet_build`: LXMF wire encode/signing.
- `send.ok = false` with `send.stage_hint = send_packet`: `rns-net` packet submission.
- `proof_match_state = no_matching_packet_proof`: RNS proof callback, remote proof behavior, or packet hash correlation.
- `inbound_reply_match_state = no_matching_peer_reply`: remote reply behavior or inbound LXMF decode/routing.

## Combined NomadNet + LXMF Run

Use this when validating both browser and messaging setup against the same Reticulum TCP peer.

```bash
cargo run --features native-network -- \
  --native-smoke <nomadnet-destination-hash>:<nomadnet-path> \
  --backend reticulum \
  --app-root <app-root> \
  --identity <identity-file> \
  --tcp-client <tcp-host:tcp-port> \
  --known-destinations <known-destinations-file> \
  --warm-path \
  --path-wait 10 \
  --live \
  --fetch-page \
  --lxmf-interop \
  --send-lxmf-smoke <lxmf-peer-destination-hash> \
  --stdout \
  > /tmp/omenbrowser-rs-live/combined.json \
  2> /tmp/omenbrowser-rs-live/combined.summary.txt
```

Inspect:

```bash
jq '.classification' /tmp/omenbrowser-rs-live/combined.json
jq '.lxmf_live_interop.classification' /tmp/omenbrowser-rs-live/combined.json
```

## Report Bundle To Preserve

For a failed live attempt, preserve:

```text
*.summary.txt
*.json
Reticulum config used for the run, with secrets removed
OMENbrowser_rs command line, with local paths generalized if needed
Remote peer/node type and whether it is controlled by you
Whether Python OMENbrowser or NomadNet can reach the same destination
```

Do not paste private identity material, raw private keys, or message bodies into issue reports.
