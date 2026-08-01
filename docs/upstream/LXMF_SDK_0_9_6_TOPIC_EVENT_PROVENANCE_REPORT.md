# LXMF SDK 0.9.6 topic event provenance report

Date: 2026-07-31  
Resolved crates: `lxmf-sdk = 0.9.6`, `reticulum-rs-rpc = 0.9.6`

## Scope

This is a deterministic local reproducer and compatibility record. It is not an
upstream issue submission, a fork, or a claim about a later release. OMENbrowser
does not activate NomadNet update topics on this evidence.

## Observed public contract

The public RPC daemon accepts topic create, subscribe, publish, poll-events, and
telemetry-query operations. A publication produces `sdk_topic_published` with:

- `topic_id`;
- optional `correlation_id`;
- `ts_ms`;
- the caller-supplied payload.

The event has no top-level `peer_id`, publisher identity, signing identity,
signature, or authenticated-principal field. Its telemetry recovery record has
a topic tag but likewise no publisher identity. `TopicSubscriptionRequest`
contains a cursor, but the 0.9.6 daemon handler reads and discards it; an
arbitrary cursor is accepted.

Evidence locations in the resolved crate sources:

- `reticulum-rs-rpc/src/rpc/daemon/sdk_topics.rs`,
  `handle_sdk_topic_subscribe_v2` and `handle_sdk_topic_publish_v2`;
- `reticulum-rs-rpc/src/rpc/daemon/events.rs`, `sdk_stream_event_frame`;
- `reticulum-rs-rpc/src/rpc/daemon/sdk_negotiate_poll_parts/.../
  handle_sdk_poll_events_v2.rs`;
- `lxmf-sdk/src/domain_parts/support_types.rs`, `TopicPublishRequest` and
  `TopicSubscriptionRequest`;
- `lxmf-sdk/src/event.rs`, `SdkEvent`.

## OMENbrowser consequence

The update-pointer trust model requires transport-authenticated publisher
identity. A generic SDK `peer_id`, even if one appeared, is not documented as
that proof. Payload fields are attacker-controlled and cannot authenticate
themselves. Topic-list recovery restores topic definitions, while telemetry can
restore published values, but neither supplies authenticated publisher
provenance. Cursor-gap reconciliation therefore cannot safely rebuild the
admission owner's trust evidence.

OMENbrowser's fail-closed classifier reports a well-shaped 0.9.6 publication as
`PublisherAuthenticationAbsent`, retains no payload, and never permits
admission. Oversized or malformed event wrappers are rejected separately.

## Reproducer

```bash
cargo test --locked --no-default-features --features desktop-product \
  locked_096_daemon_reproducer_has_no_publisher_or_subscription_cursor_proof \
  --lib
```

The test uses an in-memory `MessagesStore` and public `RpcDaemon::handle_rpc`
calls. It creates and subscribes to one isolated topic, deliberately supplies an
invalid subscription cursor, publishes one harmless fixture, polls the SDK
event log, and queries telemetry. It asserts that no publisher proof exists and
that the resulting event remains inadmissible. It performs no network I/O and
touches no maintainer state.

## Upstream capability needed for activation

A future usable contract would need all of the following, versioned and tested:

1. authoritative publisher identity bound by the daemon to the accepted
   publication, not copied from caller payload;
2. explicit signature/authentication semantics visible to SDK consumers;
3. a subscription cursor with defined validation and replay behavior;
4. a bounded snapshot/reconciliation API that preserves publisher evidence;
5. mixed-version behavior for daemons and clients lacking the contract.

Until the locked dependency provides equivalent evidence, OMENbrowser will not
fake it, trust payload identity, or activate topic receive/publication as a
NomadNet feature.
