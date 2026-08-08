# OMENbrowser_rs and omenchatd v0.9.8-3 release notes

Reticulum/LXMF crate train: exact official registry `0.9.8`

Status: final

## Independent NomadNet response selection

- The quiet `omenchatd` NomadNet portal now selects its response primitive from
  the size of the complete packed `[request_id, response_body]` envelope. The
  request ingress primitive no longer controls the response primitive.
- A direct request can therefore receive a large response Resource, and a
  request Resource can receive a small direct `PacketContext::Response`.
- Direct responses use the public `Link::response_packet()` constructor.
  Responses larger than the public conservative `PACKET_MDU` boundary use
  `Transport::send_response_resource()`.
- Each operation selects once and dispatches at most once. Constructor, Link,
  or Resource-dispatch failure does not trigger primitive fallback, replay, or
  a second response.

## Bounds and evidence

- Portal files are read through the private bounded reader. The complete packed
  response envelope is limited to 4 MiB and is rejected before dispatch when it
  exceeds that ceiling.
- Deterministic Rust and pinned/current Python matrices cover all four request /
  response primitive combinations, exact request correlation, and exact page
  bytes. The normal process smoke also covers a direct small request returning
  an incompressible 32 KiB response Resource.
- Dynamic negotiated-payload-MDU response selection remains deferred until a
  suitable public upstream boundary is available. The conservative public
  `PACKET_MDU` selector is intentionally retained.
- A terminal NomadNet response timeout now retires only the route selected for
  that failed Link and requests paths on the other attached interfaces. The
  failed request is never replayed; recovery applies only to a later explicit
  user attempt. A three-gateway isolated-root smoke reproduced a timeout over
  one partially healthy route and loaded the exact 33,277-byte page on the
  next explicit attempt without disabling any gateway.
- An outbound OMENchat Resource failure applies the same route-scoped recovery:
  the affected Link is closed, its ownership is removed, and alternate path
  discovery is prepared without resending the attachment. Isolated two-client
  local-gateway smokes transferred and fetched exact 873-byte and 54,427-byte
  attachments.

## Compatibility boundaries

- Package versions advance to `0.9.8-3`; all active Reticulum/LXMF dependencies
  remain exact official crates.io `0.9.8`, without a Git source, fork,
  vendoring, or patch override.
- OMENchat protocol version `1`, frame layouts, operation numbers, capability
  identifiers, and mixed-version behavior are unchanged.
- No database, configuration, cache, identity, destination, message, ticket,
  upload-content, or Reticulum-storage migration is introduced.
- No request replay, backend switch, post-dispatch primitive fallback, or
  second response dispatch is introduced.
- The independent maximum-UDP Resource sentinel remains visible and unchanged.
  Stock upstream TCP interfaces still do not enforce IFAC; the project-local
  `omen-ifac-tcp` adapter and enforcing-gateway topology remain supported.

## Rollback

This revision changes only quiet-portal response construction and selection.
A `v0.9.8-2` binary can reopen the same state without conversion. Roll back the
binaries only; do not delete identities, messages, configuration, databases,
uploads, or Reticulum storage.
