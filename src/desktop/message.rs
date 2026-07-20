use std::path::PathBuf;

use iced::widget::scrollable::RelativeOffset;
use iced::widget::{pane_grid, text_editor};
use iced::{keyboard, window};

use crate::app::{DirectoryScope, TabId};
#[cfg(feature = "chat-client")]
use crate::browser::DownloadedFile;
#[cfg(feature = "chat-client")]
use crate::chat::protocol::RoomId;
#[cfg(feature = "chat-client")]
use crate::chat::ChatSessionId;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use crate::chat::OmenChatDescriptor;
use crate::desktop::page_widget::PageMessage;
use crate::workspace::WorkspaceSection;

#[cfg(feature = "chat-client")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::desktop) enum OmenChatDraftCommandResult {
    NotCommand,
    HandledClear,
    HandledKeep,
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
#[derive(Clone, Debug)]
pub(in crate::desktop) struct OmenChatLiveOpenCompletion {
    pub(in crate::desktop) descriptor: OmenChatDescriptor,
    pub(in crate::desktop) result: Result<crate::runtime::OmenChatLinkOpened, String>,
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
#[derive(Clone, Debug)]
pub(in crate::desktop) struct OmenChatLiveReconnectCompletion {
    pub(in crate::desktop) session_id: ChatSessionId,
    pub(in crate::desktop) generation: u64,
    pub(in crate::desktop) descriptor: OmenChatDescriptor,
    pub(in crate::desktop) result: Result<crate::runtime::OmenChatLinkOpened, String>,
}

#[cfg(feature = "chat-client")]
#[derive(Clone, Debug)]
pub(in crate::desktop) struct OmenChatMediaCacheCompletion {
    pub(in crate::desktop) session_id: ChatSessionId,
    pub(in crate::desktop) cache_key: String,
    pub(in crate::desktop) generation: u64,
    pub(in crate::desktop) result:
        Result<super::omenchat_desktop_state::CachedOmenChatMedia, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::desktop) enum DesktopPane {
    Browser(TabId),
    Conversation(u64),
    #[cfg(feature = "chat-client")]
    OmenChat(ChatSessionId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::desktop) struct ExternalLinkPrompt {
    pub(in crate::desktop) url: String,
    pub(in crate::desktop) source_tab: Option<TabId>,
}

#[cfg(feature = "chat-client")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::desktop) enum OmenChatMediaLoadState {
    Loading {
        message: String,
        received: Option<u64>,
        total: Option<u64>,
    },
    Cached {
        path: String,
        content_type: String,
        animated: bool,
    },
    Failed {
        message: String,
    },
}

#[derive(Clone, Debug)]
pub(in crate::desktop) enum ThemeMessage {
    SetTheme(String),
    SetFontSize(u16),
    ToggleReducedMotion,
}

#[derive(Clone, Debug)]
pub(in crate::desktop) enum ClearwebMessage {
    ToggleSocksProxy,
    ToggleRemoteMedia,
}

#[derive(Clone, Debug)]
pub(in crate::desktop) enum ExternalBrowserMessage {
    SelectPreferred(usize),
    ClearPreferred,
    OpenWith(usize),
    CopyUrl,
    PromptUrl(String),
    DismissPrompt,
}

#[derive(Clone, Debug)]
pub(in crate::desktop) enum RuntimeMessage {
    ToggleAutoSyncAfterPropagationAccept,
    SelectNativeBackend,
    StartNativeRuntime,
    NativeQuickstart,
    InterfaceStatsSampled(Result<crate::runtime::InterfaceStats, String>),
}

#[derive(Clone, Debug)]
pub(in crate::desktop) enum IdentityMessage {
    Create,
    ActivateManaged(String),
    ActiveLabelChanged(String),
    DeleteActive,
    ConfirmDeleteActive,
    CancelDeleteActive,
    ClearActive,
    AnnounceNow,
    CopyActiveHash,
}

#[derive(Clone, Debug)]
pub(in crate::desktop) enum PluginMessage {
    Select(usize),
    Toggle(usize),
    BeginRemove(usize),
    ToggleSelected,
    BeginInstall,
    BeginSelectedRemove,
    Refresh,
    ShowLogs,
}

#[derive(Clone, Debug)]
pub(in crate::desktop) enum DirectoryMessage {
    SwitchKind(crate::directory::DirectoryKind),
    SwitchScope(DirectoryScope),
    FilterChanged(String),
    SelectEntry(usize),
    OpenEntry(usize),
    OpenPeerChat(usize),
    #[cfg(feature = "chat-client")]
    OpenOmenChat(usize),
    InspectPeer(usize),
    SaveEntry(usize),
    ToggleTrust(usize),
    ToggleIdentify(usize),
    CycleDelivery(usize),
    RequestPath(usize),
    RefreshPropagation(usize),
    CancelPropagationRefresh,
    UsePropagation(usize),
    ClearPropagation,
}

#[derive(Clone, Debug)]
pub(in crate::desktop) enum InterfaceMessage {
    CreateTcpClient,
    CreateI2p,
    CreateRNode,
    CreateGatewayPreset(String),
    SelectProfile(usize),
    ToggleEnabled(usize),
    DeleteProfile(usize),
    ConfirmDelete,
    CancelDelete,
    NameChanged { profile_id: String, value: String },
    TcpClientHostChanged { profile_id: String, value: String },
    TcpClientPortChanged { profile_id: String, value: String },
    TcpClientIfacNetworkChanged { profile_id: String, value: String },
    TcpClientIfacPassphraseChanged { profile_id: String, value: String },
    TcpServerHostChanged { profile_id: String, value: String },
    TcpServerPortChanged { profile_id: String, value: String },
    TcpServerIfacNetworkChanged { profile_id: String, value: String },
    TcpServerIfacPassphraseChanged { profile_id: String, value: String },
    ToggleI2pConnectable(usize),
    I2pPeersChanged { profile_id: String, value: String },
    RNodeDevicePortChanged { profile_id: String, value: String },
    RNodeFrequencyChanged { profile_id: String, value: String },
    RNodeBandwidthChanged { profile_id: String, value: String },
    RNodeTxPowerChanged { profile_id: String, value: String },
    RNodeSpreadingFactorChanged { profile_id: String, value: String },
    RNodeCodingRateChanged { profile_id: String, value: String },
}

#[derive(Clone, Debug)]
pub(in crate::desktop) enum DiagnosticsMessage {
    Show,
    PreviewManagedConfig,
    ExportManagedConfig,
    PreviewBundle,
    ExportBundle,
    PreviewLiveInteropReport,
    ExportLiveInteropReport,
    NativePreflight,
    NativeSmokeDryRun,
    NativeSmokeLiveProbe,
    NativeLiveFetchValidate,
    NativeLxmfSmokeSend,
    NativeLxmfInterop,
    NativeLxmfPropagationDiagnostics,
    SyncPropagationNow,
    BeginKnownDestinationsPreload,
}

#[derive(Clone, Debug)]
pub(in crate::desktop) enum WorkspacePaneMessage {
    NewConversation,
    CloseConversationTab(u64),
    Clicked(pane_grid::Pane),
    Dragged(pane_grid::DragEvent),
    Resized(pane_grid::ResizeEvent),
    Maximize(pane_grid::Pane),
    Restore,
    Close(pane_grid::Pane),
    RestoreDesktop(DesktopPane),
}

#[derive(Clone, Debug)]
pub(in crate::desktop) enum ShellMessage {
    SwitchSection(WorkspaceSection),
    ToggleNavigation,
    WorkspaceScrollTick,
    InternalEventsReady,
    PersistenceDeadlineReached,
    MonitoringTick,
    LxmfReconcileDeadlineReached,
    BrowserPartialDeadlineReached,
    OmenChatMaintenanceDeadlineReached,
    WindowCloseRequested(window::Id),
    WindowShutdownBegin(window::Id),
    WindowShutdownComplete {
        window_id: window::Id,
        outcome: ShutdownOutcome,
    },
    KeyboardModifiersChanged(keyboard::Modifiers),
}

#[derive(Clone, Debug)]
pub(in crate::desktop) enum BrowserMessage {
    SelectTab(usize),
    NewTab,
    CloseTab,
    ClosePaneTab(TabId),
    AddressChanged(String),
    OpenAddress,
    PaneAddressChanged { tab_id: TabId, value: String },
    OpenPaneAddress(TabId),
    ReloadPane(TabId),
    PaneBack(TabId),
    PaneForward(TabId),
    PaneTop(TabId),
    StopPaneTask(TabId),
    InlineProbePane(TabId),
    LiveProbePane(TabId),
    WarmPanePath(TabId),
    RetryPaneAfterPath(TabId),
    PanePathDiagnostics(TabId),
    CapturePaneRender(TabId),
    DismissPaneWarning(TabId),
    DismissPaneRequest(TabId),
    TogglePaneIdentify(TabId),
    OpenSetupAddress,
    Reload,
    Back,
    Forward,
    StopTask,
    InlineProbe,
    LiveProbe,
    WarmPath,
    RetryAfterPath,
    PathDiagnostics,
    CaptureRender,
    FieldKey(BrowserFieldKey),
    SubmitFieldDraft,
    CancelFieldDraft,
    FocusItem { reverse: bool },
    ActivateFocusedItem,
    Zoom { direction: isize },
    ScrollPage { direction: isize },
    Page(PageMessage),
    PageForTab { tab_id: TabId, page: PageMessage },
}

#[derive(Clone, Debug)]
pub(in crate::desktop) enum ConversationMessage {
    Switch(usize),
    Scrolled {
        conversation_id: u64,
        offset: RelativeOffset,
    },
    JumpToPresent(u64),
    TitleChanged(String),
    BodyChanged(String),
    PanePeerChanged {
        conversation_id: u64,
        value: String,
    },
    PaneTitleChanged {
        conversation_id: u64,
        value: String,
    },
    PaneBodyChanged {
        conversation_id: u64,
        value: String,
    },
    PaneBodyEdited {
        conversation_id: u64,
        action: text_editor::Action,
    },
    PickAttachment(u64),
    RemoveAttachment {
        conversation_id: u64,
        index: usize,
    },
    OpenAttachment(PathBuf),
    TogglePaneDeliveryMode(u64),
    TogglePaneTicket(u64),
    SendPaneDraft(u64),
    PrepareLatestRetryForConversation(u64),
    SendLatestRetryForConversation(u64),
    SelectPaneRow {
        conversation_id: u64,
        key: String,
    },
    PrepareRetryForConversationRow {
        conversation_id: u64,
        key: String,
    },
    SendRetryForConversationRow {
        conversation_id: u64,
        key: String,
    },
    CancelConversationRow {
        conversation_id: u64,
        key: String,
    },
    DismissPaneRow {
        conversation_id: u64,
        key: String,
    },
    ClosePaneDetails {
        conversation_id: u64,
    },
    SyncPropagationForConversationRow {
        conversation_id: u64,
        key: String,
    },
    InspectPanePeer(u64),
    RequestPanePeerPath(u64),
    PaneDiagnostics(u64),
    TogglePaneTrust(u64),
    ToggleDeliveryMode,
    ToggleTicket,
    SendDraft,
    PrepareLatestRetry,
    SendLatestRetry,
    SelectRow(String),
    PrepareRetryForRow(String),
    SendRetryForRow(String),
    CancelRow(String),
    SyncPropagationForRow(String),
    SyncMessages,
    InspectPeer,
    RequestPeerPath,
}

#[derive(Clone, Debug)]
pub(in crate::desktop) enum ConversationCompletionMessage {
    AttachmentPicked {
        conversation_id: u64,
        result: Result<Option<PathBuf>, String>,
    },
}

#[cfg(feature = "chat-client")]
#[derive(Clone, Debug)]
pub(in crate::desktop) enum OmenChatMessage {
    NewPane,
    ServerEntryChanged(String),
    OpenServerEntry,
    ToggleRooms,
    JoinRoom {
        session_id: ChatSessionId,
        room: String,
    },
    DraftChanged {
        session_id: ChatSessionId,
        value: String,
    },
    Scrolled {
        session_id: ChatSessionId,
        room_id: RoomId,
        offset: RelativeOffset,
    },
    JumpToPresent {
        session_id: ChatSessionId,
        room_id: RoomId,
    },
    SendDraft(ChatSessionId),
    ResendLocalEcho {
        session_id: ChatSessionId,
        room_id: RoomId,
        event_id: u64,
        body: String,
        action: bool,
    },
    LoadOlderHistory(ChatSessionId),
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    CopySessionDiagnostics(ChatSessionId),
    CloseSession(ChatSessionId),
    OpenCachedMedia(String),
    LoadMedia(String),
    FetchUploadResource {
        session_id: ChatSessionId,
        resource_id: String,
    },
    PickUpload(ChatSessionId),
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    RequestPath(ChatSessionId),
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    ReconnectSession(ChatSessionId),
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    ReconnectSessionIfDisconnected(ChatSessionId),
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
#[derive(Clone, Debug)]
pub(in crate::desktop) enum OmenChatTransportCompletionMessage {
    PathRequest {
        session_id: ChatSessionId,
        destination: String,
        result: Result<bool, String>,
    },
    LiveOpen(Box<OmenChatLiveOpenCompletion>),
    LiveReconnect(Box<OmenChatLiveReconnectCompletion>),
}

#[cfg(feature = "chat-client")]
#[derive(Clone, Debug)]
pub(in crate::desktop) enum OmenChatMediaCompletionMessage {
    UploadPicked {
        session_id: ChatSessionId,
        result: Result<Option<PathBuf>, String>,
    },
    GifFramesLoaded {
        path: String,
        result: Result<super::omenchat_desktop_state::DecodedOmenChatGif, String>,
    },
    CacheCompleted(Box<OmenChatMediaCacheCompletion>),
    StaleMediaRemoved,
    MediaLoaded {
        url: String,
        result: Result<(DownloadedFile, Vec<String>), String>,
    },
}

#[derive(Clone, Debug)]
pub(in crate::desktop) enum Message {
    Shell(ShellMessage),
    Browser(BrowserMessage),
    Conversation(ConversationMessage),
    ConversationCompletion(ConversationCompletionMessage),
    #[cfg(feature = "chat-client")]
    OmenChat(OmenChatMessage),
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    OmenChatTransportCompletion(OmenChatTransportCompletionMessage),
    #[cfg(feature = "chat-client")]
    OmenChatMediaCompletion(Box<OmenChatMediaCompletionMessage>),
    WorkspacePane(WorkspacePaneMessage),
    Diagnostics(DiagnosticsMessage),
    Identity(IdentityMessage),
    Interface(InterfaceMessage),
    Directory(DirectoryMessage),
    Plugin(PluginMessage),
    Theme(ThemeMessage),
    Clearweb(ClearwebMessage),
    ExternalBrowser(ExternalBrowserMessage),
    Runtime(RuntimeMessage),
    #[cfg(test)]
    TestUnhandledRouting,
}

#[cfg(all(test, feature = "desktop-ui"))]
mod size_tests {
    use super::Message;

    #[test]
    fn asynchronous_completion_envelopes_keep_router_message_small() {
        assert!(
            std::mem::size_of::<Message>() < 128,
            "desktop Message grew to {} bytes",
            std::mem::size_of::<Message>()
        );
    }
}

#[derive(Clone, Debug)]
pub(in crate::desktop) enum ShutdownOutcome {
    Stopped,
    Failed(String),
    TimedOut,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::desktop) enum BrowserFieldKey {
    Insert(String),
    Backspace,
    Delete,
    MoveLeft,
    MoveRight,
    MoveHome,
    MoveEnd,
}
