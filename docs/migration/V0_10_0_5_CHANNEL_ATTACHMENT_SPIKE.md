# v0.10.0-5 Channel/Buffer attachment spike

Status: deferred

The exact published Reticulum 0.10.0 Channel/Buffer APIs were reviewed as a
possible OMENchat-specific attachment transport. No production code, feature
flag, dependency, protocol frame, or persistent state was added.

Shipping requires explicit per-Link negotiation, legacy byte equivalence,
Channel-MDU-derived bounded chunks, backpressure, digest and atomic-file
handling, bounded cancellation/timeout/duplicate cleanup, true three-node
loss/reordering/reconnect evidence, quotas, malformed-input tests, and both
directions of v0.10.0-4 compatibility. This checkout has no complete named
three-node and adjacent-artifact evidence set, so implementation would violate
the proof gate. Generic Resource parity is not inferred from Channel support.

Next acceptance requires a separate bounded protocol spike using only official
published APIs, with no whole-file buffering, automatic uncertain replay,
schema migration, upstream patch, or wire break.
