# 14 — Native LXMF-rs Integration

This document guides implementation of LXMF in OMENbrowser_rs using native Rust crates behind the existing messaging/runtime boundary.

## Current external reality

The `lxmf` Rust umbrella crate currently exposes wire and SDK layers. Public docs show re-exports for `lxmf_core` as `wire` and `lxmf_sdk` as `sdk`, plus high-level concepts such as `Client`, `ClientHandle`, `Message`, `Payload`, `SendRequest`, `StartRequest`, `RuntimeSnapshot`, `MessageMethod`, `MessageState`, `RuntimeState`, `TransportMethod`, and SDK traits.

Maintainers must inspect the exact version currently selected by Cargo before implementing.

## Architectural rule

LXMF implementation belongs behind these boundaries:

```text
TUI -> MessagingService -> NetworkRuntime trait -> Native runtime/LXMF adapter
```

The UI must not use `lxmf::*` types directly.

The message store must remain an OMENbrowser-owned durable JSON store even if the native LXMF SDK has its own internal runtime state.

## Cargo feature strategy

Use feature gating:

```toml
[features]
default = []
live-reticulum = [...]
live-lxmf = ["live-reticulum", "dep:lxmf"]
```

If native LXMF can compile independently from live Reticulum but cannot run independently, keep the feature dependency anyway. OMENbrowser_rs wants a real integrated runtime, not an isolated message codec demo.

## Module layout

Create:

```text
src/runtime/native_lxmf/
  mod.rs
  config.rs
  codec.rs
  client.rs
  delivery.rs
  events.rs
  propagation.rs
  store_sync.rs
```

Responsibilities:

- `config.rs`: build LXMF client/start config from app settings/identity.
- `codec.rs`: map OMENbrowser message models to/from native LXMF messages.
- `client.rs`: owns native LXMF client lifecycle.
- `delivery.rs`: direct/propagated send implementation.
- `events.rs`: incoming message and delivery status conversion.
- `propagation.rs`: propagation node selection, sync, tickets.
- `store_sync.rs`: reconcile pending local messages with native runtime state.

## Model mapping

Map app models to native LXMF carefully.

### Outbound

| OMENbrowser model | Native LXMF concept |
|---|---|
| `MessageEnvelope.destination` | recipient/destination hash/address |
| `MessageEnvelope.source` | active identity/source LXMF address |
| `title` | LXMF title/subject field if supported |
| `body` | content/payload text |
| `attachments` | LXMF attachment/payload parts if supported |
| `DeliveryMode::Direct` | direct delivery / opportunistic method |
| `DeliveryMode::Propagated` | propagation method |
| `include_ticket` | propagation ticket flag if crate supports it |

### Inbound

| Native LXMF | OMENbrowser model |
|---|---|
| message id/hash | `MessageSummary.id` |
| source hash | `peer_hash` / source address |
| title | `title` |
| body/payload | `body` |
| timestamp | `received_at` or native timestamp fallback |
| delivery state | `DeliveryStatus` |
| method | direct/propagated transport enum |
| attachments | `AttachmentSummary` entries |

Never drop unknown LXMF metadata if there is a reasonable `extra` map available. If no map exists yet, add one to the internal model, not directly to the UI.

## Client lifecycle

The native runtime should own one LXMF client per active identity/runtime.

Lifecycle:

1. Load/create Reticulum identity.
2. Build LXMF client config.
3. Start client.
4. Subscribe to receive/status events.
5. Expose send/sync/status calls through `NetworkRuntime`.
6. Stop client cleanly on app shutdown or identity switch.

Identity switching must restart or reconfigure LXMF safely. Do not let old identity tasks keep sending events into the new identity session.

Use generation/session IDs for runtime identity sessions if needed.

## Send behavior

`MessagingService::send_message()` should remain the service entry point.

Native send sequence:

1. Validate non-empty draft at service/UI layer.
2. Build `MessageEnvelope`.
3. Call runtime send.
4. Runtime maps envelope to native `SendRequest` or equivalent.
5. Native adapter returns local outbound message summary immediately if possible.
6. Delivery state updates arrive asynchronously through runtime events.
7. Message store records pending/sent/failed updates.

Do not block the UI waiting for final remote delivery confirmation.

## Receive behavior

Incoming messages should arrive through runtime events.

Expected event path:

```text
native LXMF receive -> adapter converts to MessageEnvelope/MessageSummary -> RuntimeEvent::MessageReceived -> AppEvent -> MessagingService/store -> open conversation tab update
```

Manual `Ctrl-G` sync can remain, but live runtime should also push events automatically.

## Propagation behavior

Support these operations where native crate permits:

- set outbound propagation node
- get outbound propagation node
- sync from propagation node
- send propagated message
- include ticket if available
- show propagation status in diagnostics

If a feature is not yet supported by native crate API, return structured unsupported errors and document the gap.

Do not fake propagated delivery success in live mode.

## Pending reconciliation

The app already tracks pending send generation and message store state.

Native adapter should expose:

- local outbound id
- native message id if different
- state transitions
- failure reason
- retry eligibility

Reconciliation rules:

1. If a pending message has a matching native id, update that message.
2. If native runtime reports a message already delivered but store says pending, update to delivered/sent.
3. If native runtime reports failure, mark failed with reason.
4. Do not create duplicates when syncing.
5. Preserve user draft text if send fails before native acceptance.

## Attachments

Initial support can be conservative.

Minimum acceptable implementation:

- receive attachment metadata without crashing;
- store attachment names/sizes/types where available;
- reject oversized outbound attachments with a clear error;
- do not load entire large attachments into UI memory.

Full support:

- attachment picker/control path;
- streaming or file-backed outbound payloads;
- received attachment save/export;
- diagnostics and tests.

## Tests

### Codec tests

- outbound text maps correctly;
- inbound text maps correctly;
- direct vs propagated method maps correctly;
- unknown metadata preserved;
- invalid destination rejected;
- attachment metadata converted.

### Service tests

- send direct schedules native request;
- send propagated schedules native request;
- native sent event updates pending store entry;
- native failed event updates pending store entry;
- inbound event creates conversation tab;
- inbound duplicate does not duplicate visible thread.

### Feature tests

Run:

```bash
cargo check --features live-lxmf
cargo test --features live-lxmf
```

## Done when

- `live-lxmf` builds.
- Native send works through `MessagingService`.
- Native receive flows into message store and open conversations.
- Propagation node/status paths exist.
- Delivery state changes update store and UI.
- Mock mode remains the default and all tests still pass.
