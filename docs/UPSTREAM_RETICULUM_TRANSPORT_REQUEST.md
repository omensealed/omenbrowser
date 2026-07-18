# Upstream Request: Direct Link Data Send API

This note is written so it can be pasted into an upstream
`reticulum-rs-transport` issue or used as a local patch checklist.

## Summary

OMENbrowser_rs is migrating its live Reticulum/NomadNet path to the
`reticulum-rs` / `reticulum-rs-transport` 0.6 stack. The remaining blocker for
small packet requests is a missing public helper for sending arbitrary encrypted
link data directly on the active link interface.

OMENbrowser needs this for Python/NomadNet-compatible:

```text
Link.request(path, data=...)
```

Small requests are encoded as encrypted link data with
`PacketContext::Request`, then responses are received as
`PacketContext::Response`. Oversized requests are sent as request resources;
`reticulum-rs-transport 0.6.0` already exposes public
`Transport::send_request_resource(...)` for that path.

## What Already Exists

`reticulum-rs-transport 0.6.0` already exposes most of the needed pieces:

- outbound links can be established;
- `Link::packet_with_context(...)` can build encrypted link data;
- `PacketContext::Request` and `PacketContext::Response` exist;
- inbound request/response link data appears through `received_data_events()`;
- active links retain `ingress_iface()`.
- request/response resource helpers are public, including
  `Transport::send_request_resource(...)`.
- public direct link-data/channel helpers exist, but they use
  `PacketContext::None` or `PacketContext::Channel` respectively, not
  `PacketContext::Request`.

Internal transport code already uses the correct direct-interface pattern:

- `transport/links_parts/transportchannel.rs`:
  channel send looks up `link.ingress_iface()` and calls
  `TxMessageType::Direct(iface)`.
- `transport/links_parts/transport_sections/reset_out_link.rs`:
  `send_channel_message(...)` does the same thing.
- `transport/resource_wire.rs`:
  resource responses use the link ingress interface when available.

## Why Public `send_packet` Is Not Enough

The current public `Transport::send_packet(...)` and trace/outcome variants
route by packet destination. For encrypted link data, the packet destination is
the link id, not the remote destination hash.

In live OMENbrowser clean-stack testing:

- destination path was known;
- link establishment succeeded;
- the active link had an `ingress_iface`;
- the link id did not have a path-table route;
- public generic dispatch fell back to broadcast;
- no response events arrived.

OMENbrowser now detects this and fails fast instead of waiting for timeout, but
it cannot complete normal small NomadNet page fetch without a public direct
link-data send. Large form/request submissions can now use the public
request-resource path and should be live-tested separately.

Using `send_to_out_links(...)` or the channel helpers is not compatible with
NomadNet page requests: Python Reticulum dispatches registered request handlers
only for `PacketContext::Request`.

## Minimal API Shape

The smallest useful helper would be:

```rust
impl Transport {
    pub async fn send_link_data_with_context(
        &self,
        link_id: &AddressHash,
        payload: &[u8],
        context: PacketContext,
    ) -> Result<SendPacketOutcome, RnsError>;
}
```

Expected behavior:

1. Find an active inbound or outbound link by `link_id`.
2. Error if the link is not active or has no bound `ingress_iface`.
3. Build an encrypted link-data packet for `payload` with the requested
   `PacketContext`.
4. Send with `TxMessageType::Direct(iface)`, where `iface` is the link's
   `ingress_iface`.
5. Return a normal send outcome or trace so callers can diagnose failure.

An alternate narrower helper would also work:

```rust
pub async fn send_request_to_link(
    &self,
    link_id: &AddressHash,
    payload: &[u8],
) -> Result<SendPacketOutcome, RnsError>;
```

That narrower helper can hard-code `PacketContext::Request`.

## OMENbrowser Integration Point

OMENbrowser will keep this behind its existing runtime boundary:

- `NativeLinkRequestAdapter`
- `ReticulumPageTransportClient`
- `NetworkRuntime::fetch_page`

Once the helper exists, `Reticulum06LinkRequestAdapter` can replace its current
small-packet fail-fast guard with this direct call and continue waiting on
`received_data_events()` for `PacketContext::Response`. Its existing
request-resource branch should remain for oversized packed requests.

## Local Acceptance Test

The OMENbrowser side is ready to verify with:

```bash
cargo build --features chat-client-rns-clean
./target/debug/omenbrowser_rs --version
./target/debug/omenbrowser_rs --desktop
```

Expected version flags for this migration test:

```text
chat-client-rns-clean:on
native-rns-net:off
```

Page fetch should then be tested against a real NomadNet page and a form
submission. OMEN's current adapter now composes the missing high-level operation
from published link packet/context and bound `send_direct` primitives. Current
Python tests pass direct requests, oversized request Resources, direct
responses, large response Resources, response timeout, and cancellation after
confirmed dispatch without replay. Two sequential Python handler requests also
reuse one active link. The upstream request remains useful because a high-level
helper would replace this project-local protocol composition and reduce
maintenance risk.
