use iced::Task;

use super::{DesktopApp, Message, ShellMessage};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MessageRoute {
    Browser,
    Conversation,
    Directory,
    Diagnostics,
    Identity,
    Theme,
    Clearweb,
    ExternalBrowser,
    Runtime,
    HistorySearch,
    Shell,
    Interface,
    Plugin,
    #[cfg(feature = "chat-client")]
    OmenChat,
    WorkspacePane,
    #[cfg(test)]
    TestUnhandled,
}

impl Message {
    /// Assign every top-level message to exactly one subsystem.
    ///
    /// This match intentionally has no wildcard: adding a production message
    /// is a compile error until its ownership boundary is explicit here.
    fn route(&self) -> MessageRoute {
        match self {
            Message::Browser(_) => MessageRoute::Browser,

            Message::Conversation(_) | Message::ConversationCompletion(_) => {
                MessageRoute::Conversation
            }

            Message::Directory(_) => MessageRoute::Directory,

            Message::Diagnostics(_) => MessageRoute::Diagnostics,

            Message::Identity(_) => MessageRoute::Identity,

            Message::Theme(_) => MessageRoute::Theme,

            Message::Clearweb(_) => MessageRoute::Clearweb,

            Message::ExternalBrowser(_) => MessageRoute::ExternalBrowser,

            Message::Runtime(_) => MessageRoute::Runtime,

            Message::HistorySearch(_) => MessageRoute::HistorySearch,

            Message::Shell(_) => MessageRoute::Shell,

            Message::Interface(_) => MessageRoute::Interface,

            Message::Plugin(_) => MessageRoute::Plugin,

            #[cfg(feature = "chat-client")]
            Message::OmenChat(_) | Message::OmenChatMediaCompletion(_) => MessageRoute::OmenChat,
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            Message::OmenChatTransportCompletion(_) | Message::OmenChatMutationCompletion(_) => {
                MessageRoute::OmenChat
            }

            Message::WorkspacePane(_) => MessageRoute::WorkspacePane,

            #[cfg(test)]
            Message::TestUnhandledRouting => MessageRoute::TestUnhandled,
        }
    }
}

impl DesktopApp {
    pub(super) fn update(&mut self, message: Message) -> Task<Message> {
        if !self.ui.shutdown_phase.is_running()
            && !matches!(
                &message,
                Message::Shell(
                    ShellMessage::WindowCloseRequested(_)
                        | ShellMessage::WindowShutdownBegin(_)
                        | ShellMessage::WindowShutdownComplete { .. }
                )
            )
        {
            return Task::none();
        }
        let route = message.route();
        let result = match route {
            MessageRoute::Browser => self.dispatch_browser_message(message),
            MessageRoute::Conversation => self.dispatch_conversation_message(message).map(|task| {
                Task::batch([task, self.snap_conversations_with_new_messages_to_bottom()])
            }),
            MessageRoute::Directory => self.dispatch_directory_message(message),
            MessageRoute::Diagnostics => self.dispatch_diagnostics_message(message),
            MessageRoute::Identity => self.dispatch_identity_message(message),
            MessageRoute::Theme => self.dispatch_theme_message(message),
            MessageRoute::Clearweb => self.dispatch_clearweb_message(message),
            MessageRoute::ExternalBrowser => self.dispatch_external_browser_message(message),
            MessageRoute::Runtime => self.dispatch_runtime_message(message),
            MessageRoute::HistorySearch => self.dispatch_history_search_message(message),
            MessageRoute::Shell => self.dispatch_shell_message(message),
            MessageRoute::Interface => self.dispatch_interface_message(message),
            MessageRoute::Plugin => self.dispatch_plugin_message(message),
            #[cfg(feature = "chat-client")]
            MessageRoute::OmenChat => self.dispatch_omenchat_message(message).map(|task| {
                Task::batch([
                    task,
                    self.snap_omenchat_with_new_events_to_bottom(),
                    self.drain_omenchat_media_cache_tasks(),
                ])
            }),
            MessageRoute::WorkspacePane => self.dispatch_workspace_pane_message(message),
            #[cfg(test)]
            MessageRoute::TestUnhandled => Err(message),
        };
        let message = match result {
            Ok(task) => return task,
            Err(message) => message,
        };
        let discriminant = std::mem::discriminant(&message);
        let diagnostic = format!(
            "unhandled desktop message discriminant {discriminant:?}; add a dispatch route"
        );
        tracing::error!(?discriminant, "desktop message routing failure");
        self.app.record_desktop_routing_error(diagnostic);
        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, LogSeverity, LogSource};
    use crate::desktop::{
        BrowserMessage, ClearwebMessage, ConversationCompletionMessage, ConversationMessage,
        DiagnosticsMessage, DirectoryMessage, ExternalBrowserMessage, HistorySearchMessage,
        IdentityMessage, InterfaceMessage, PluginMessage, RuntimeMessage, ThemeMessage,
        WorkspacePaneMessage,
    };
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    use crate::desktop::{
        OmenChatLiveOpenCompletion, OmenChatLiveReconnectCompletion,
        OmenChatTransportCompletionMessage,
    };
    #[cfg(feature = "chat-client")]
    use crate::desktop::{
        OmenChatMediaCacheCompletion, OmenChatMediaCompletionMessage, OmenChatMessage,
    };

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_media_completion_messages_have_one_compile_time_route() {
        let session_id = 1;
        for completion in [
            OmenChatMediaCompletionMessage::UploadPicked {
                session_id,
                result: Ok(None),
            },
            OmenChatMediaCompletionMessage::GifFramesLoaded {
                path: "animated.gif".into(),
                result: Err("not decoded".into()),
            },
            OmenChatMediaCompletionMessage::CacheCompleted(Box::new(
                OmenChatMediaCacheCompletion {
                    session_id,
                    cache_key: "cache-key".into(),
                    generation: 1,
                    result: Err("not cached".into()),
                },
            )),
            OmenChatMediaCompletionMessage::StaleMediaRemoved,
            OmenChatMediaCompletionMessage::MediaLoaded {
                url: "https://example.invalid/media.png".into(),
                result: Err("not loaded".into()),
            },
        ] {
            assert_eq!(
                Message::OmenChatMediaCompletion(Box::new(completion)).route(),
                MessageRoute::OmenChat
            );
        }
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    #[test]
    fn omenchat_transport_completion_messages_have_one_compile_time_route() {
        let session_id = 1;
        let descriptor = || crate::chat::OmenChatDescriptor {
            server_destination: "server".into(),
            ..crate::chat::OmenChatDescriptor::default()
        };
        for completion in [
            OmenChatTransportCompletionMessage::PathRequest {
                session_id,
                destination: "server".into(),
                result: Ok(true),
            },
            OmenChatTransportCompletionMessage::LiveOpen(Box::new(OmenChatLiveOpenCompletion {
                descriptor: descriptor(),
                result: Err("not opened".into()),
            })),
            OmenChatTransportCompletionMessage::LiveReconnect(Box::new(
                OmenChatLiveReconnectCompletion {
                    session_id,
                    generation: 1,
                    descriptor: descriptor(),
                    result: Err("not reconnected".into()),
                },
            )),
        ] {
            assert_eq!(
                Message::OmenChatTransportCompletion(completion).route(),
                MessageRoute::OmenChat
            );
        }
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_domain_messages_have_one_compile_time_route() {
        let session_id = 1;
        let room_id = 2;
        for message in [
            OmenChatMessage::NewPane,
            OmenChatMessage::ServerEntryChanged("omenchat://server".into()),
            OmenChatMessage::OpenServerEntry,
            OmenChatMessage::ConfirmInvitation,
            OmenChatMessage::CancelInvitation,
            OmenChatMessage::ToggleRooms,
            OmenChatMessage::JoinRoom {
                session_id,
                room: "lobby".into(),
            },
            OmenChatMessage::ToggleMuteExceptMentions {
                session_id,
                room_id,
            },
            OmenChatMessage::DraftChanged {
                session_id,
                value: "message".into(),
            },
            OmenChatMessage::Scrolled {
                session_id,
                room_id,
                offset: iced::widget::scrollable::RelativeOffset { x: 0.0, y: 1.0 },
            },
            OmenChatMessage::JumpToPresent {
                session_id,
                room_id,
            },
            OmenChatMessage::JumpToEvent {
                session_id,
                room_id,
                event_id: 3,
            },
            OmenChatMessage::BeginReply {
                session_id,
                room_id,
                event_id: 3,
            },
            OmenChatMessage::CancelReply(session_id),
            OmenChatMessage::ToggleMention {
                session_id,
                user_id: 4,
            },
            OmenChatMessage::ClearMentions(session_id),
            OmenChatMessage::SendDraft(session_id),
            OmenChatMessage::ResendLocalEcho {
                session_id,
                room_id,
                event_id: 3,
                body: "retry".into(),
                action: false,
            },
            OmenChatMessage::LoadOlderHistory(session_id),
            OmenChatMessage::CopyInvitation(session_id),
            #[cfg(feature = "desktop-qr")]
            OmenChatMessage::ToggleInvitationQr(session_id),
            #[cfg(feature = "desktop-qr")]
            OmenChatMessage::CloseInvitationQr,
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            OmenChatMessage::CopySessionDiagnostics(session_id),
            OmenChatMessage::CloseSession(session_id),
            OmenChatMessage::OpenCachedMedia("cached.png".into()),
            OmenChatMessage::LoadMedia("https://example.invalid/media.png".into()),
            OmenChatMessage::FetchUploadResource {
                session_id,
                resource_id: "resource".into(),
            },
            OmenChatMessage::PickUpload(session_id),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            OmenChatMessage::RequestPath(session_id),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            OmenChatMessage::ReconnectSession(session_id),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            OmenChatMessage::ReconnectSessionIfDisconnected(session_id),
        ] {
            assert_eq!(Message::OmenChat(message).route(), MessageRoute::OmenChat);
        }
    }

    #[test]
    fn conversation_domain_messages_have_one_compile_time_route() {
        let conversation_id = 1;
        let row = || (conversation_id, "message-key".to_string());
        let (select_id, select_key) = row();
        let (prepare_id, prepare_key) = row();
        let (send_id, send_key) = row();
        let (cancel_id, cancel_key) = row();
        let (dismiss_id, dismiss_key) = row();
        let (sync_id, sync_key) = row();
        for message in [
            ConversationMessage::Switch(0),
            ConversationMessage::Scrolled {
                conversation_id,
                offset: iced::widget::scrollable::RelativeOffset { x: 0.0, y: 1.0 },
            },
            ConversationMessage::JumpToPresent(conversation_id),
            ConversationMessage::TitleChanged("subject".into()),
            ConversationMessage::BodyChanged("body".into()),
            ConversationMessage::PanePeerChanged {
                conversation_id,
                value: "peer".into(),
            },
            ConversationMessage::PaneTitleChanged {
                conversation_id,
                value: "subject".into(),
            },
            ConversationMessage::PaneBodyChanged {
                conversation_id,
                value: "body".into(),
            },
            ConversationMessage::PaneBodyEdited {
                conversation_id,
                action: iced::widget::text_editor::Action::SelectAll,
            },
            ConversationMessage::PickAttachment(conversation_id),
            ConversationMessage::RemoveAttachment {
                conversation_id,
                index: 0,
            },
            ConversationMessage::OpenAttachment(std::path::PathBuf::from("attachment")),
            ConversationMessage::TogglePaneDeliveryMode(conversation_id),
            ConversationMessage::TogglePaneTicket(conversation_id),
            ConversationMessage::SendPaneDraft(conversation_id),
            ConversationMessage::PrepareLatestRetryForConversation(conversation_id),
            ConversationMessage::SendLatestRetryForConversation(conversation_id),
            ConversationMessage::SelectPaneRow {
                conversation_id: select_id,
                key: select_key,
            },
            ConversationMessage::PrepareRetryForConversationRow {
                conversation_id: prepare_id,
                key: prepare_key,
            },
            ConversationMessage::SendRetryForConversationRow {
                conversation_id: send_id,
                key: send_key,
            },
            ConversationMessage::CancelConversationRow {
                conversation_id: cancel_id,
                key: cancel_key,
            },
            ConversationMessage::DismissPaneRow {
                conversation_id: dismiss_id,
                key: dismiss_key,
            },
            ConversationMessage::ClosePaneDetails { conversation_id },
            ConversationMessage::SyncPropagationForConversationRow {
                conversation_id: sync_id,
                key: sync_key,
            },
            ConversationMessage::InspectPanePeer(conversation_id),
            ConversationMessage::RequestPanePeerPath(conversation_id),
            ConversationMessage::PaneDiagnostics(conversation_id),
            ConversationMessage::TogglePaneTrust(conversation_id),
            ConversationMessage::ToggleDeliveryMode,
            ConversationMessage::ToggleTicket,
            ConversationMessage::SendDraft,
            ConversationMessage::PrepareLatestRetry,
            ConversationMessage::SendLatestRetry,
            ConversationMessage::SelectRow("message-key".into()),
            ConversationMessage::PrepareRetryForRow("message-key".into()),
            ConversationMessage::SendRetryForRow("message-key".into()),
            ConversationMessage::CancelRow("message-key".into()),
            ConversationMessage::SyncPropagationForRow("message-key".into()),
            ConversationMessage::SyncMessages,
            ConversationMessage::InspectPeer,
            ConversationMessage::RequestPeerPath,
        ] {
            assert_eq!(
                Message::Conversation(message).route(),
                MessageRoute::Conversation
            );
        }
    }

    #[test]
    fn conversation_completion_messages_have_one_compile_time_route() {
        let conversation_id = 1;
        assert_eq!(
            Message::ConversationCompletion(ConversationCompletionMessage::AttachmentPicked {
                conversation_id,
                result: Ok(None),
            })
            .route(),
            MessageRoute::Conversation
        );
    }

    #[test]
    fn browser_domain_messages_have_one_compile_time_route() {
        let tab_id = 1;
        let page = || crate::desktop::page_widget::PageMessage::Scroll {
            delta: 1,
            width: 80,
            height: 24,
        };
        for message in [
            BrowserMessage::SelectTab(0),
            BrowserMessage::NewTab,
            BrowserMessage::CloseTab,
            BrowserMessage::ClosePaneTab(tab_id),
            BrowserMessage::AddressChanged("mock.node:/".into()),
            BrowserMessage::OpenAddress,
            BrowserMessage::PaneAddressChanged {
                tab_id,
                value: "mock.node:/page.mu".into(),
            },
            BrowserMessage::OpenPaneAddress(tab_id),
            BrowserMessage::ReloadPane(tab_id),
            BrowserMessage::PaneBack(tab_id),
            BrowserMessage::PaneForward(tab_id),
            BrowserMessage::PaneTop(tab_id),
            BrowserMessage::StopPaneTask(tab_id),
            BrowserMessage::InlineProbePane(tab_id),
            BrowserMessage::LiveProbePane(tab_id),
            BrowserMessage::WarmPanePath(tab_id),
            BrowserMessage::RetryPaneAfterPath(tab_id),
            BrowserMessage::PanePathDiagnostics(tab_id),
            BrowserMessage::CapturePaneRender(tab_id),
            BrowserMessage::DismissPaneWarning(tab_id),
            BrowserMessage::DismissPaneRequest(tab_id),
            BrowserMessage::TogglePaneIdentify(tab_id),
            BrowserMessage::OpenSetupAddress,
            BrowserMessage::Reload,
            BrowserMessage::Back,
            BrowserMessage::Forward,
            BrowserMessage::StopTask,
            BrowserMessage::InlineProbe,
            BrowserMessage::LiveProbe,
            BrowserMessage::WarmPath,
            BrowserMessage::RetryAfterPath,
            BrowserMessage::PathDiagnostics,
            BrowserMessage::CaptureRender,
            BrowserMessage::FieldKey(crate::desktop::BrowserFieldKey::Backspace),
            BrowserMessage::SubmitFieldDraft,
            BrowserMessage::CancelFieldDraft,
            BrowserMessage::FocusItem { reverse: false },
            BrowserMessage::ActivateFocusedItem,
            BrowserMessage::Zoom { direction: 1 },
            BrowserMessage::ScrollPage { direction: 1 },
            BrowserMessage::Page(page()),
            BrowserMessage::PageForTab {
                tab_id,
                page: page(),
            },
        ] {
            assert_eq!(Message::Browser(message).route(), MessageRoute::Browser);
        }
    }

    #[test]
    fn shell_domain_messages_have_one_compile_time_route() {
        let window_id = iced::window::Id::unique();
        for message in [
            ShellMessage::SwitchSection(crate::workspace::WorkspaceSection::Settings),
            ShellMessage::ToggleNavigation,
            ShellMessage::WorkspaceScrollTick,
            ShellMessage::InternalEventsReady,
            ShellMessage::PersistenceDeadlineReached,
            ShellMessage::MonitoringTick,
            ShellMessage::LxmfReconcileDeadlineReached,
            ShellMessage::BrowserPartialDeadlineReached,
            ShellMessage::OmenChatMaintenanceDeadlineReached,
            ShellMessage::WindowCloseRequested(window_id),
            ShellMessage::WindowShutdownBegin(window_id),
            ShellMessage::WindowShutdownComplete {
                window_id,
                outcome: crate::desktop::ShutdownOutcome::Stopped,
            },
            ShellMessage::KeyboardModifiersChanged(iced::keyboard::Modifiers::empty()),
        ] {
            assert_eq!(Message::Shell(message).route(), MessageRoute::Shell);
        }
    }

    #[test]
    fn shutdown_gate_rejects_non_lifecycle_shell_messages() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-desktop-shell-shutdown-gate-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root.clone());
        paths.ensure().expect("isolated paths");
        let mut desktop = DesktopApp::new(App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        }));
        assert!(desktop.ui.shutdown_phase.request());
        let original_section = desktop.app.workspace.active_section;

        let _ = desktop.update(Message::Shell(ShellMessage::SwitchSection(
            crate::workspace::WorkspaceSection::Settings,
        )));

        assert_eq!(desktop.app.workspace.active_section, original_section);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn workspace_pane_domain_messages_have_one_compile_time_route() {
        let (mut panes, pane) = iced::widget::pane_grid::State::new(());
        let (_, split) = panes
            .split(iced::widget::pane_grid::Axis::Vertical, pane, ())
            .expect("pane split");

        for message in [
            WorkspacePaneMessage::NewConversation,
            WorkspacePaneMessage::CloseConversationTab(1),
            WorkspacePaneMessage::ApplyPreset(
                crate::desktop::DesktopWorkspacePreset::BrowserAndMessages,
            ),
            WorkspacePaneMessage::Clicked(pane),
            WorkspacePaneMessage::Dragged(iced::widget::pane_grid::DragEvent::Picked { pane }),
            WorkspacePaneMessage::Resized(iced::widget::pane_grid::ResizeEvent {
                split,
                ratio: 0.5,
            }),
            WorkspacePaneMessage::Maximize(pane),
            WorkspacePaneMessage::Restore,
            WorkspacePaneMessage::Close(pane),
            WorkspacePaneMessage::RestoreDesktop(crate::desktop::DesktopPane::Browser(1)),
        ] {
            assert_eq!(
                Message::WorkspacePane(message).route(),
                MessageRoute::WorkspacePane
            );
        }
    }

    #[test]
    fn diagnostics_domain_messages_have_one_compile_time_route() {
        for message in [
            DiagnosticsMessage::Show,
            DiagnosticsMessage::CopyOperationDiagnostics(crate::operations::OperationId::numeric(
                crate::operations::OperationDomain::PathDiscovery,
                1,
            )),
            DiagnosticsMessage::PreviewManagedConfig,
            DiagnosticsMessage::ExportManagedConfig,
            DiagnosticsMessage::PreviewBundle,
            DiagnosticsMessage::ExportBundle,
            DiagnosticsMessage::PreviewLiveInteropReport,
            DiagnosticsMessage::ExportLiveInteropReport,
            DiagnosticsMessage::NativePreflight,
            DiagnosticsMessage::NativeSmokeDryRun,
            DiagnosticsMessage::NativeSmokeLiveProbe,
            DiagnosticsMessage::NativeLiveFetchValidate,
            DiagnosticsMessage::NativeLxmfSmokeSend,
            DiagnosticsMessage::NativeLxmfInterop,
            DiagnosticsMessage::NativeLxmfPropagationDiagnostics,
            DiagnosticsMessage::SyncPropagationNow,
            DiagnosticsMessage::BeginKnownDestinationsPreload,
        ] {
            assert_eq!(
                Message::Diagnostics(message).route(),
                MessageRoute::Diagnostics
            );
        }
    }

    #[test]
    fn interface_domain_messages_have_one_compile_time_route() {
        let field = || ("profile-id".to_string(), "value".to_string());
        let (name_id, name_value) = field();
        let (client_host_id, client_host_value) = field();
        let (client_port_id, client_port_value) = field();
        let (client_network_id, client_network_value) = field();
        let (client_pass_id, client_pass_value) = field();
        let (server_host_id, server_host_value) = field();
        let (server_port_id, server_port_value) = field();
        let (server_network_id, server_network_value) = field();
        let (server_pass_id, server_pass_value) = field();
        let (i2p_id, i2p_value) = field();
        let (device_id, device_value) = field();
        let (frequency_id, frequency_value) = field();
        let (bandwidth_id, bandwidth_value) = field();
        let (power_id, power_value) = field();
        let (spreading_id, spreading_value) = field();
        let (coding_id, coding_value) = field();

        for message in [
            InterfaceMessage::CreateTcpClient,
            InterfaceMessage::CreateI2p,
            InterfaceMessage::CreateRNode,
            InterfaceMessage::CreateGatewayPreset("gateway".into()),
            InterfaceMessage::SelectProfile(0),
            InterfaceMessage::ToggleEnabled(0),
            InterfaceMessage::DeleteProfile(0),
            InterfaceMessage::ConfirmDelete,
            InterfaceMessage::CancelDelete,
            InterfaceMessage::NameChanged {
                profile_id: name_id,
                value: name_value,
            },
            InterfaceMessage::TcpClientHostChanged {
                profile_id: client_host_id,
                value: client_host_value,
            },
            InterfaceMessage::TcpClientPortChanged {
                profile_id: client_port_id,
                value: client_port_value,
            },
            InterfaceMessage::TcpClientIfacNetworkChanged {
                profile_id: client_network_id,
                value: client_network_value,
            },
            InterfaceMessage::TcpClientIfacPassphraseChanged {
                profile_id: client_pass_id,
                value: client_pass_value,
            },
            InterfaceMessage::TcpServerHostChanged {
                profile_id: server_host_id,
                value: server_host_value,
            },
            InterfaceMessage::TcpServerPortChanged {
                profile_id: server_port_id,
                value: server_port_value,
            },
            InterfaceMessage::TcpServerIfacNetworkChanged {
                profile_id: server_network_id,
                value: server_network_value,
            },
            InterfaceMessage::TcpServerIfacPassphraseChanged {
                profile_id: server_pass_id,
                value: server_pass_value,
            },
            InterfaceMessage::ToggleI2pConnectable(0),
            InterfaceMessage::I2pPeersChanged {
                profile_id: i2p_id,
                value: i2p_value,
            },
            InterfaceMessage::RNodeDevicePortChanged {
                profile_id: device_id,
                value: device_value,
            },
            InterfaceMessage::RNodeFrequencyChanged {
                profile_id: frequency_id,
                value: frequency_value,
            },
            InterfaceMessage::RNodeBandwidthChanged {
                profile_id: bandwidth_id,
                value: bandwidth_value,
            },
            InterfaceMessage::RNodeTxPowerChanged {
                profile_id: power_id,
                value: power_value,
            },
            InterfaceMessage::RNodeSpreadingFactorChanged {
                profile_id: spreading_id,
                value: spreading_value,
            },
            InterfaceMessage::RNodeCodingRateChanged {
                profile_id: coding_id,
                value: coding_value,
            },
        ] {
            assert_eq!(Message::Interface(message).route(), MessageRoute::Interface);
        }
    }

    #[test]
    fn directory_domain_messages_have_one_compile_time_route() {
        for message in [
            DirectoryMessage::SwitchKind(crate::directory::DirectoryKind::Node),
            DirectoryMessage::SwitchScope(crate::app::DirectoryScope::Live),
            DirectoryMessage::FilterChanged("peer".into()),
            DirectoryMessage::SelectEntry(0),
            DirectoryMessage::OpenEntry(0),
            DirectoryMessage::OpenPeerChat(0),
            DirectoryMessage::InspectPeer(0),
            DirectoryMessage::SaveEntry(0),
            DirectoryMessage::ToggleTrust(0),
            DirectoryMessage::ToggleIdentify(0),
            DirectoryMessage::CycleDelivery(0),
            DirectoryMessage::CycleFallback(0),
            DirectoryMessage::CycleDirectStampLimit(0),
            DirectoryMessage::CycleDirectStampConfirmation(0),
            DirectoryMessage::CycleReplyTicketPreference(0),
            DirectoryMessage::RequestPath(0),
            DirectoryMessage::RefreshPropagation(0),
            DirectoryMessage::CancelPropagationRefresh,
            DirectoryMessage::UsePropagation(0),
            DirectoryMessage::ClearPropagation,
        ] {
            assert_eq!(Message::Directory(message).route(), MessageRoute::Directory);
        }

        #[cfg(feature = "chat-client")]
        assert_eq!(
            Message::Directory(DirectoryMessage::OpenOmenChat(0)).route(),
            MessageRoute::Directory
        );
    }

    #[test]
    fn plugin_domain_messages_have_one_compile_time_route() {
        for message in [
            PluginMessage::Select(0),
            PluginMessage::Toggle(0),
            PluginMessage::BeginRemove(0),
            PluginMessage::ToggleSelected,
            PluginMessage::BeginInstall,
            PluginMessage::BeginSelectedRemove,
            PluginMessage::Refresh,
            PluginMessage::ShowLogs,
        ] {
            assert_eq!(Message::Plugin(message).route(), MessageRoute::Plugin);
        }
    }

    #[test]
    fn identity_domain_messages_have_one_compile_time_route() {
        for message in [
            IdentityMessage::Create,
            IdentityMessage::ActivateManaged("identity/path".into()),
            IdentityMessage::ActiveLabelChanged("label".into()),
            IdentityMessage::DeleteActive,
            IdentityMessage::ConfirmDeleteActive,
            IdentityMessage::CancelDeleteActive,
            IdentityMessage::ClearActive,
            IdentityMessage::AnnounceNow,
            IdentityMessage::CopyActiveHash,
        ] {
            assert_eq!(Message::Identity(message).route(), MessageRoute::Identity);
        }
    }

    #[test]
    fn runtime_domain_messages_have_one_compile_time_route() {
        for message in [
            RuntimeMessage::ToggleAutoSyncAfterPropagationAccept,
            RuntimeMessage::SelectNativeBackend,
            RuntimeMessage::StartNativeRuntime,
            RuntimeMessage::NativeQuickstart,
            RuntimeMessage::InterfaceStatsSampled(Err("unavailable".into())),
        ] {
            assert_eq!(Message::Runtime(message).route(), MessageRoute::Runtime);
        }
    }

    #[test]
    fn history_search_messages_have_one_compile_time_route() {
        for message in [
            HistorySearchMessage::QueryChanged("query".into()),
            HistorySearchMessage::CycleSource,
            HistorySearchMessage::SubmitCurrent,
            HistorySearchMessage::Submit(crate::history_search::LocalHistorySearchQuery::default()),
            HistorySearchMessage::Jump(crate::history_search::LocalHistoryResultKey::LxmfStored {
                peer_key: "peer".into(),
                message_index: 0,
                message_key: "message".into(),
            }),
            HistorySearchMessage::Completed {
                generation: 1,
                result: Ok(crate::history_search::LocalHistorySearchPage::default()),
            },
        ] {
            assert_eq!(
                Message::HistorySearch(Box::new(message)).route(),
                MessageRoute::HistorySearch
            );
        }
    }

    #[test]
    fn external_browser_domain_messages_have_one_compile_time_route() {
        for message in [
            ExternalBrowserMessage::SelectPreferred(0),
            ExternalBrowserMessage::ClearPreferred,
            ExternalBrowserMessage::OpenWith(0),
            ExternalBrowserMessage::CopyUrl,
            ExternalBrowserMessage::PromptUrl("https://example.org".into()),
            ExternalBrowserMessage::DismissPrompt,
        ] {
            assert_eq!(
                Message::ExternalBrowser(message).route(),
                MessageRoute::ExternalBrowser
            );
        }
    }

    #[test]
    fn clearweb_domain_messages_have_one_compile_time_route() {
        assert_eq!(
            Message::Clearweb(ClearwebMessage::ToggleSocksProxy).route(),
            MessageRoute::Clearweb
        );
        assert_eq!(
            Message::Clearweb(ClearwebMessage::ToggleRemoteMedia).route(),
            MessageRoute::Clearweb
        );
    }

    #[test]
    fn theme_domain_messages_have_one_compile_time_route() {
        assert_eq!(
            Message::Theme(ThemeMessage::SetTheme("omen".into())).route(),
            MessageRoute::Theme
        );
        assert_eq!(
            Message::Theme(ThemeMessage::SetFontSize(16)).route(),
            MessageRoute::Theme
        );
        assert_eq!(
            Message::Theme(ThemeMessage::ToggleReducedMotion).route(),
            MessageRoute::Theme
        );
    }

    #[test]
    fn unhandled_message_is_release_visible_in_status_and_persisted_logs() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-desktop-unhandled-message-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root.clone());
        paths.ensure().expect("isolated paths");
        let mut desktop = DesktopApp::new(App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        }));

        let _ = desktop.update(Message::TestUnhandledRouting);

        assert!(desktop
            .app
            .status
            .task
            .contains("internal UI routing error"));
        let entry = desktop.app.logs.entries.last().expect("routing log entry");
        assert_eq!(entry.severity, LogSeverity::Error);
        assert_eq!(entry.source, LogSource::App);
        assert!(entry
            .message
            .contains("unhandled desktop message discriminant"));
        assert!(desktop
            .app
            .flush_structured_logs(std::time::Duration::from_secs(2)));
        let persisted = std::fs::read_to_string(root.join("logs/omenbrowser_rs.jsonl"))
            .expect("persisted routing log");
        assert!(persisted.contains("unhandled desktop message discriminant"));
        assert!(!persisted.contains("TestUnhandledRouting"));
        let _ = std::fs::remove_dir_all(root);
    }
}
