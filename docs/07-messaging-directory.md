# 07 — Messaging and Directory

## Messaging goal

The Rust app must preserve LXMF client behavior while improving conversation UI. The service layer stores and updates messages; the UI layer opens independent conversation tabs.

## Python source

Reference files:

```text
src/omenbrowser/services/message_store.py
src/omenbrowser/services/messages.py
src/omenbrowser/services/directory.py
```

## Message model

Port `MessageSummary` and `ConversationThread`.

Required fields:

- peer hash;
- peer label;
- title;
- content;
- timestamp;
- transport method;
- delivered;
- failed;
- incoming;
- unread;
- message ID;
- public fields;
- attachments.

Use explicit enums where possible:

```rust
pub enum TransportMethod { Direct, Propagated, Unknown(String) }
pub enum DeliveryState { Incoming, Pending, Delivered, Failed }
```

## Message store

The Python store writes one thread per peer. Preserve this simple model at first.

Recommended layout:

```text
app_data/messages/
  <peer_hash>.json
```

Each file stores:

```rust
pub struct StoredConversationThread {
    pub peer_hash: String,
    pub peer_label: String,
    pub unread_count: u32,
    pub messages: Vec<MessageSummary>,
}
```

Message store methods:

- append message;
- list threads;
- get thread;
- mark read;
- update delivery by message ID;
- update peer label;
- ensure thread;
- reconcile pending messages.

## Messaging service

Messaging service responsibilities:

- ingest runtime messages;
- convert raw runtime messages to `MessageSummary`;
- resolve labels from directory;
- compose outbound message;
- call runtime send;
- append outgoing message;
- update outbound status;
- reconcile pending state against runtime pending IDs.

## Conversation tabs

UI state for conversation tabs should be separate from stored messages.

A conversation tab owns:

- peer hash;
- peer label;
- draft title;
- draft body;
- attachment paths;
- send mode;
- include ticket flag;
- scroll state.

Blank new conversation tabs are allowed. They should render a peer-destination input instead of inventing mock peer hashes, and the send path must continue to block until the peer hash is a valid LXMF delivery destination. Closing a pane only hides that pane; deleting a conversation is the destructive action that removes the persisted thread from message storage. If the last conversation is deleted, the UI should replace it with one blank compose tab so the Messages workspace remains usable without restoring old history.

When a message arrives for an open conversation:

- update the corresponding thread in store;
- update open tab snapshot;
- if the tab is not active, increment visible unread indicator;
- if active, mark read after display.

## Sending

The UI should expose two clear send buttons/modes:

- Direct Send
- Send via Propagation

When sending:

1. Validate peer hash.
2. Validate body/title according to minimal requirements.
3. Snapshot attachments.
4. Call messaging service.
5. Append pending outgoing message immediately if possible.
6. Update status on runtime outbound event.
7. Show failure in both conversation and status/logs.

The desktop UI should create a visible, non-persisted pending row before awaiting the runtime send future. This is especially important for native propagated LXMF because path discovery, peer-key recall, stamp generation, and propagation-node link setup can take long enough that waiting for `send_message` to return makes the conversation look stuck. The pending row is replaced by the stored `MessageSummary` when the send result arrives, or by runtime propagation evidence if the native router reports propagation-node acceptance first.

For native direct LXMF, `delivered=true` is reserved for actual LXMF delivery evidence. Packet submission and RNS packet proof rows should show as submitted/proof-observed with peer delivery unconfirmed, matching the Python client's queue-first behavior.

Desktop status text should not call native LXMF packet submission or packet proof "delivered" unless there is stronger LXMF delivery evidence. The current native path still lacks the Python `LXMRouter` delivered callback parity, so packet proof is transport evidence, not proof the destination user saw the message.

## Attachments

Attachment support should preserve Python behavior initially:

- store attachment path summaries;
- pass attachment paths to runtime adapter;
- show name and size when known;
- do not load huge attachments into UI state.

## Directory model

Directory entries represent nodes, peers, and propagation destinations.

Required fields:

- destination hash;
- display name;
- kind;
- trusted/trust level;
- saved;
- identify on connect;
- preferred delivery;
- sort rank;
- hosts node;
- associated hash;
- node associated hash;
- last seen.

## Trust levels

Preserve Python constants:

```text
WARNING   = 0x00
UNTRUSTED = 0x01
UNKNOWN   = 0x02
TRUSTED   = 0xFF
```

Rust should expose them as an enum while preserving serializable numeric value where useful.

## Directory service methods

Port equivalent behavior:

- load/save;
- clear transient announces;
- ingest announce;
- sync discoveries;
- save entry;
- remove saved entry;
- set trust/trust level;
- trust lookup;
- set preferred delivery;
- set identify-on-connect;
- find entry;
- list entries;
- known nodes;
- propagation hash for node;
- list live entries;
- filtered entries.

Live announce persistence must be coalesced. Runtime announces can arrive continuously, and
rewriting the full directory file for every announce blocks the desktop UI on large Reticulum
directories. Explicit user changes such as Save, Trust/Untrust, preferred delivery, and
identify-on-connect still save immediately. Transient announce changes should mark the directory
dirty, then flush on a low-frequency debounce and on shutdown.

## Directory UI integration

Actions:

- Save
- Remove Saved
- Trust/Untrust or cycle trust level
- Identify on connect toggle
- Browse Node
- Message Peer
- Use Propagation
- Clear Propagation
- Request Path
- Inspect Destination

Directory must integrate with:

- Browser workspace: browse selected node in current or new browser tab.
- Messages workspace: open peer conversation tab.
- Runtime: set preferred propagation node.
- Settings: persist preferred propagation node.

## Sorting

Use the same spirit as Python sorting:

- saved/trusted entries should be easy to find;
- nodes/peers/propagation can be filtered;
- recent announcements should remain visible;
- placeholder names should not overwrite better labels;
- persistent saved entries should not be discarded when transient announce state is cleared.

Transient live-only entries should be pruned during long-running sessions as well as during startup. Announce ingest should drop live-only rows older than the retention window and enforce a bounded transient count so stale nodes, peers, and propagation destinations do not grow the directory indefinitely. Saved, trusted, identify-on-connect, and preferred-delivery rows are persistent and must survive transient pruning.

Desktop directory rendering should avoid creating full UI cards for thousands of live rows at once. Render a bounded first page for the current kind/scope/filter and guide the user toward search or saved/trusted scopes for narrower views.
