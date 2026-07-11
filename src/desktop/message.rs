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
pub(in crate::desktop) enum Message {
    SwitchSection(WorkspaceSection),
    SelectBrowserTab(usize),
    NewBrowserTab,
    CloseBrowserTab,
    CloseBrowserPaneTab(TabId),
    NewConversationPane,
    CloseConversationPaneTab(u64),
    #[cfg(feature = "chat-client")]
    NewOmenChatPane,
    #[cfg(feature = "chat-client")]
    OmenChatServerEntryChanged(String),
    #[cfg(feature = "chat-client")]
    OpenOmenChatServerEntry,
    #[cfg(feature = "chat-client")]
    ToggleOmenChatRooms,
    #[cfg(feature = "chat-client")]
    JoinOmenChatRoom {
        session_id: ChatSessionId,
        room: String,
    },
    #[cfg(feature = "chat-client")]
    OmenChatDraftChanged {
        session_id: ChatSessionId,
        value: String,
    },
    #[cfg(feature = "chat-client")]
    OmenChatScrolled {
        session_id: ChatSessionId,
        room_id: RoomId,
        offset: RelativeOffset,
    },
    #[cfg(feature = "chat-client")]
    JumpOmenChatToPresent {
        session_id: ChatSessionId,
        room_id: RoomId,
    },
    #[cfg(feature = "chat-client")]
    SendOmenChatDraft(ChatSessionId),
    #[cfg(feature = "chat-client")]
    ResendOmenChatLocalEcho {
        session_id: ChatSessionId,
        room_id: RoomId,
        event_id: u64,
        body: String,
        action: bool,
    },
    #[cfg(feature = "chat-client")]
    LoadOlderOmenChatHistory(ChatSessionId),
    #[cfg(feature = "chat-client")]
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    RequestOmenChatPath(ChatSessionId),
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    ReconnectOmenChatSession(ChatSessionId),
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    ReconnectOmenChatSessionIfDisconnected(ChatSessionId),
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    OmenChatPathRequestResult {
        session_id: ChatSessionId,
        destination: String,
        result: Result<bool, String>,
    },
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    OmenChatLiveOpenResult {
        descriptor: OmenChatDescriptor,
        result: Result<crate::runtime::OmenChatLinkOpened, String>,
    },
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    OmenChatLiveReconnectResult {
        session_id: ChatSessionId,
        generation: u64,
        descriptor: OmenChatDescriptor,
        result: Result<crate::runtime::OmenChatLinkOpened, String>,
    },
    AddressChanged(String),
    OpenAddress,
    BrowserPaneAddressChanged {
        tab_id: TabId,
        value: String,
    },
    OpenBrowserPaneAddress(TabId),
    ReloadBrowserPane(TabId),
    BrowserPaneBack(TabId),
    BrowserPaneForward(TabId),
    BrowserPaneTop(TabId),
    StopBrowserPaneTask(TabId),
    InlineProbeBrowserPane(TabId),
    LiveProbeBrowserPane(TabId),
    WarmBrowserPanePath(TabId),
    RetryBrowserPaneAfterPath(TabId),
    BrowserPanePathDiagnostics(TabId),
    CaptureBrowserPaneRender(TabId),
    DismissBrowserPaneWarning(TabId),
    DismissBrowserPaneRequest(TabId),
    ToggleBrowserPaneIdentify(TabId),
    OpenSetupAddress,
    ReloadBrowser,
    BrowserBack,
    BrowserForward,
    StopBrowserTask,
    InlineProbe,
    LiveProbe,
    WarmPath,
    RetryAfterPath,
    PathDiagnostics,
    CaptureBrowserRender,
    ShowDiagnostics,
    PreviewManagedConfig,
    ExportManagedConfig,
    PreviewDiagnosticsBundle,
    ExportDiagnosticsBundle,
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
    CreateIdentity,
    ActivateManagedIdentity(String),
    ActiveIdentityLabelChanged(String),
    DeleteActiveIdentity,
    ConfirmDeleteActiveIdentity,
    CancelDeleteActiveIdentity,
    ClearActiveIdentity,
    AnnounceIdentityNow,
    CreateTcpClientInterface,
    CreateI2pInterface,
    CreateRNodeInterface,
    CreateGatewayPreset(String),
    SwitchConversation(usize),
    ConversationScrolled {
        conversation_id: u64,
        offset: RelativeOffset,
    },
    JumpConversationToPresent(u64),
    ConversationTitleChanged(String),
    ConversationBodyChanged(String),
    ConversationPanePeerChanged {
        conversation_id: u64,
        value: String,
    },
    ConversationPaneTitleChanged {
        conversation_id: u64,
        value: String,
    },
    ConversationPaneBodyChanged {
        conversation_id: u64,
        value: String,
    },
    ConversationPaneBodyEdited {
        conversation_id: u64,
        action: text_editor::Action,
    },
    PickConversationAttachment(u64),
    ConversationAttachmentPicked {
        conversation_id: u64,
        result: Result<Option<PathBuf>, String>,
    },
    RemoveConversationAttachment {
        conversation_id: u64,
        index: usize,
    },
    OpenConversationAttachment(PathBuf),
    ToggleConversationPaneDeliveryMode(u64),
    ToggleConversationPaneTicket(u64),
    SendConversationPaneDraft(u64),
    PrepareLatestLxmfRetryForConversation(u64),
    SendLatestLxmfRetryForConversation(u64),
    SelectConversationPaneRow {
        conversation_id: u64,
        key: String,
    },
    PrepareLxmfRetryForConversationRow {
        conversation_id: u64,
        key: String,
    },
    SendLxmfRetryForConversationRow {
        conversation_id: u64,
        key: String,
    },
    DismissConversationPaneRow {
        conversation_id: u64,
        key: String,
    },
    CloseConversationPaneDetails {
        conversation_id: u64,
    },
    SyncPropagationForConversationRow {
        conversation_id: u64,
        key: String,
    },
    InspectConversationPanePeer(u64),
    RequestConversationPanePeerPath(u64),
    ConversationPaneDiagnostics(u64),
    ToggleConversationPaneTrust(u64),
    ToggleConversationDeliveryMode,
    ToggleConversationTicket,
    SendConversationDraft,
    PrepareLatestLxmfRetry,
    SendLatestLxmfRetry,
    SelectConversationRow(String),
    PrepareLxmfRetryForRow(String),
    SendLxmfRetryForRow(String),
    SyncPropagationForRow(String),
    SyncMessages,
    InspectLxmfPeer,
    RequestLxmfPeerPath,
    SwitchDirectoryKind(crate::directory::DirectoryKind),
    SwitchDirectoryScope(DirectoryScope),
    DirectoryFilterChanged(String),
    SelectDirectoryEntry(usize),
    OpenDirectoryEntry(usize),
    OpenPeerChat(usize),
    #[cfg(feature = "chat-client")]
    OpenDirectoryOmenChat(usize),
    InspectDirectoryPeer(usize),
    SaveDirectoryEntry(usize),
    ToggleDirectoryTrust(usize),
    ToggleDirectoryIdentify(usize),
    CycleDirectoryDelivery(usize),
    RequestDirectoryPath(usize),
    UseDirectoryPropagation(usize),
    ClearDirectoryPropagation,
    SelectInterfaceProfile(usize),
    ToggleInterfaceEnabled(usize),
    DeleteInterfaceProfile(usize),
    ConfirmInterfaceDelete,
    CancelInterfaceDelete,
    SelectPlugin(usize),
    TogglePlugin(usize),
    BeginPluginRemove(usize),
    ToggleSelectedPlugin,
    BeginPluginInstall,
    BeginSelectedPluginRemove,
    RefreshPlugins,
    ShowPluginLogs,
    InterfaceNameChanged {
        profile_id: String,
        value: String,
    },
    TcpClientHostChanged {
        profile_id: String,
        value: String,
    },
    TcpClientPortChanged {
        profile_id: String,
        value: String,
    },
    TcpClientIfacNetworkChanged {
        profile_id: String,
        value: String,
    },
    TcpClientIfacPassphraseChanged {
        profile_id: String,
        value: String,
    },
    TcpServerHostChanged {
        profile_id: String,
        value: String,
    },
    TcpServerPortChanged {
        profile_id: String,
        value: String,
    },
    TcpServerIfacNetworkChanged {
        profile_id: String,
        value: String,
    },
    TcpServerIfacPassphraseChanged {
        profile_id: String,
        value: String,
    },
    ToggleI2pConnectable(usize),
    I2pPeersChanged {
        profile_id: String,
        value: String,
    },
    RNodeDevicePortChanged {
        profile_id: String,
        value: String,
    },
    RNodeFrequencyChanged {
        profile_id: String,
        value: String,
    },
    RNodeBandwidthChanged {
        profile_id: String,
        value: String,
    },
    RNodeTxPowerChanged {
        profile_id: String,
        value: String,
    },
    RNodeSpreadingFactorChanged {
        profile_id: String,
        value: String,
    },
    RNodeCodingRateChanged {
        profile_id: String,
        value: String,
    },
    SetTheme(String),
    SetFontSize(u16),
    SelectPreferredExternalBrowser(usize),
    ClearPreferredExternalBrowser,
    ToggleClearwebSocksProxy,
    ToggleClearwebRemoteMedia,
    ToggleAutoSyncAfterPropagationAccept,
    SelectNativeBackend,
    StartNativeRuntime,
    NativeQuickstart,
    InterfaceStatsSampled(Result<crate::runtime::InterfaceStats, String>),
    ToggleNavigation,
    BrowserFieldKey(BrowserFieldKey),
    SubmitBrowserFieldDraft,
    CancelBrowserFieldDraft,
    FocusBrowserItem {
        reverse: bool,
    },
    ActivateFocusedBrowserItem,
    BrowserZoom {
        direction: isize,
    },
    ScrollBrowserPage {
        direction: isize,
    },
    Tick,
    Page(PageMessage),
    PageForTab {
        tab_id: TabId,
        page: PageMessage,
    },
    WorkspacePaneClicked(pane_grid::Pane),
    WorkspacePaneDragged(pane_grid::DragEvent),
    WorkspacePaneResized(pane_grid::ResizeEvent),
    WorkspacePaneMaximize(pane_grid::Pane),
    WorkspacePaneRestore,
    WorkspacePaneClose(pane_grid::Pane),
    RestoreDesktopPane(DesktopPane),
    #[cfg(feature = "chat-client")]
    CloseOmenChatSession(ChatSessionId),
    WindowCloseRequested(window::Id),
    WindowShutdownComplete(window::Id),
    KeyboardModifiersChanged(keyboard::Modifiers),
    OpenExternalLinkWith(usize),
    CopyExternalLinkUrl,
    CopyActiveIdentityHash,
    PromptExternalUrl(String),
    #[cfg(feature = "chat-client")]
    OpenCachedOmenChatMedia(String),
    #[cfg(feature = "chat-client")]
    LoadOmenChatMedia(String),
    #[cfg(feature = "chat-client")]
    FetchOmenChatUploadResource {
        session_id: ChatSessionId,
        resource_id: String,
    },
    #[cfg(feature = "chat-client")]
    PickOmenChatUpload(ChatSessionId),
    #[cfg(feature = "chat-client")]
    OmenChatUploadPicked {
        session_id: ChatSessionId,
        result: Result<Option<PathBuf>, String>,
    },
    #[cfg(feature = "chat-client")]
    OmenChatGifFramesLoaded {
        path: String,
        result: Result<Vec<u8>, String>,
    },
    #[cfg(feature = "chat-client")]
    OmenChatMediaLoaded {
        url: String,
        result: Result<DownloadedFile, String>,
    },
    DismissExternalLinkPrompt,
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
