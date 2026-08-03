# LXMF SDK 0.9.7 external RPC send-field reproducer

## Scope

This is a deterministic reproducer for the official crates.io
`lxmf-sdk = 0.9.7` and `reticulum-rs-rpc = 0.9.7` crates locked by
OMENbrowser. It documents an upstream boundary; it is not a private patch or
alternate wire contract.

OMENbrowser constructs a public `lxmf_sdk::SendRequest` with a TTL,
idempotency key, correlation identifier, nonempty extension, delivery method,
stamp policy, and propagation fallback policy, then passes it unchanged to the
shipped `lxmf_sdk::RpcBackendClient`.

## Deterministic reproduction

Run:

```bash
cargo test --locked --no-default-features --features desktop-product \
  upstream_rpc_097_ --lib -- --nocapture
```

The test uses an isolated loopback MessagePack RPC capture endpoint and the
real published client. It returns a deterministic daemon message identity and
also proves cancellation uses that exact identity.

Observed `sdk_send_v2` properties:

- preserved: source, destination, title, content, fields, delivery method,
  stamp cost, fresh-ticket request, and propagation fallback choice;
- dropped: TTL, idempotency key, correlation identifier, and extensions;
- not representable: an explicit remembered reply ticket;
- preserved after response: the daemon message ID used for
  `sdk_cancel_message_v2`.

The fixture contains no user identity, ticket, message, path, or credential.

## Upstream source evidence

In official `lxmf-sdk-0.9.7`,
`src/backend/rpc/core_impl_parts/rpcbackendclient_sections/default_idle_tick_delay_ms.rs`
destructures `SendRequest` and explicitly ignores `idempotency_key`, `ttl_ms`,
`correlation_id`, and `extensions` in `RpcBackendClient::send_params`.

The separate ZMQ pipeline serializer carries those four fields, but that is
not the `RpcBackendClient` path shipped by OMENbrowser and does not establish
external-daemon guarantees for this product mode.

The production fail-closed boundary is covered separately with:

```bash
cargo test --locked --no-default-features --features desktop-product \
  external_rpc_097_ --lib -- --nocapture
```

## OMEN policy

- Reject plans requiring TTL, idempotency, correlation, or extensions before
  connecting or dispatching.
- Never automatically retry an uncertain external send.
- Reject an explicit remembered reply ticket before opening the connection.
- Keep endpoint availability distinct from full send equivalence. Managed
  integrated sending remains the supported complete path.

A later exact upstream release that changes this serialization will make the
capture assertion fail and require a deliberate compatibility decision.
