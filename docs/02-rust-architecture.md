# 02 — Rust Architecture

## Architecture principle

The Rust port should be organized around explicit state, services, and async runtime boundaries. The Python version places a large amount of orchestration inside `ui/app.py`; the Rust version must split that into reusable modules.

## Workspace modules

Recommended crate/module responsibilities:

```text
omenbrowser-core
  shared models, settings, paths, identity, errors

omenbrowser-renderer
  Micron parser, MicronPlus transforms, cell renderer, style model

omenbrowser-runtime
  RuntimeAdapter trait, MockAdapter, Reticulum/LXMF bridge implementation

omenbrowser-services
  browser sessions, cache, partials, messages, message store, directory, interfaces, diagnostics

omenbrowser-ui
  ratatui/crossterm UI, workspace state, event loop, widgets

omenbrowser-plugins
  plugin manifest, typed hooks, safe capability registry
```

For a first implementation, these may be modules inside one crate. Preserve the boundaries anyway.

## Core data types

Port Python dataclasses into Rust types.

### Identity

```rust
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct IdentityProfile {
    pub label: String,
    pub path: PathBuf,
    pub hash_hex: String,
    pub managed: bool,
}
```

### Runtime status

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuntimeStatus {
    pub connected: bool,
    pub backend: RuntimeBackendName,
    pub active_identity: Option<IdentityProfile>,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum RuntimeBackendName {
    Auto,
    Mock,
    Reticulum,
    Bridge(String),
}
```

### Browser page

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrowserPage {
    pub url: String,
    pub markup: String,
    pub title: String,
    pub source: PageSource,
    pub metadata: BTreeMap<String, serde_json::Value>,
    pub request_data: Option<BTreeMap<String, String>>,
}
```

### Browser tab

This is new in Rust and central to the improved UI.

```rust
#[derive(Clone, Debug)]
pub struct BrowserTab {
    pub id: TabId,
    pub title: String,
    pub session: BrowserSession,
    pub address_input: String,
    pub current_page: Option<BrowserPage>,
    pub render_cache: RenderCache,
    pub loading: Option<LoadState>,
    pub partials: PartialRefreshState,
    pub scroll: ScrollState,
    pub focused_control: Option<FocusedControl>,
}
```

Every browser tab owns an independent `BrowserSession`. Do not use one global browser session.

### Conversation tab

This is new in Rust and central to the improved UI.

```rust
#[derive(Clone, Debug)]
pub struct ConversationTab {
    pub id: TabId,
    pub peer_hash: String,
    pub peer_label: String,
    pub thread: ConversationThread,
    pub draft_title: String,
    pub draft_body: String,
    pub attachments: Vec<PathBuf>,
    pub delivery_mode: DeliveryMode,
    pub include_ticket: bool,
    pub scroll: ScrollState,
    pub unread_at_open: u32,
}
```

Conversation tabs are UI state. Persist messages through `MessageStore`, not through UI state.

### Directory entry

Preserve the Python trust model:

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DirectoryEntry {
    pub destination_hash: String,
    pub display_name: String,
    pub kind: DirectoryKind,
    pub trusted: bool,
    pub trust_level: TrustLevel,
    pub saved: bool,
    pub identify_on_connect: bool,
    pub preferred_delivery: Option<PreferredDelivery>,
    pub sort_rank: Option<i32>,
    pub hosts_node: bool,
    pub associated_hash: Option<String>,
    pub node_associated_hash: Option<String>,
    pub last_seen: f64,
}
```

Recommended enums:

```rust
pub enum DirectoryKind { Node, Peer, Propagation, Unknown }
pub enum TrustLevel { Warning, Untrusted, Unknown, Trusted }
pub enum PreferredDelivery { Direct, Propagated }
```

### Interface profile

Port `ReticulumInterfaceProfile` but use enums for kind:

```rust
pub enum InterfaceKind {
    TcpClient,
    TcpServer,
    I2p,
    RNode,
    Unknown(String),
}
```

Keep fields for TCP host/port, IFAC network/passphrase, I2P peers/connectable, and RNode radio parameters.

## Application state

Use a single root `AppState` owned by the UI event loop.

```rust
pub struct AppState {
    pub settings: AppSettings,
    pub paths: AppPaths,
    pub runtime_status: RuntimeStatus,
    pub active_section: WorkspaceSection,
    pub browser_workspace: BrowserWorkspaceState,
    pub messages_workspace: MessagesWorkspaceState,
    pub directory_state: DirectoryPanelState,
    pub interfaces_state: InterfacesPanelState,
    pub diagnostics_state: DiagnosticsPanelState,
    pub logs: LogBuffer,
    pub plugins_state: PluginsPanelState,
    pub status_line: StatusLine,
}
```

`AppState` should not perform blocking I/O directly. It receives events from services/background tasks.

## Event model

Use an event enum to centralize UI updates:

```rust
pub enum AppEvent {
    Input(InputEvent),
    Tick,
    RuntimeStatus(RuntimeStatus),
    PageLoaded { tab_id: TabId, result: Result<BrowserPage, BrowserError> },
    PagePartialLoaded { tab_id: TabId, slot: String, result: Result<String, BrowserError> },
    DownloadFinished { tab_id: TabId, result: Result<DownloadedFile, BrowserError> },
    MessageReceived(MessageSummary),
    MessageSent { tab_id: Option<TabId>, result: Result<MessageSummary, MessageError> },
    DirectoryAnnounce(AnnouncePayload),
    RuntimeDebug(String),
    Log(String),
    Shutdown,
}
```

Background tasks send `AppEvent` over a Tokio channel. The UI loop consumes events and redraws.

## Service ownership

Recommended ownership:

- `RuntimeService`: owns `Arc<dyn RuntimeAdapter>`.
- `BrowserService`: stateless helper plus cache/runtime handles.
- `MessagingService`: owns `MessageStore`, `DirectoryService`, runtime handle.
- `DirectoryService`: owns directory persistence.
- `InterfaceConfigService`: owns interface profile/config persistence.
- `PluginService`: owns typed hooks/capabilities.

The UI should call services through methods that spawn async tasks where appropriate.

## Error model

Use `thiserror` for typed service errors:

```rust
#[derive(Debug, thiserror::Error)]
pub enum BrowserError {
    #[error("invalid address: {0}")]
    InvalidAddress(String),
    #[error("request cancelled")]
    Cancelled,
    #[error("runtime error: {0}")]
    Runtime(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
```

Convert to user-visible status messages at UI boundaries.

## Concurrency model

- UI render loop runs on main thread.
- Network/runtime tasks run on Tokio tasks.
- File I/O can be sync if small and not in render path; otherwise use spawn-blocking.
- Cancellation uses tokens per browser tab load and partial refresh.
- A stale background result must not overwrite a newer browser tab state.

Each browser tab should maintain a generation counter:

```rust
pub struct LoadState {
    pub generation: u64,
    pub target: String,
    pub started_at: Instant,
    pub cancel: CancellationToken,
}
```

When a result returns, apply it only if the tab still has the same generation.

## Logging

Use `tracing`. Keep an in-memory ring buffer for the Logs UI. Also write persistent logs to the app data directory.

Do not print secrets, identity private material, passphrases, or raw message bodies into diagnostics unless user explicitly exports debug data and the export warns them.

