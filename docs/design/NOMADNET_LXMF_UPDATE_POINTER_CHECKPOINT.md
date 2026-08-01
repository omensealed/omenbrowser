# NomadNet LXMF update-pointer checkpoint

Date: 2026-07-31  
Locked upstream train: `lxmf-sdk = 0.9.6`, `lxmf-wire = 0.9.6`

## Scope and current decision

Phase 5 begins with a pure dormant envelope. No SDK topic, runtime event,
subscription, publication, persistence, browser fetch, cache write, UI notice,
timer, or background task consumes it.

The locked SDK publicly exposes topic create/list/subscribe/unsubscribe/publish
types and the `sdk.capability.topics`, `sdk.capability.topic_subscriptions`, and
`sdk.capability.topic_fanout` capability names. OMENbrowser's current managed
native and external SDK adapters do not yet negotiate or bridge those operations.
Their existence in the dependency is therefore not treated as product support.

## Envelope

`browser::update_pointer::NomadNetUpdatePointer` contains only:

- exact protocol and version;
- canonical lowercase 32-character destination hash;
- canonical page path without query, fragment, backslash, repeated separator,
  or dot traversal segments;
- bounded revision or content-hash identifier;
- bounded control-free title;
- publication and expiry timestamps.

The serialized JSON is capped at 2 KiB. Paths are capped at 512 bytes, titles
at 256 bytes, and revision identifiers at 128 bytes. Publication may be at most
five minutes ahead of the local clock. Expiry must follow publication and may
span at most 30 days; expired pointers receive only the same five-minute skew.
Unknown fields, malformed input, unsupported versions, and noncanonical values
are rejected atomically.

The stable deduplication tuple is destination, path, and revision/content hash.
Authenticated publisher identity is deliberately not claimed by this envelope;
it must arrive as authoritative transport/SDK evidence and be bound by a later
admission owner.

## Required next gates

Before runtime activation:

1. Prove topic capabilities separately for each backend; absence is supported,
   not silently emulated.
2. Define a frontend-neutral owner bounded by followed topics, retained pointer
   items and bytes, per-publisher rate, age, and incremental pruning.
3. Bind each pointer to authenticated publisher evidence or mark it unverified.
4. Add explicit follow/unfollow and preview actions without automatic fetch.
5. Add mixed-version and event-gap/snapshot recovery evidence.

No page body belongs in the pointer. No pointer may trigger background crawling,
prefetch, trust, or rendering of unvalidated title/path content.

## Implementation progress

The frontend-neutral admission owner is now implemented but has no production
caller. It owns at most 64 followed targets and 64 KiB of their strings, 128
notices and 64 KiB of their encoded/publisher bytes, and 512 fixed-size rate
records. Each publisher may admit at most eight pointers per ten-minute window.
Destination/path/revision duplicates are rejected. Notice and rate expiry scans
remove at most eight records total per call.

Each retained notice preserves authenticated versus unverified publisher
evidence as supplied by a future transport boundary. The payload does not
upgrade that evidence. Unfollow removes matching bounded notices, clear removes
all ephemeral state, and the notice model always reports automatic fetch as
disabled.

No topic capability, SDK call, runtime event, disk owner, browser action, or UI
consumer has been added. The next gate remains backend-specific topic capability
and authenticated-event evidence, followed by a separately reviewed runtime
adapter boundary.

The locked-SDK audit now has a compiler-checked project classifier. Managed
native is explicitly `ProductAdapterMissing`. External RPC capability snapshots
are item/name/byte bounded and cannot become receive-ready unless topics,
subscriptions, cursor replay, asynchronous events, project-proven gap recovery,
a proven topic-event contract, and authenticated publisher events are all true.
Fanout is tracked separately for publishing.

Current code supplies none of the three project evidence booleans and the
external event worker requests only asynchronous events, so activation remains
blocked. See `docs/upstream/LXMF_SDK_0_9_6_TOPIC_AUDIT.md`.

An explicit diagnostics-only external RPC negotiation probe is now available.
It can establish what a configured local daemon negotiated, but it performs no
topic calls and cannot change any of the three project evidence booleans. A
successful probe therefore still reports the receive adapter inactive.

The locked 0.9.6 public daemon event contract has now been reproduced
deterministically. Its publication event and telemetry recovery record omit
authenticated publisher identity, while the subscription cursor is accepted
but ignored. The frontend-neutral admission owner therefore remains caller
inert. No local payload field will be treated as publisher proof.
