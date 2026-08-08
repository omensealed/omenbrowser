# OMENbrowser_rs and omenchatd v0.9.8-2 release notes

Reticulum/LXMF crate train: exact official registry `0.9.8`

Status: final

## Native NomadNet stale-path recovery

- A native NomadNet Link-setup timeout now closes and removes the failed Link,
  expires the cached route for that destination, and emits one bounded path
  discovery request before any executable NomadNet request is dispatched.
- The existing single bounded automatic retry remains limited to Link setup,
  where the remote page operation has not run. Request-send and response-wait
  failures now present an explicit manual Retry because their remote outcome
  can be uncertain.
- Identification behavior is unchanged. Live comparison showed that disabling
  identify-on-connect did not affect the failure or the successful fresh-route
  fetch.

## Qualification evidence

- A fresh isolated Rust client selected a working two-hop path, activated the
  Link, and fetched the exact 33,277-byte page over a compressed response
  Resource. The same node also answered the reference Python client.
- The stale-path recovery regression proves that recovery emits path discovery,
  replaces the failed outbound Link, and does not dispatch a NomadNet Request or
  request Resource.
- Native request coverage, retry-state regressions, formatting, and strict
  desktop Clippy pass locally. Hosted interoperability and platform packaging
  remain release workflow gates.

## Compatibility boundaries

- Package versions advance to `0.9.8-2`; the exact official Reticulum/LXMF
  `0.9.8` train remains unchanged with no Git dependency, fork, vendoring, or
  patch override.
- OMENchat protocol version `1`, frame layouts, operation numbers, and
  capability identifiers remain unchanged.
- No database, configuration, cache, identity, destination, message, ticket,
  upload-content, or Reticulum-storage migration is introduced.
- No automatic retry, replay, primitive fallback, backend switch, or second
  executable request dispatch is added after a remote outcome can be uncertain.
- The independent maximum-UDP Resource sentinel remains visible and unchanged.

## Rollback

This revision changes only native NomadNet runtime recovery and application
retry presentation. A `v0.9.8-1` binary can reopen the same state without
conversion. Roll back the binaries only; do not delete identities, messages,
configuration, databases, or Reticulum storage.
