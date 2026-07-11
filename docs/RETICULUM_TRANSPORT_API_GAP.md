# Reticulum Transport API Gap

OMENbrowser_rs can build against the clean `reticulum-rs` / `lxmf` 0.6 stack,
and live NomadNet page fetch is now verified through the request-resource
compatibility path.

This project intentionally uses the published crates as-is. Do not add a local
`[patch.crates-io]` override for `reticulum-rs-transport`; OMENbrowser_rs is not
the upstream transport maintainer and should not depend on private forked APIs
for normal builds.

The OMENchat client and `omenchatd` `live-reticulum` server have a clean-stack
transport path now: links are opened against `omenchat.node`, normal OMENchat
frames are sent as context-zero encrypted link data with
`rns_transport::delivery::send_on_link`, and OMENchat media/upload resources use
the public transport resource API with `omenchat-resource:` metadata. The live
server accepts context-zero data only when the payload decodes as an OMENchat
frame. This is intentionally separate from the legacy `0x4f` OMENchat packet
context because `reticulum-rs-transport` 0.6.0 does not expose an arbitrary
custom packet context variant.

The remaining missing primitive is a public `reticulum-rs-transport` helper for
sending efficient small encrypted `PacketContext::Request` link data directly on
the active link's bound interface. OMENbrowser currently needs:

- `PacketContext::Request` for efficient small NomadNet page requests.

The 0.6.0 crate does expose public
request/response resource helpers, and OMENbrowser_rs now wires the clean
adapter to use `Transport::send_request_resource()` for all prepared NomadNet
request frames. Python Reticulum normally chooses request-resource only for
oversized requests, but the receiver accepts request resources by advertisement
flags and request id, not by size.

See `docs/UPSTREAM_RETICULUM_TRANSPORT_REQUEST.md` for the upstream-ready issue
text and local acceptance checklist.

## What Works

- `reticulum-rs-transport` can start configured TCP client interfaces.
- IFAC profile values can be passed through `InterfaceSharedConfig`.
- OMENbrowser_rs now has a project-local IFAC TCP client interface that
  implements `reticulum-rs-transport`'s public `Interface` trait and applies
  Python-compatible IFAC wire masking before HDLC framing. This is not a
  vendored crate patch.
- destination identities can be recalled after announces/path discovery.
- outbound links can be created and activated.
- inbound link data preserves context and request ids through
  `received_data_events()`.
- public transport request/response resource helpers send on
  `link.ingress_iface()`.
- public link-data helpers can send on established output links, but only with
  `PacketContext::None`.
- public channel helpers send on `link.ingress_iface()`, but they frame payloads
  as `PacketContext::Channel`.
- `PacketContext::LinkIdentify` exists, inbound link handling preserves it, and
  OMENbrowser can send it by building encrypted link data, marking the packet
  `LinkIdentify`, and dispatching with `Transport::send_direct()` on
  `link.ingress_iface()`.

## What Still Needs Improvement

### IFAC/private gateway support

Published `reticulum-rs-transport` 0.6.0 exposes IFAC-related config and packet
types, but its stock TCP client path serializes packets directly to HDLC without
applying the Python Reticulum IFAC wire transform. That is why private-gateway
OMENchat tests previously failed before path/link establishment even though
interface profiles showed `ifac=configured`.

OMENbrowser_rs now handles configured IFAC TCP client profiles with a small
project-local interface implementation:

- `src/runtime/native/ifac_tcp.rs` implements the public
  `rns_transport::iface::Interface` trait.
- It uses the published crate's public `Packet`, `Hdlc`, buffer, hash, identity,
  and interface-channel APIs.
- It derives the IFAC identity/key from configured `network_name` and
  `passphrase`, signs packet bytes, inserts the IFAC bytes, masks/unmasks the
  correct wire ranges, verifies inbound IFAC signatures, and then hands normal
  `Packet` values back to `reticulum-rs-transport`.
- Non-IFAC TCP client profiles continue to use the stock upstream TCP client.

This keeps OMENbrowser on published crates as requested, while avoiding a
private `[patch.crates-io]` transport fork. The remaining upstream improvement
would be for `reticulum-rs-transport`'s stock TCP interfaces to apply the same
wire transform internally when IFAC config is present.

Python NomadNet page fetch uses `Link.request(path, data=...)`. Python sends
small requests as a `PacketContext::Request` link-data packet, and sends
oversized packed requests as a request resource with the truncated request hash
as the resource request id.

OMENbrowser_rs can build a Python-compatible request frame and send it through
`Transport::send_request_resource()`, then wait for a response resource whose
request id matches the frame hash. This is expected to be compatible with
Python Reticulum's request-resource receiver, is live-verified for basic page
loads and identify-on-connect page loads, and avoids the unsafe generic packet
dispatch path.

For encrypted link data, the destination is the link id. That link id is not a
normal destination path-table entry. In broadcast-enabled transport configs,
generic dispatch therefore broadcasts the active link packet instead of sending
it directly on the interface that proved the link.

The existing public `send_to_out_links()` helper is not a compatible shortcut:
it sends directly to active outbound links, but it builds packets with
`PacketContext::None`. The public channel helpers also send on the bound
interface, but use `PacketContext::Channel`. Python Reticulum's request handler
only dispatches registered NomadNet page handlers for `PacketContext::Request`,
so using `None` or `Channel` would silently bypass the server's request path.
Identify-on-connect has a narrower workaround: OMENbrowser builds the encrypted
link-data packet with `Link::data_packet()`, changes the public packet context
to `PacketContext::LinkIdentify`, and sends it directly through
`Transport::send_direct()` on the active link's ingress interface.

Earlier clean-stack logs showed why generic packet dispatch must not be used
for small requests:

- destination path known;
- link established;
- link has an ingress interface;
- link id has no path-table route;
- generic dispatch used broadcast;
- no response events arrived.

The adapter now uses request-resource instead of generic packet dispatch. The
remaining practical improvement is an upstream direct request-context helper so
small requests do not need the resource path.

## Desired Upstream Shape

A minimal public helper could look like this:

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

The implementation should:

- find the active inbound or outbound link by `link_id`;
- build `link.packet_with_context(payload, context)` or equivalent;
- send the packet directly on `link.ingress_iface()`;
- return an error if the link is not active or has no bound interface.

That generic helper would cover both `Request` and `LinkIdentify`, although
OMENbrowser no longer needs it for identify-on-connect. A narrower identify
helper would still be useful upstream:

```rust
pub async fn identify_link(
    &self,
    link_id: &AddressHash,
    identity: &PrivateIdentity,
) -> Result<SendPacketOutcome, RnsError>;
```

It should build Python-compatible proof data:

```text
signed_data = link_id || identity_public_key
proof_data  = identity_public_key || sign(signed_data)
context     = PacketContext::LinkIdentify
```

An alternate narrower helper specific to requests would also work:

```rust
pub async fn send_request_to_link(
    &self,
    link_id: &AddressHash,
    payload: &[u8],
) -> Result<SendPacketOutcome, RnsError>;
```

## OMENbrowser Boundary

The UI should not call this helper directly. It should remain behind:

- `NativeLinkRequestAdapter`;
- `ReticulumPageTransportClient`;
- `NetworkRuntime::fetch_page`.

When the small-packet helper exists upstream, `Reticulum06LinkRequestAdapter`
can route small frames through the direct helper and continue to wait on
`received_data_events()` for `PacketContext::Response` data. The current
request-resource path should remain for large form submissions because it
matches Python Reticulum's `Link.request()` behavior and provides a safe
compatibility fallback.

For identify-on-connect, `NativePageFetchContext` carries both the policy flag
and the loaded local identity. Clean-stack page loads send the Python-compatible
LinkIdentify proof after the page link is active. If the identity cannot be
loaded or the link lacks a bound ingress interface, page fetch still proceeds
and the skipped identify is logged.

## IFAC-Gated Gateway Status

The clean OMENchat protocol has now been smoke-tested with one `omenchatd`
server and two isolated OMENbrowser clients all attached as TCP clients to the
same IFAC-gated private gateway. With the project-local IFAC TCP interface in
place, both clients opened the Reticulum link, joined the room, and observed a
message echo. The server saw the expected `SessionOpen`, `JoinRoom`, and
`RoomMessage` frames.

The smoke helper supports this topology:

```text
scripts/release-omenchat-smoke.sh \
  --server-tcp-client <gateway-host:port> \
  --network-name <ifac-network> \
  --multi-client
```

Pass the IFAC passphrase through `OMENCHAT_PASSPHRASE` or `--passphrase`; avoid
committing real gateway secrets. The same helper still supports non-IFAC local
TCP server smoke tests.

The desired upstream behavior remains:

- derive the IFAC access code from configured `network_name` and `passphrase`;
- mark authenticated packets with `IfacFlag::Authenticated` or the equivalent
  upstream packet representation;
- serialize the IFAC bytes in the correct Reticulum wire position;
- verify/drop inbound IFAC packets before normal packet dispatch;
- keep non-IFAC interfaces behavior unchanged.

## Clean LXMF Propagation Status

The published Reticulum 0.6 transport APIs are now sufficient for OMENbrowser's
clean LXMF propagation send/sync path:

- outbound propagation uses active Reticulum links plus request packets to the
  selected propagation node;
- propagation envelopes are generated by the `lxmf` 0.6 wire API;
- propagation stamps are generated in OMENbrowser with the LXMF-compatible
  HKDF/SHA256 workblock algorithm;
- sync validates and strips stamps before decrypting with the recipient
  identity-hash salt expected by `lxmf` 0.6.

No `rns-net` compatibility crate is required for the clean path. The remaining
known gap is sender-side direct proof/reply correlation: direct messages can be
delivered to an online peer, but short CLI smoke runs may still time out waiting
for proof evidence even when the receiver observed the message.

Clean LXMF ticket support is now implemented in the clean codec/app path:
outbound include-ticket sends emit the LXMF ticket field, inbound decode
extracts reply-ticket metadata, and direct replies can use a valid stored reply
ticket to produce the LXMF ticket stamp. This does not require `rns-net`; it uses
the `lxmf` 0.6 wire payload field/stamp surfaces.
