# LXMF SDK 0.9.6 external RPC send-field reproducer

## Scope

This is a deterministic reproducer for the published `lxmf-sdk = 0.9.6` and
`reticulum-rs-rpc = 0.9.6` crates locked by OMENbrowser. It documents an
upstream boundary; it is not a private patch or an alternate wire contract.

OMENbrowser constructs a public `lxmf_sdk::SendRequest` containing a TTL,
idempotency key, correlation identifier, delivery method, stamp policy, and
propagation fallback policy. The request is passed unchanged to
`lxmf_sdk::RpcBackendClient::send`.

## Deterministic reproduction

Run:

```bash
cargo test --locked --no-default-features --features desktop-product \
  external_rpc_096_send_capture --lib -- --nocapture
```

The test starts an isolated loopback-only MessagePack RPC capture endpoint,
sends through the real published `RpcBackendClient`, and decodes the public
`rns_rpc::RpcRequest`. It then returns a deterministic daemon message ID and
verifies that cancellation uses that exact ID.

Observed `sdk_send_v2` properties:

- Preserved: source, destination, title, content, fields, delivery method,
  stamp cost, request-fresh-ticket flag, and propagation fallback choice.
- Dropped: TTL, idempotency key, correlation identifier, and extensions.
- Not representable: an explicit remembered reply ticket.
- Preserved after response: the daemon-returned message ID used for
  `sdk_cancel_message_v2`.

No identities, ticket values, message bodies from a user profile, or endpoint
credentials are used or written to test artifacts.

## Upstream source evidence

In the published `lxmf-sdk-0.9.6` source,
`src/backend/rpc/core_impl_parts/rpcbackendclient_sections/default_idle_tick_delay_ms.rs`,
`RpcBackendClient::send_params` destructures and discards:

- `idempotency_key`;
- `ttl_ms`;
- `correlation_id`;
- `extensions`.

It then serializes only the preserved properties listed above to
`sdk_send_v2`.

In the published `reticulum-rs-rpc-0.9.6` source,
`src/rpc/send_request.rs`, `SendMessageV2Params` contains no TTL,
idempotency-key, correlation-ID, extensions, or explicit-ticket fields.

## Expected behavior

The public external RPC path should preserve the application-facing
`SendRequest` guarantees that the daemon supports, or negotiate and report that
they are unsupported. If explicit reply-ticket delivery is supported, the
public request contract also needs a field with unambiguous encoding and
validation.

## OMENbrowser policy until upstream changes

- Enforce the persisted absolute expiry locally before dispatch and during
  reconciliation, without claiming daemon-side TTL enforcement.
- Do not claim external-daemon idempotency or correlation guarantees.
- Never automatically retry an uncertain external send.
- Reject an explicit remembered reply ticket before connecting rather than
  silently altering stamp policy.
- Continue to use the preserved method, stamp, fresh-ticket, fallback, and
  cancellation-ID fields.

When a later pinned upstream release changes this boundary, the deterministic
capture test should fail and force a deliberate re-evaluation.
