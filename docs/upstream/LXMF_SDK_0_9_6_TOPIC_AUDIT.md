# LXMF SDK 0.9.6 topic audit

Date: 2026-07-31  
Resolved source: crates.io `lxmf-sdk 0.9.6`

## Public upstream surface

The locked source exposes:

- `LxmfSdkTopics` and `SdkBackend` topic create/get/list, subscribe/unsubscribe,
  and publish methods;
- `TopicId`, `TopicPath`, `TopicRecord`, `TopicPublishRequest`, and
  `TopicSubscriptionRequest`;
- profile capability names `sdk.capability.topics`,
  `sdk.capability.topic_subscriptions`, and `sdk.capability.topic_fanout`;
- RPC method mappings `sdk_topic_create_v2`, `sdk_topic_get_v2`,
  `sdk_topic_list_v2`, `sdk_topic_subscribe_v2`,
  `sdk_topic_unsubscribe_v2`, and `sdk_topic_publish_v2`.

Evidence symbols are in `src/api.rs`, `src/backend.rs`,
`src/domain_parts/support_types.rs`, `src/profiles.rs`, and
`src/backend/rpc/domains_impl_parts/rpcbackendclient_sections/
operation_registry_impl.rs` in the resolved crate.

Profile support is not negotiated backend evidence. `RpcBackendClient` records
effective capabilities only after `negotiate`; constructing the client or
calling `snapshot` does not prove topic support.

## Event and publisher-provenance boundary

The public `SdkEvent` is generic. It carries `event_type`, `peer_id`, arbitrary
JSON payload, sequence/cursor fields, and extensions, but the locked public
surface does not define a typed topic-delivery event or state that `peer_id` is
an authenticated publisher identity for a topic payload. No OMENbrowser code may
upgrade that field into authenticated evidence without a tested contract.

The current OMENbrowser external event worker requests only
`sdk.capability.async_events`. It preserves bounded cursors, deduplicates event
IDs, detects sequence gaps, and forwards unknown events, but it does not request
topic capabilities, subscribe to topics, parse a topic event, authenticate a
publisher, or reconcile a topic snapshot after a gap.

## Current product modes

Managed-native mode uses OMENbrowser's Reticulum/LXMF transport and wire adapter,
not an SDK topic backend. It therefore reports `ProductAdapterMissing`.

The normal external RPC sender probe calls `snapshot`, not `negotiate`, and its
trait contains send, cancel, history, and status operations only. A separate,
explicit diagnostics command can now negotiate the topic/event capability set
once. It does not subscribe, publish, retry, install a worker, or shut down the
externally owned daemon. Even a daemon that advertises the full requested set
remains unready for NomadNet update-pointer receive because topic-event schema,
authenticated publisher provenance, and cursor-gap snapshot reconciliation are
unproven.

## Decision

Do not activate create, subscribe, publish, or event admission. The project-owned
classifier requires bounded negotiated capability evidence plus separately
proven cursor recovery, topic-event contract, and authenticated publisher
events. Dependency/profile presence alone always remains insufficient.

This is a local compatibility finding, not an upstream defect claim. A future
implementation needs a controlled local-daemon capture of negotiation, topic
publication, subscription delivery, restart, event gap, and publisher identity
semantics before the dormant admission owner can be connected.

## Diagnostics-only negotiation evidence

`--lxmf-topic-capability-probe` reads the configured local SDK/RPC endpoint,
applies the existing local-only endpoint validator, and requests topics,
subscriptions, fanout, cursor replay, and asynchronous events under one
10-second total deadline. Its report contains only the redacted endpoint class,
contract version, fixed capability booleans, current proof gaps, elapsed time,
and fixed zero-operation counters. A successful fanout negotiation is labelled
as upstream capability; the OMENbrowser publish adapter remains inactive.

The implementation uses the SDK's cancellable Tokio RPC transport. It drops the
client after negotiation and deliberately does not call SDK shutdown, because
the daemon is externally owned. Missing capabilities fail the requested
negotiation; they are not emulated. There is no automatic retry.

## Deterministic event-contract result

The exact public 0.9.6 daemon was exercised in memory through topic create,
subscribe, publish, event poll, and telemetry query. The publication event shape
is stable enough to identify a topic/payload/correlation tuple, but it contains
no authenticated publisher identity. Telemetry recovery also lacks publisher
identity, and the supplied subscription cursor is ignored. OMENbrowser now has
a fail-closed bounded classifier for this event; it always denies admission
because payload or generic event fields cannot manufacture authentication.

This closes the source-inspection question but confirms the activation blocker.
See `LXMF_SDK_0_9_6_TOPIC_EVENT_PROVENANCE_REPORT.md` for the reproducer and the
future contract requirements.
