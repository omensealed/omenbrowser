use super::DesktopApp;

impl DesktopApp {
    pub(super) fn update_conversation_title_changed(&mut self, value: String) {
        self.app.set_active_conversation_draft_title(value);
    }

    pub(super) fn update_conversation_body_changed(&mut self, value: String) {
        self.app.set_active_conversation_draft_body(value);
        self.sync_conversation_body_editor(self.app.active_conversation().id);
    }

    pub(super) fn update_toggle_conversation_delivery_mode(&mut self) {
        self.app.toggle_active_conversation_delivery_mode();
    }

    pub(super) fn update_toggle_conversation_ticket(&mut self) {
        self.app.toggle_active_conversation_ticket();
    }

    pub(super) fn update_send_conversation_draft(&mut self) {
        self.app.send_active_conversation_draft();
    }

    pub(super) fn update_prepare_latest_lxmf_retry(&mut self) {
        self.app.prepare_latest_lxmf_retry();
    }

    pub(super) fn update_send_latest_lxmf_retry(&mut self) {
        self.app.send_latest_lxmf_retry();
    }

    pub(super) fn update_select_conversation_row(&mut self, key: String) {
        self.app.select_active_conversation_message(key);
    }

    pub(super) fn update_prepare_lxmf_retry_for_row(&mut self, key: String) {
        self.app.prepare_lxmf_retry_by_message_key(&key);
    }

    pub(super) fn update_send_lxmf_retry_for_row(&mut self, key: String) {
        self.app.send_lxmf_retry_by_message_key(&key);
    }

    pub(super) fn update_sync_propagation_for_row(&mut self, key: String) {
        self.app.sync_propagation_for_message_key(&key);
    }

    pub(super) fn update_sync_messages(&mut self) {
        self.app.sync_runtime_messages();
    }

    pub(super) fn update_inspect_lxmf_peer(&mut self) {
        self.app.inspect_active_lxmf_peer();
    }

    pub(super) fn update_request_lxmf_peer_path(&mut self) {
        self.app.request_active_lxmf_peer_path();
    }
}
