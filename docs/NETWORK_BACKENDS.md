# Network Backends

OMENbrowser keeps Reticulum and LXMF behind the project-owned `NetworkRuntime`
boundary. Application and UI code must not infer backend capabilities from the
crate version or a saved mode name.

## Managed integrated mode

`reticulum_instance_mode = "managed"` is the supported v0.9.6-4 product mode.
OMENbrowser owns the runtime lifecycle, identity attachment, configured
interfaces, bounded event workers, and orderly shutdown. The diagnostics
lifecycle and capability snapshots report what that active adapter actually
supports.

### Interface configuration scope

The Interfaces panels edit profile storage and atomically rewrite the managed
Reticulum configuration. They do not mutate an already-running transport.
Create, edit, enable, disable, connectable-toggle, and delete operations take
effect at the next runtime start/restart. Both desktop and TUI surfaces state
this scope, and a detected runtime/config mismatch produces an explicit restart
warning.

The native adapter reports `interface_mutation` as `unknown`; OMENbrowser does
not infer support from the 0.9 crate version. Live add/remove/reload remains
deferred until a typed public API is negotiated and cancellation, ownership,
failure recovery, and live interface tests exist.

## External/shared mode

`reticulum_instance_mode = "external"` remains readable for configuration and
measurement compatibility, but full external/shared Reticulum operation is
deferred. Selecting it does not prove a daemon exists or that OMENbrowser is
connected to one. Both the application startup gate and the native adapter
refuse to start integrated interfaces in this mode, preventing a silent second
runtime from conflicting with an operator-managed instance.

Diagnostics report the configured state as `external_deferred` separately from
the negotiated `shared_instance` capability. Until a typed backend successfully
negotiates live ownership, that capability and the shared-instance network
status remain `unknown` or unavailable.

The optional local LXMF SDK/RPC endpoint provides only the explicitly
negotiated SDK functions and bounded event stream. It is not treated as a full
Reticulum transport, interface, NomadNet, OMENchat, or shared-instance backend.

## OMENchat announcement-room feature identity

`omenchat-announcement-rooms` exists independently in the root OMENbrowser and
standalone `src/server` manifests. It is dependency-free and is included by
the canonical `desktop-product`, `desktop-product-static-media`,
`server-headless`, and `server-full` aliases. A capable client requests
`announcement-rooms-v1`; a capable server accepts it and shapes room values
per authenticated Link.

Legacy or non-negotiating peers continue receiving byte-exact four-field room
values. Server authorization is unconditional and does not depend on this
feature or negotiation. `scripts/verify-product-features.sh` fails if any
canonical client/server graph omits the production feature.

## OMENchat slow-mode feature identity

`omenchat-slow-mode` is dependency-free and included by the canonical
`desktop-product`, `desktop-product-static-media`, `server-headless`, and
`server-full` aliases. Client and server negotiate `room-slow-mode-v1` only
with `durable-mutations-v1`; exact legacy four-field and announcement-only
five-field room values remain unchanged for peers that do not negotiate it.

`omenchat-slow-mode-qualification` depends on the product feature but remains
excluded from every product alias. It owns only deterministic process-test
hooks such as the isolated room-policy transition and GUI auto-open behavior.
The product verifier requires the production feature and rejects the
qualification feature in release graphs.

## OMENchat room media-policy feature identity

`omenchat-room-media-policy` is dependency-free and included by canonical
`desktop-product`, `desktop-product-static-media`, `server-headless`, and
`server-full`. It depends on the already active announcement-room and slow-mode
features. Current peers select the cumulative seven-field room shape only
after explicit request/accept; non-negotiating peers retain their exact
four-, five-, or six-field shape and global upload admission.

`omenchat-room-media-policy-qualification` depends on the production feature
but remains excluded from every product alias. It owns only deterministic
process/GUI hooks. The product verifier requires the production feature and
rejects the qualification hook from release graphs.

Neither feature adds a runtime, interface, worker, timer, queue, cache,
subscription, or dependency. The production activation and rollback decision
is recorded in
`audits/omenchat-room-media-policy-activation-review.md`.

## Security and ownership requirements

A future external backend must be explicit opt-in, prefer a restrictive local
Unix socket, negotiate capabilities, verify endpoint ownership where supported,
recover from disconnect through snapshot plus event cursor, and never enable an
unauthenticated non-loopback endpoint. It must clearly identify which process
owns interfaces and identities before external mode can start network work.

## Migration and rollback

Existing external-mode configuration is preserved without conversion. To use
the integrated v0.9.6-4 runtime, select Managed and restart. Do not delete
identity, configuration, history, or cache data. Rolling back this safety gate
is source-only, but should not be done without a tested shared backend because
the former behavior launched an integrated runtime while labeling it External.

## Propagation-node operator view

The desktop and TUI Directory surfaces consume the project-owned bounded
propagation-node inventory. They show authenticated identity/name evidence,
freshness, path state, advertised stamp cost, compatibility, and whether the
node is selected. `unknown`, `stale`, and `not-known` remain distinct; the UI
does not infer trust or reachability from an advertised name.

Selection, refresh, and propagation sync controls call the existing
settings/runtime owners. Rendering the inventory performs no network work and
adds no timer or polling subscription. The bounded projection is cached in the
directory panel state and rebuilt only after directory, path-evidence, or
preferred-node changes, so TUI redraws do not repeatedly clone and sort
announce records.

Propagation-node refresh is an explicit operator action. It admits one global
in-flight refresh, coalesces concurrent attempts, enforces a 30-second
monotonic cooldown, considers at most three destination/association path
candidates, and has a six-second total deadline. Cancel, timeout, no-path,
failure, and success are separate visible outcomes. Desktop and TUI shutdown
cancel the owned operation. Selecting a node persists and notifies the runtime
but does not silently start path discovery.
