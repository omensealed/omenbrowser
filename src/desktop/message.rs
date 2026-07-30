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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::desktop) enum OmenChatMutationResolutionAction {
    Retry,
    Abandon,
    Expire,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::desktop) enum DesktopWorkspacePreset {
    BrowserFocus,
    MessagesFocus,
    BrowserAndMessages,
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
pub(in crate::desktop) enum HistorySearchMessage {
    QueryChanged(String),
    CycleSource,
    SubmitCurrent,
    Submit(crate::history_search::LocalHistorySearchQuery),
    Jump(crate::history_search::LocalHistoryResultKey),
    Completed {
        generation: u64,
        result: Result<crate::history_search::LocalHistorySearchPage, String>,
    },
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
    CycleFallback(usize),
    CycleDirectStampLimit(usize),
    CycleDirectStampConfirmation(usize),
    CycleReplyTicketPreference(usize),
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
    CopyOperationDiagnostics(crate::operations::OperationId),
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
    ApplyPreset(DesktopWorkspacePreset),
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
    OpenCommandPalette,
    CloseCommandPalette,
    CommandPaletteQueryChanged(String),
    ExecuteFirstCommandPaletteResult,
    ExecuteCommandPalette(CommandPaletteCommand),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::desktop) enum CommandPaletteCommand {
    OpenBrowser,
    OpenMessages,
    OpenDirectory,
    OpenNetworkDoctor,
    OpenDiagnostics,
    OpenMonitoring,
    NewBrowserTab,
    RequestActiveBrowserPath,
    InspectActiveBrowserPath,
    CopyActiveIdentityHash,
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
    ConfirmPaneDirectStamp(u64),
    CancelPaneDirectStamp(u64),
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
    ConfirmInvitation,
    CancelInvitation,
    ToggleRooms,
    JoinRoom {
        session_id: ChatSessionId,
        room: String,
    },
    ToggleMuteExceptMentions {
        session_id: ChatSessionId,
        room_id: RoomId,
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
    JumpToEvent {
        session_id: ChatSessionId,
        room_id: RoomId,
        event_id: u64,
    },
    BeginReply {
        session_id: ChatSessionId,
        room_id: RoomId,
        event_id: u64,
    },
    CancelReply(ChatSessionId),
    ToggleMention {
        session_id: ChatSessionId,
        user_id: u32,
    },
    ClearMentions(ChatSessionId),
    SendDraft(ChatSessionId),
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    ToggleReaction {
        session_id: ChatSessionId,
        room_id: RoomId,
        event_id: u64,
        token: crate::chat::protocol::ReactionToken,
    },
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    TogglePin {
        session_id: ChatSessionId,
        room_id: RoomId,
        event_id: u64,
        action: crate::chat::protocol::PinAction,
    },
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    BeginMessageCorrection {
        session_id: ChatSessionId,
        room_id: RoomId,
        event_id: u64,
    },
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    MessageCorrectionChanged {
        session_id: ChatSessionId,
        value: String,
    },
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    SubmitMessageCorrection(ChatSessionId),
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    CancelMessageCorrection(ChatSessionId),
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    BeginMessageDeletion {
        session_id: ChatSessionId,
        room_id: RoomId,
        event_id: u64,
    },
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    ConfirmMessageDeletion,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    CancelMessageDeletion,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    BeginMutationResolution {
        mutation_id: crate::chat::protocol::MutationId,
        action: OmenChatMutationResolutionAction,
    },
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    ConfirmMutationResolution,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    CancelMutationResolution,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    ToggleRecoveredMutationReview(String),
    ResendLocalEcho {
        session_id: ChatSessionId,
        room_id: RoomId,
        event_id: u64,
        body: String,
        action: bool,
    },
    LoadOlderHistory(ChatSessionId),
    #[cfg(all(
        feature = "omenchat-moderation-audit",
        any(feature = "chat-client-rns", feature = "chat-client-rns-clean")
    ))]
    RefreshModerationAudit(ChatSessionId),
    #[cfg(all(
        feature = "omenchat-moderation-audit",
        any(feature = "chat-client-rns", feature = "chat-client-rns-clean")
    ))]
    LoadOlderModerationAudit(ChatSessionId),
    CopyInvitation(ChatSessionId),
    #[cfg(feature = "desktop-qr")]
    ToggleInvitationQr(ChatSessionId),
    #[cfg(feature = "desktop-qr")]
    CloseInvitationQr,
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

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
#[derive(Clone, Debug)]
pub(in crate::desktop) enum OmenChatMutationCompletionMessage {
    Recovered {
        result: Result<Vec<crate::chat::mutation_intents::OutboundMutationIntent>, String>,
    },
    Prepared {
        session_id: ChatSessionId,
        result: Result<crate::chat::mutation_intents::OutboundMutationIntent, String>,
    },
    MarkedUncertain {
        session_id: ChatSessionId,
        result: Result<crate::chat::mutation_intents::IntentTransition, String>,
    },
    Acknowledged {
        session_id: ChatSessionId,
        mutation_id: crate::chat::protocol::MutationId,
        result: Result<crate::chat::mutation_intents::IntentTransition, String>,
    },
    Terminalized {
        session_id: ChatSessionId,
        mutation_id: crate::chat::protocol::MutationId,
        next: crate::chat::mutation_intents::OutboundMutationState,
        result: Result<crate::chat::mutation_intents::IntentTransition, String>,
    },
    Rejected {
        session_id: ChatSessionId,
        mutation_id: crate::chat::protocol::MutationId,
        reason: crate::chat::DurableMutationRejectionReason,
        result: Result<crate::chat::mutation_intents::IntentRemoval, String>,
    },
    Resolved {
        mutation_id: crate::chat::protocol::MutationId,
        next: crate::chat::mutation_intents::OutboundMutationState,
        result: Result<crate::chat::mutation_intents::IntentTransition, String>,
    },
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
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    OmenChatMutationCompletion(Box<OmenChatMutationCompletionMessage>),
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
    HistorySearch(Box<HistorySearchMessage>),
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
