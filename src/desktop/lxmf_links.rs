use crate::micron::render::HitAction;

use super::DesktopApp;

impl DesktopApp {
    pub(in crate::desktop) fn activate_focused_lxmf_link(&mut self) -> bool {
        let Some(link) = self
            .app
            .active_browser_tab()
            .focused_link
            .as_ref()
            .map(|link| crate::micron::LinkAction {
                target: link.target.clone(),
                fields: link.fields.clone(),
            })
        else {
            return false;
        };
        self.activate_lxmf_link(link)
    }

    pub(in crate::desktop) fn activate_lxmf_hit_action_if_needed(
        &mut self,
        action: &HitAction,
    ) -> bool {
        let HitAction::Link(link) = action else {
            return false;
        };
        self.activate_lxmf_link(link.clone())
    }

    pub(in crate::desktop) fn activate_lxmf_link(
        &mut self,
        link: crate::micron::LinkAction,
    ) -> bool {
        if !self.app.open_lxmf_peer_link(&link.target) {
            return false;
        }
        self.ensure_pane_for_active_conversation();
        self.persist_workspace_panes("workspace panes");
        true
    }
}
