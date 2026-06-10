# 21 — Security, Privacy, and Threat Model

OMENbrowser_rs handles identities, messages, network paths, local storage, plugins, and terminal rendering. This document sets safety expectations for the final port.

## Assets to protect

- Reticulum private identity material.
- LXMF private/source identity material.
- Message contents.
- Attachments.
- Directory trust decisions.
- Config paths and local usernames.
- Plugin state.
- User-entered form fields.
- Logs that may contain addresses or message metadata.

## Hard rules

1. Never log private key material.
2. Never show private key material in diagnostics.
3. Never send plugin code identity secrets.
4. Never overwrite user identity files without backup and explicit path handling.
5. Never let network input crash the TUI.
6. Never trust remote Micron markup as terminal control sequences.
7. Never execute remote content as code.
8. Never silently auto-trust a peer from an announce.
9. Never block terminal restore on panic/error.

## Terminal escape safety

Remote pages/messages may contain hostile control characters.

Renderer must:

- sanitize or escape raw control characters;
- avoid passing arbitrary ANSI escape sequences through;
- preserve printable Unicode where safe;
- handle invalid UTF-8 without panic;
- constrain line/cell width.

Micron style tokens are parsed into ratatui styles, not printed as raw ANSI.

## Network privacy notes

Reticulum/LXMF metadata can reveal operational information such as timing, path availability, and hop counts. OMENbrowser_rs should not exaggerate anonymity.

Diagnostics should label hop/interface/path data as network diagnostics, not location.

Do not add UI features that claim to geolocate peers. Do not present hop count as physical distance.

Clearweb `http://` and `https://` URLs are outside Reticulum/NomadNet privacy guarantees. OMENbrowser_rs must not silently fetch remote clearweb media by default, because previews can expose the user's home IP address and timing metadata.

Current policy:

- clearweb links from NomadNet and OMENchat are routed through the external-browser prompt;
- the user can choose a preferred installed browser in Settings, but OMENbrowser_rs cannot force that browser to use Tor;
- Settings shows the default local SOCKS5/Tor proxy hint at `127.0.0.1:9050`, also checks Tor Browser Bundle's common `127.0.0.1:9150`, and reports whether a listener is detected;
- rich media previews remain disabled by default;
- if rich media previews are enabled and a SOCKS5/Tor proxy is detected, OMENbrowser_rs may fetch clearweb image bytes through that proxy only;
- clearweb media fetches use remote DNS through the SOCKS proxy and fail closed with no direct-TCP fallback.

Implementation direction:

- use SOCKS5 proxy support for the rich-media fetcher, since it works with Tor Browser's local proxy, system Tor, and non-Tor SOCKS proxies without embedding a full Tor client;
- consider `arti-client` for a later self-contained Rust Tor mode if we want OMENbrowser_rs to bootstrap Tor itself;
- avoid the old `tor` crate for new functionality.

## Local storage safety

Rules:

- atomic writes for settings/directory/message store;
- backup corrupted JSON before fallback;
- file permissions should be restrictive for identity/message stores where platform allows;
- filenames derived from network/user input must be sanitized;
- downloads must not overwrite without explicit user action;
- cache keys must avoid raw secret data if possible.

## Plugin safety

Plugin rules are in `docs/20-plugin-execution.md`. Security-critical summary:

- disabled by default;
- capabilities enforced;
- no identity secrets;
- timeout;
- output size limit;
- isolated state dir;
- repeated failure handling;
- clear warning for process plugins.

## Message safety

- Show sender hash/label clearly.
- Do not mark unknown senders as trusted.
- Failed outbound messages must not look delivered.
- Propagated/direct mode must be visible.
- Attachments must show size/type before opening/exporting.
- Message body rendering must sanitize terminal escapes.

## Directory trust safety

Trust levels are user-controlled.

Network announces may suggest names or capabilities, but they must not:

- override saved labels;
- raise trust;
- change preferred delivery;
- set identify-on-connect;
- set propagation node automatically unless configured.

## Interface safety

Managed interface config may expose the node to networks.

UI should distinguish:

- client-only connections;
- TCP server/listening mode;
- transport/node behavior if ever supported;
- I2P behavior;
- RNode/radio behavior.

Do not enable node hosting/transport mode silently.

## Diagnostics redaction

Diagnostics export must redact:

- private keys;
- full identity secret paths if user setting requests privacy;
- message bodies unless explicit export includes them;
- plugin secrets/settings marked secret;
- tokens/password fields;
- exact local username paths if redaction setting enabled.

Include enough information to debug:

- app version;
- feature flags;
- runtime mode;
- OS/platform;
- interface profile summary;
- identity display hash;
- error summaries;
- counts and statuses.

## Tests

Add tests for:

- terminal escape sanitization;
- invalid UTF-8 handling;
- secret redaction;
- identity backup on overwrite;
- unsafe download filename sanitization;
- plugin capability denial;
- saved directory label/trust not overwritten by announce;
- corrupted JSON backup;
- message failed state visible.

## Done when

- No known path leaks identity secret material.
- Remote markup/messages cannot inject terminal escapes.
- Plugins are controlled by explicit capabilities.
- Diagnostics are useful but safe.
