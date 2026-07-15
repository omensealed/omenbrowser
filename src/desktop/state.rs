use crate::app::App;

use super::clearweb_state::ClearwebDesktopState;
use super::conversation_state::ConversationDesktopState;
use super::monitoring_state::DesktopMonitoringState;
#[cfg(feature = "chat-client")]
use super::omenchat_desktop_state::OmenChatDesktopState;
#[cfg(feature = "chat-client")]
use super::startup::restore_omenchat_startup_state;
use super::startup::{
    clearweb_startup_state, conversation_startup_state, desktop_workspace_startup_state,
};
use super::ui_state::DesktopUiState;
use super::workspace_state::DesktopWorkspaceState;

#[cfg(all(test, feature = "chat-client"))]
#[path = "state_tests.rs"]
mod tests;

pub(in crate::desktop) struct DesktopApp {
    pub(in crate::desktop) app: App,
    pub(in crate::desktop) conversation: ConversationDesktopState,
    pub(in crate::desktop) workspace: DesktopWorkspaceState,
    pub(in crate::desktop) ui: DesktopUiState,
    pub(in crate::desktop) monitoring: DesktopMonitoringState,
    pub(in crate::desktop) clearweb: ClearwebDesktopState,
    #[cfg(feature = "chat-client")]
    pub(in crate::desktop) omenchat: OmenChatDesktopState,
}

impl DesktopApp {
    pub(super) fn new(app: App) -> Self {
        #[cfg(feature = "chat-client")]
        let omenchat_startup = restore_omenchat_startup_state(&app);
        #[cfg(feature = "chat-client")]
        let omenchat_session_ids = omenchat_startup.session_ids.clone();
        #[cfg(not(feature = "chat-client"))]
        let omenchat_session_ids = Vec::new();
        let workspace_startup = desktop_workspace_startup_state(&app, &omenchat_session_ids);
        let conversation_startup =
            conversation_startup_state(&app, &workspace_startup.workspace_panes);
        #[cfg(feature = "chat-client")]
        let omenchat = OmenChatDesktopState::from_startup(
            omenchat_startup,
            &workspace_startup.workspace_panes,
        );
        #[cfg(feature = "chat-client")]
        let restore_workspace_scrolls_pending = !conversation_startup.scroll_offsets.is_empty()
            || !omenchat.chat_scroll_offsets.is_empty();
        #[cfg(not(feature = "chat-client"))]
        let restore_workspace_scrolls_pending = !conversation_startup.scroll_offsets.is_empty();
        let workspace = DesktopWorkspaceState::from_startup(
            workspace_startup.workspace_panes,
            workspace_startup.active_workspace_pane,
            restore_workspace_scrolls_pending,
        );
        let clearweb = clearweb_startup_state(&app);

        tracing::info!(
            workspace_panes = workspace.workspace_panes.len(),
            browser_tabs = app.workspace.browser_tabs.len(),
            conversations = app.workspace.conversations.len(),
            omenchat_sessions = omenchat_session_ids.len(),
            "desktop workspace restored"
        );

        Self {
            app,
            conversation: conversation_startup,
            workspace,
            ui: Default::default(),
            monitoring: Default::default(),
            clearweb,
            #[cfg(feature = "chat-client")]
            omenchat,
        }
    }
}
