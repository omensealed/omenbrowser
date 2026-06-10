# 19 — UI Finalization

This document finishes the tabbed OMENbrowser_rs interface.

## Goal

The Rust UI should be a real terminal workspace:

- multiple browser tabs;
- multiple conversation tabs;
- sidebar navigation;
- live directory/interfaces/settings/diagnostics/logs/plugins panels;
- clear focus model;
- mouse and keyboard support;
- visible network/message/browser status;
- no blocking network calls in render/input paths.

## Current foundation

The app has:

- top-level sections;
- browser tabs;
- conversation tabs;
- command/address input;
- title/body composer input;
- mouse routing for major areas;
- non-blocking browser/message tasks;
- service-backed panels.

Still needed:

- scroll wheels and scroll state;
- inline link/control hit testing;
- richer panel actions;
- help overlay completion;
- status badges;
- error/toast/log UX;
- command palette/history if useful.

## Focus model

Use explicit focus states. Avoid implicit behavior based on “last clicked thing” only.

Suggested focus enum:

```rust
pub enum FocusArea {
    Header,
    Sidebar,
    BrowserTabs,
    BrowserContent,
    BrowserCommand,
    BrowserPageControl(ControlId),
    ConversationTabs,
    ConversationThread,
    ConversationTitle,
    ConversationBody,
    DirectoryList,
    InterfacesList,
    SettingsList,
    DiagnosticsPanel,
    LogsPanel,
    PluginsPanel,
    HelpOverlay,
}
```

Keep current enum if equivalent exists, but make every input path explicit.

## Scroll state

Track scroll per panel/tab:

- browser content scroll per browser tab;
- conversation thread scroll per conversation;
- sidebar scroll;
- directory list scroll;
- logs scroll;
- diagnostics scroll;
- plugins scroll.

Mouse wheel should apply to the panel under cursor.

Keyboard scroll expectations:

- PageUp/PageDown scroll focused content;
- Home/End jump in focused list/content;
- Up/Down move selection in list focus;
- Ctrl-Up/Ctrl-Down or similar scroll without changing selection where needed.

## Browser tab UI

Must show:

- tab title;
- active tab marker;
- loading state;
- error state;
- partial refresh indicator;
- modified/form-state indicator if controls changed;
- close affordance if mouse hit testing supports it.

## Conversation tab UI

Must show:

- peer label/hash;
- unread count;
- pending send marker;
- failed send marker;
- direct/propagated mode;
- active tab marker.

## Status bar

Status bar should include concise indicators:

- network: offline/mock/live/starting/error;
- identity display hash/name;
- interface count/up count;
- known path/announce activity;
- LXMF receive/send status;
- propagation node status;
- active section;
- key hint.

Do not show secret identity paths or keys in the status bar.

## Help overlay

The `?` overlay should be context-sensitive.

It must list at least:

Global:

- q quit;
- F1–F8 sections;
- Tab / Shift-Tab focus;
- ? help;
- Esc cancel/close overlay/input.

Browser:

- Ctrl-T new tab;
- Ctrl-W close tab;
- Left/Right tab cycle;
- Ctrl-L address;
- Enter open/activate;
- Ctrl-R reload;
- Alt-Left/Alt-Right history;
- Ctrl-D download;
- PageUp/PageDown scroll.

Messages:

- Ctrl-N new conversation;
- Ctrl-Y edit title;
- Ctrl-E edit body;
- Ctrl-S send;
- Ctrl-P toggle propagated/direct;
- Ctrl-U toggle ticket;
- Ctrl-G sync.

Directory:

- Enter open selected;
- m message peer;
- s save/unsave;
- t trust cycle;
- p set propagation node;
- / filter/search.

Interfaces/settings/diagnostics/logs/plugins should have their own local help sections as implemented.

## Error/toast handling

Errors should appear without crashing.

Suggested model:

- transient toast for recent error/success;
- logs panel for persistent history;
- diagnostics panel for structured snapshots;
- errors attached to relevant tab/conversation where possible.

Network timeouts should not look like app crashes.

## Mouse hit testing

Complete mouse support in layers:

1. scroll wheel routing;
2. browser tabs close/select;
3. conversation tabs close/select;
4. inline Micron links/controls;
5. directory row actions;
6. settings/interface toggles;
7. diagnostics/log row expand/copy.

Mouse mapping should stay deterministic and unit-tested.

## Theme

Keep OMEN style, but do not sacrifice readability.

Theme should support:

- normal terminal fallback;
- OMEN purple/red accent theme;
- high-contrast option;
- page-rendered Micron colors with policy to avoid unreadable combinations.

## Tests

Add tests for:

- focus transitions;
- scroll state isolation per tab;
- help overlay context;
- mouse wheel target mapping;
- status bar formatting without secrets;
- directory row action mapping;
- inline link hit region mapping;
- error toast lifecycle.

## Done when

- User can operate every major app feature by keyboard.
- Mouse works for core navigation and page links/controls.
- UI never freezes from network operations.
- Status/help/error feedback is clear enough for daily use.
