# 18 — Live Directory, Announces, and Known Nodes

This document finishes the directory side of the port.

## Goal

The directory should become a live Reticulum/NomadNet/LXMF address book and discovery panel, not only a saved JSON list.

## Existing foundation

Current Rust app already has:

- `DirectoryService`;
- saved entries;
- announce stream persistence;
- trust levels;
- preferred delivery;
- identify-on-connect;
- known nodes;
- propagation lookup;
- live entries;
- filtered entries;
- placeholder-name protection;
- UI panel backed by service state.

Still needed:

- live native announce ingestion;
- richer app-data parsing;
- UI row actions;
- pruning/coalescing;
- relationship with LXMF contacts and browser nodes.

## Announce ingestion path

Correct flow:

```text
native Reticulum event
-> RuntimeEvent::Announce
-> AppEvent::Runtime
-> DirectoryService::ingest_announce
-> Directory panel refresh
-> optional toast/log
```

Do not let the UI parse raw announce bytes.

## DirectoryEntry requirements

Directory entries should support:

- destination hash;
- display name;
- app kind/source kind;
- known browser/node destination;
- known LXMF address;
- last announced time;
- first seen time;
- trust level;
- saved flag;
- preferred delivery method;
- propagation node flag/details;
- interface/hop info if available;
- raw app-data summary for diagnostics;
- user notes if implemented later.

## App-data parsing

Parse native announce app-data conservatively.

Rules:

- invalid UTF-8 must not crash;
- unknown app-data must be preserved as diagnostic summary;
- known OMEN/NomadNet/LXMF app-data should populate typed fields;
- saved user label must not be overwritten by placeholder announce names;
- trust/saved state must survive live updates.

## De-duplication

Entries should be keyed by stable destination hash/address.

If two announces reference the same entity through different app types, merge carefully:

- browser/node destination may be distinct from LXMF peer address;
- keep both if known;
- update last-seen time;
- prefer saved user label over network label;
- prefer trusted saved delivery setting over transient hint.

## Pruning

Transient live entries can expire from the visible live list, but saved entries must remain.

Suggested policy:

- saved entries never pruned automatically;
- live-only entries hidden after configured stale threshold;
- announce history may be capped by count/time;
- diagnostics can show stale/hidden count.

## UI actions

Directory panel should support keyboard and mouse actions:

- open node in browser tab;
- open/reuse conversation;
- save/unsave entry;
- trust/distrust or trust-level cycle;
- set preferred delivery direct/propagated/auto;
- set as propagation node if compatible;
- request path/warm path;
- inspect destination;
- copy destination/address where clipboard support exists;
- filter/search.

Actions must call `DirectoryService` or runtime trait methods, not mutate raw lists in UI.

## Relationship with messaging

When opening a conversation from directory:

1. Resolve LXMF-capable peer address.
2. Create or reuse conversation tab.
3. Apply display label from directory.
4. Load message thread from store.
5. Preserve peer hash as stable identity.

When inbound LXMF message arrives from unknown peer:

1. Messaging service stores message.
2. Directory service may create/update a transient peer entry.
3. UI can show unknown peer safely.

## Relationship with browser

When opening a node:

1. Resolve browser destination/page.
2. Create or reuse browser tab depending on user action.
3. Navigate through browser service.
4. Directory state may track last opened time.

## Diagnostics

Directory diagnostics should include:

- saved entry count;
- live entry count;
- stale hidden count;
- announce history count;
- trusted count;
- propagation node candidates;
- last announce time;
- parse failure count.

## Tests

Add tests for:

- announce creates live entry;
- saved label survives announce update;
- trust survives announce update;
- duplicate announce updates last seen only;
- invalid app-data does not crash;
- inbound message creates peer entry;
- open node action creates browser tab action;
- open peer action creates conversation tab action;
- stale live entry hidden but saved entry visible.

## Done when

- Live announces populate directory through runtime events.
- Directory actions operate through services/runtime.
- Saved/trusted user state is never destroyed by transient network data.
