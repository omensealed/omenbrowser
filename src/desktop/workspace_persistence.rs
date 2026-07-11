use crate::app::TabId;
use crate::storage::settings::{
    DesktopWorkspaceLayoutNode, DesktopWorkspacePaneKind, DesktopWorkspacePaneSettings,
};

use super::layout::desktop_workspace_node_to_settings;
use super::{DesktopApp, DesktopPane};

impl DesktopApp {
    pub(in crate::desktop) fn persist_workspace_panes(&mut self, label: &str) {
        let panes = self.desktop_workspace_pane_settings();
        let layout = self.desktop_workspace_layout_settings();
        let active = self
            .workspace
            .workspace_panes
            .iter()
            .position(|(pane, _)| *pane == self.workspace.active_workspace_pane);
        self.app
            .save_desktop_workspace_layout(panes, active, layout, label);
    }

    pub(in crate::desktop) fn schedule_workspace_panes_persist(&mut self, label: &str) {
        let panes = self.desktop_workspace_pane_settings();
        let layout = self.desktop_workspace_layout_settings();
        let active = self
            .workspace
            .workspace_panes
            .iter()
            .position(|(pane, _)| *pane == self.workspace.active_workspace_pane);
        self.app
            .schedule_desktop_workspace_layout_save(panes, active, layout, label);
    }

    pub(in crate::desktop) fn desktop_workspace_pane_settings(
        &self,
    ) -> Vec<DesktopWorkspacePaneSettings> {
        self.workspace
            .workspace_panes
            .iter()
            .filter_map(|(_, pane)| self.desktop_pane_to_settings(pane))
            .collect()
    }

    pub(in crate::desktop) fn desktop_pane_to_settings(
        &self,
        pane: &DesktopPane,
    ) -> Option<DesktopWorkspacePaneSettings> {
        match pane {
            DesktopPane::Browser(tab_id) => {
                let index = self
                    .app
                    .workspace
                    .browser_tabs
                    .iter()
                    .position(|tab| tab.id == *tab_id)?;
                Some(DesktopWorkspacePaneSettings {
                    kind: DesktopWorkspacePaneKind::Browser,
                    index,
                })
            }
            DesktopPane::Conversation(conversation_id) => {
                let index = self
                    .app
                    .workspace
                    .conversations
                    .iter()
                    .position(|conversation| conversation.id == *conversation_id)?;
                Some(DesktopWorkspacePaneSettings {
                    kind: DesktopWorkspacePaneKind::Conversation,
                    index,
                })
            }
            #[cfg(feature = "chat-client")]
            DesktopPane::OmenChat(session_id) => {
                let index = self
                    .omenchat
                    .chat_client
                    .sessions()
                    .iter()
                    .position(|session| session.session_id == *session_id)?;
                Some(DesktopWorkspacePaneSettings {
                    kind: DesktopWorkspacePaneKind::OmenChat,
                    index,
                })
            }
        }
    }

    pub(in crate::desktop) fn desktop_workspace_layout_settings(
        &self,
    ) -> Option<DesktopWorkspaceLayoutNode> {
        desktop_workspace_node_to_settings(self.workspace.workspace_panes.layout(), self)
    }

    pub(in crate::desktop) fn remove_workspace_panes_for_missing_targets(
        &mut self,
        closing_browser_id: Option<TabId>,
        closing_conversation_id: Option<u64>,
    ) {
        let browser_ids = self
            .app
            .workspace
            .browser_tabs
            .iter()
            .map(|tab| tab.id)
            .collect::<std::collections::BTreeSet<_>>();
        let conversation_ids = self
            .app
            .workspace
            .conversations
            .iter()
            .map(|conversation| conversation.id)
            .collect::<std::collections::BTreeSet<_>>();
        let stale = self
            .workspace
            .workspace_panes
            .iter()
            .filter_map(|(pane, kind)| {
                let missing = match kind {
                    DesktopPane::Browser(id) => {
                        Some(id) == closing_browser_id.as_ref() || !browser_ids.contains(id)
                    }
                    DesktopPane::Conversation(id) => {
                        Some(id) == closing_conversation_id.as_ref()
                            || !conversation_ids.contains(id)
                    }
                    #[cfg(feature = "chat-client")]
                    DesktopPane::OmenChat(id) => self.omenchat.chat_client.session(*id).is_none(),
                };
                missing.then_some(*pane)
            })
            .collect::<Vec<_>>();

        for pane in stale {
            if self.workspace.workspace_panes.len() <= 1 {
                break;
            }
            self.close_workspace_pane(pane);
        }
    }
}
