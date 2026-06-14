# Getting Online Fast

This is the shortest path for a new OMENbrowser_rs user who wants the Directory,
NomadNet pages, LXMF, and OMENchat discovery to start showing real network data.

## Recommended First Run

1. Start OMENbrowser_rs.
2. Open `Interfaces`.
3. Add or enable the `WNS` and `RMAP` gateway presets.
4. Add any private gateway you personally use, or configure your RNode/LoRa
   interface if that is your normal Reticulum path.
5. Open `Identities` and set a recognizable identity label.
6. Restart OMENbrowser_rs.

After restart, the selected interfaces and identity label load immediately with
the app. This gives the runtime a clean startup path, which is usually the most
reliable way to begin seeing announces, Directory entries, NomadNet nodes,
LXMF peers, propagation nodes, and OMENchat servers.

## Why WNS And RMAP Are Recommended

The bundled gateway presets are there so new users can get onto RNS without
hand-writing a Reticulum config on day one. For people interested in current
OMEN development, `WNS` and `RMAP` are the recommended public presets because
official OMEN test nodes and services are expected to stay connected there.

That means a new user who enables those presets has the best chance of seeing
OMEN-related Directory entries, NomadNet pages, OMENchat servers, and LXMF
activity without needing a private gateway first.

This recommendation can change as the network changes. If OMEN development
moves to different gateway presets, this document should be updated.

## Private Gateways And RNodes

If you already have a private Reticulum gateway, add it in `Interfaces` and use
that alongside or instead of public presets.

If you use RNode/LoRa hardware, configure it as your normal Reticulum path.
Public TCP gateways are convenient for getting started, but they are not the
only valid way to use OMENbrowser_rs.

## If Nothing Appears

- Confirm the selected interface shows connected traffic in `Interfaces` or
  `Monitoring`.
- Wait a little for announces to arrive.
- Use `Diagnostics` and `Logs` to check for interface or path errors.
- Restart once after changing interface presets or identity labels.
- If testing multiple clients on one machine, use separate `--app-root` paths so
  identities and Reticulum storage do not collide.
