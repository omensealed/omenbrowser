# Network Backends

OMENbrowser keeps Reticulum and LXMF behind the project-owned `NetworkRuntime`
boundary. Application and UI code must not infer backend capabilities from the
crate version or a saved mode name.

## Managed integrated mode

`reticulum_instance_mode = "managed"` is the supported v0.9.5-1 product mode.
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

## Security and ownership requirements

A future external backend must be explicit opt-in, prefer a restrictive local
Unix socket, negotiate capabilities, verify endpoint ownership where supported,
recover from disconnect through snapshot plus event cursor, and never enable an
unauthenticated non-loopback endpoint. It must clearly identify which process
owns interfaces and identities before external mode can start network work.

## Migration and rollback

Existing external-mode configuration is preserved without conversion. To use
the integrated v0.9.5-1 runtime, select Managed and restart. Do not delete
identity, configuration, history, or cache data. Rolling back this safety gate
is source-only, but should not be done without a tested shared backend because
the former behavior launched an integrated runtime while labeling it External.
