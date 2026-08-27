# v0.10.0-5 Channel attachment implementation

Status: implemented; final proof gate in progress

OMENchat v0.10.0-5 adds the negotiated `omenchat-channel-attachments-v1`
capability using only the public Reticulum 0.10.0 `Transport::channel`,
`TransportChannel::mdu`, `send`, `is_ready_to_send`, `message_state`, and
handler APIs. OMENchat wire protocol remains 1. The public Rust protocol crate
is 0.3.0 because it now exports the additive Channel frame API.

Peers must explicitly accept the capability on their current Link. A peer that
does not accept it receives the exact legacy Resource upload path. Once Channel
dispatch begins there is no Resource fallback, second dispatch, automatic
retry, or replay. Timeout or Link loss is reported as uncertain.

The sender reads files incrementally, derives chunks from the live Channel MDU,
obeys Channel readiness/backpressure, maintains an incremental SHA-256 digest,
and has a bounded overall deadline. The server permits at most 16 active stages
and four per Link, keys each stage by exact Link and resource ID, validates
ownership, length, offset, final digest, room policy, and quota, writes private
create-new temporary files, fsyncs, atomically renames, and couples publication
to the existing database commit. Malformed, cancelled, closed-Link, failed
commit, and shutdown paths remove exact staging state.

Deterministic tests cover frame bounds, Channel negotiation, byte-exact legacy
downgrade, no Resource fallback, offset and digest rejection, exact-Link
cleanup, atomic publication, and second-client retrieval. Isolated direct and
three-node routed client/gateway/server process lanes passed with the 873-byte
fixture and reported `sender_upload_primitive=channel`.

Official Reticulum 0.10.0 tests independently cover Channel out-of-order
buffering, duplicate/window rejection, bounded flow control, retransmission,
and retry exhaustion. OMEN's three-node run did not inject packet loss or
reordering, so that narrower end-to-end impairment claim remains pending. The
Immutable v0.10.0-4 binary lanes pass in both directions: current browser to old
server and old browser to current server selected the legacy Resource path and
completed multi-client upload/download without retry or fallback. This
OMEN-specific path does not repair or promote generic NomadNet/LXMF Resource
parity or either Resource sentinel.
