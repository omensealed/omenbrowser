# 03 — UI Workspace

## UI goal

The Rust version must improve the Python UI by turning Browser and Messages into true multi-window/tab workspaces inside the main app window.

The UI should still run well in a terminal. Use `ratatui` plus `crossterm` unless a later decision document explains a better choice.

## Top-level layout

The app should have one main window with a top navigation bar, workspace-specific controls, main content, and global status bar.

```text
┌──────────────────────────────── OMENbrowser_rs ────────────────────────────────┐
│ Browser  Messages  Directory  Interfaces  Settings  Diagnostics  Logs  Plugins │
├────────────────────────────────────────────────────────────────────────────────┤
│ workspace toolbar / nested tabs / address or filter controls                    │
├────────────────────────────────────────────────────────────────────────────────┤
│ workspace content                                                               │
├────────────────────────────────────────────────────────────────────────────────┤
│ identity | backend | destination/path | propagation | task/status | clock        │
└────────────────────────────────────────────────────────────────────────────────┘
```
An improvement from this idea would be to make the top nav a sliding menu on the left that also contains what conversations and browser tabs we have open as well as the ability to access all of those menu options.
## Top-level sections

Required sections:

1. Browser
2. Messages
3. Directory
4. Interfaces
5. Settings
6. Diagnostics
7. Logs
8. Plugins

Keyboard shortcuts should be supported, but mouse support is desirable if the terminal supports it.

Suggested shortcuts:

```text
F1 Browser
F2 Messages
F3 Directory
F4 Interfaces
F5 Settings
F6 Diagnostics
F7 Logs
F8 Plugins
Ctrl+T new browser tab
Ctrl+W close current tab
Ctrl+L focus address bar
Esc cancel load or back out of focused widget
Alt+Left browser back
Alt+Right browser forward
Ctrl+R reload
```

## Browser workspace

The Browser workspace contains multiple independent browser tabs that can be maximized inside the browser or tabbed to display multiple sites at once.

```text
┌ Browser ───────────────────────────────────────────────────────────────────────┐
│ [mock.node:/] [node:chat] [the.fusion.chamber:/] [+]                           │
│ URL: [ mock.node:/                                                  ] [Open]   │
│ [Back] [Forward] [Reload] [Stop] [Download] [Duplicate] [Close]                │
├────────────────────────────────────────────────────────────────────────────────┤
│ Micron rendered cell-grid content                                              │
│                                                                                │
└────────────────────────────────────────────────────────────────────────────────┘
```

Each tab owns:

- `BrowserSession`;
- current URL;
- history and forward stack;
- address input;
- field/control state;
- partial refresh slots;
- scroll state;
- focused control/link;
- load generation/cancellation token;
- current rendered document cache.

### Browser tab operations

Required operations:

- new blank tab;
- new tab from link;
- close tab;
- duplicate tab;
- rename title from page title or URL;
- open URL;
- open relative URL;
- back;
- forward;
- reload;
- cancel current request;
- download current URL or selected download URL;
- focus next/previous link or field;
- submit focused control;
- edit field controls in-page;
- copy visible destination/hash/status where possible.

### Browser load behavior

When opening a page:

1. Increment the tab generation.
2. Store `LoadState` with target and cancellation token.
3. Spawn async fetch through `RuntimeService`.
4. Render loading indicator in tab/status bar.
5. Apply the result only if generation still matches.
6. Parse Micron and update render cache.
7. Extract partial refresh descriptors and schedule refresh tasks.
8. Update history only after a successful page load unless request type intentionally says otherwise.

### Partial refresh behavior

Partials belong to the browser tab that created them. If the tab navigates away, stale partial results must be ignored.

The UI should show a subtle indicator when partial refreshes are pending.

## Messages workspace

The Messages workspace contains multiple conversation tabs/windows.

```text
┌ Messages ──────────────────────────────────────────────────────────────────────┐
│ Threads: [All] [Unread] [Saved]       Conversations: [Peer A] [Peer B] [+]     │
├───────────────┬────────────────────────────────────────────────────────────────┤
│ thread list   │ selected conversation scrollback                                │
│               │                                                                │
├───────────────┴────────────────────────────────────────────────────────────────┤
│ Peer:  [ lxmf delivery destination hash                     ]                 │
│ Title: [                                                    ]                  │
│ Body:  [ multi-line composer                               ]                  │
│ Attachments: file1.webp file2.txt                                             │
│ [Direct Send] [Send via Propagation] [Include Ticket] [Add Attachment]         │
└────────────────────────────────────────────────────────────────────────────────┘
```

Each conversation tab owns:

- peer hash;
- peer label;
- current thread snapshot;
- draft title;
- draft body;
- attachments;
- selected send mode;
- include-ticket flag;
- scroll state;
- unread count at open.

Conversation panes should render a bounded visible slice of scrollback and compact message previews in the normal pane view. Full delivery diagnostics and long message bodies belong in the selected-message/details area or logs. This keeps Iced's software renderer from repainting thousands of offscreen rounded cards while preserving access to the underlying message history.

The message store remains shared. If the same peer is open in multiple tabs, the app should avoid duplicate tabs by default, but allow duplicate/detached tabs later if explicitly implemented.

### Conversation actions

Required operations:

- open conversation from thread list;
- open conversation from Directory peer;
- start conversation by peer hash;
- create a blank conversation and type or paste a peer hash before sending;
- delete a stored conversation thread when the user intentionally chooses Delete;
- save contact label;
- mark read when viewing;
- send direct;
- send propagated;
- attach file paths;
- clear attachments;
- update delivery status live;
- display failed/pending/delivered state clearly.

## Directory workspace

Directory has node, peer, and propagation views. Preserve the Python model:

- live nodes;
- live peers;
- live propagation;
- saved nodes;
- saved peers;
- saved propagation.

Recommended layout:

```text
Filter: [                                   ]
Tabs: [Nodes] [Peers] [Propagation] [Saved Nodes] [Saved Peers] [Saved Propagation]
List panel + detail panel
Actions: Save, Remove Saved, Trust/Untrust, Identify, Browse Node, Message Peer, Use Propagation, Clear Propagation
```

Directory selection must integrate with Browser and Messages:

- Browse Node opens a browser tab.
- Message Peer opens a conversation tab.
- Use Propagation updates runtime/settings.

The desktop UI should not rebuild or sort the full directory model for every runtime announce while
another workspace is active. Directory service state may update continuously in the background, but
the rendered directory list should refresh when the Directory workspace is visible or when entering
that workspace. This keeps Reticulum announce volume from stealing render/input time from Browser
and Messages panes.

## Interfaces workspace

Port the Python interface management UI but make it clearer.

Required sections:

- runtime mode selector: auto/mock/managed/external as implemented;
- interface list;
- add profile type: TCP client/server, I2P, RNode/LoRa;
- gateway presets;
- editor fields for selected interface;
- enabled toggle;
- I2P connectable toggle;
- apply interfaces button;
- visible restart/apply warning.

## Settings workspace

Required controls:

- create managed identity;
- attach existing identity;
- import identity copy;
- rename active identity;
- reveal/copy data folder path;
- export identity backup;
- announce LXMF now;
- announce on start toggle;
- sync propagation now;
- periodic sync toggle;
- preferred propagation node hash;
- clear propagation;
- runtime package/bridge diagnostic action if applicable.

Identity operations must never silently overwrite existing identity data.

## Diagnostics workspace

Show:

- backend status;
- app version;
- platform;
- app data paths;
- active identity label/hash/path hint;
- Reticulum config/storage path;
- interface stats;
- propagation status;
- directory counts;
- cache stats;
- plugin list;
- current browser tab count;
- current conversation tab count.

Include export diagnostics, with secret redaction.

## Logs workspace

Show in-memory logs with filtering.

Minimum filters:

- all;
- runtime;
- browser;
- LXMF;
- propagation;
- directory;
- plugin;
- error/warning.

## Plugins workspace

Do not reproduce unsafe arbitrary Python execution. The Rust UI should support:

- list installed plugin manifests;
- enable/disable;
- remove;
- install local plugin folder only if plugin system supports the declared format;
- show permission/capability warning;
- show plugin errors.

## Status bar

The status bar should always show:

- active identity label/hash short form;
- backend: mock/reticulum/bridge;
- current workspace context;
- active browser destination or active peer;
- propagation node short hash/status;
- current task or latest error;
- clock.

## Theme

Default theme should be dark, OMEN-like, and readable. Use strong focus states. Avoid relying on color alone; use labels and symbols too.
