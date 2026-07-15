use crate::app::TabId;
use crate::desktop::page_widget::PageMessage;
use crate::workspace::WorkspaceSection;

#[cfg(test)]
use super::ShellMessage;
use super::{BrowserFieldKey, BrowserMessage, DesktopApp, Message};
use iced::Task;

impl DesktopApp {
    pub(super) fn dispatch_browser_message(
        &mut self,
        message: Message,
    ) -> Result<Task<Message>, Message> {
        match message {
            Message::Browser(BrowserMessage::SelectTab(index)) => {
                self.update_select_browser_tab(index);
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::NewTab) => Ok(self.update_new_browser_tab()),
            Message::Browser(BrowserMessage::CloseTab) => {
                self.update_close_active_browser_tab();
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::ClosePaneTab(tab_id)) => {
                self.update_close_browser_pane_tab(tab_id);
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::AddressChanged(value)) => {
                self.update_active_browser_address_changed(value);
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::PaneAddressChanged { tab_id, value }) => {
                self.update_browser_pane_address_changed(tab_id, value);
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::OpenAddress) => {
                self.update_open_active_browser_address();
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::OpenPaneAddress(tab_id)) => {
                self.update_open_browser_pane_address(tab_id);
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::ReloadPane(tab_id)) => {
                self.update_browser_pane_reload(tab_id);
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::PaneBack(tab_id)) => {
                self.update_browser_pane_back(tab_id);
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::PaneForward(tab_id)) => {
                self.update_browser_pane_forward(tab_id);
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::PaneTop(tab_id)) => {
                self.update_browser_pane_top(tab_id);
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::StopPaneTask(tab_id)) => {
                self.update_browser_pane_stop(tab_id);
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::InlineProbePane(tab_id)) => {
                self.update_browser_pane_inline_probe(tab_id);
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::LiveProbePane(tab_id)) => {
                self.update_browser_pane_live_probe(tab_id);
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::WarmPanePath(tab_id)) => {
                self.update_browser_pane_warm_path(tab_id);
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::RetryPaneAfterPath(tab_id)) => {
                self.update_browser_pane_retry_after_path(tab_id);
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::PanePathDiagnostics(tab_id)) => {
                self.update_browser_pane_path_diagnostics(tab_id);
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::CapturePaneRender(tab_id)) => {
                self.update_browser_pane_capture_render(tab_id);
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::DismissPaneWarning(tab_id)) => {
                self.update_browser_pane_dismiss_warning(tab_id);
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::DismissPaneRequest(tab_id)) => {
                self.update_browser_pane_dismiss_request(tab_id);
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::TogglePaneIdentify(tab_id)) => {
                self.update_browser_pane_toggle_identify(tab_id);
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::OpenSetupAddress) => {
                self.update_open_setup_address();
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::Reload) => {
                self.update_active_browser_reload();
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::Back) => {
                self.update_active_browser_back();
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::Forward) => {
                self.update_active_browser_forward();
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::StopTask) => {
                self.update_active_browser_stop();
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::InlineProbe) => {
                self.update_active_browser_inline_probe();
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::LiveProbe) => {
                self.update_active_browser_live_probe();
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::WarmPath) => {
                self.update_active_browser_warm_path();
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::RetryAfterPath) => {
                self.update_active_browser_retry_after_path();
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::PathDiagnostics) => {
                self.update_active_browser_path_diagnostics();
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::CaptureRender) => {
                self.update_active_browser_capture_render();
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::FieldKey(key)) => {
                self.update_browser_field_key(key);
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::SubmitFieldDraft) => {
                self.update_submit_browser_field_draft();
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::CancelFieldDraft) => {
                self.update_cancel_browser_field_draft();
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::FocusItem { reverse }) => {
                self.update_focus_browser_item(reverse);
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::ActivateFocusedItem) => {
                Ok(self.update_activate_focused_browser_item())
            }
            Message::Browser(BrowserMessage::ScrollPage { direction }) => {
                self.update_scroll_browser_page(direction);
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::Zoom { direction }) => {
                self.update_browser_zoom(direction);
                Ok(Task::none())
            }
            Message::Browser(BrowserMessage::Page(page)) => {
                Ok(self.update_active_page_message(page))
            }
            Message::Browser(BrowserMessage::PageForTab { tab_id, page }) => {
                Ok(self.update_page_message_for_tab(tab_id, page))
            }
            _ => Err(message),
        }
    }

    pub(super) fn update_active_browser_address_changed(&mut self, value: String) {
        self.app.finish_active_browser_field_edit_preserving_value();
        self.app.active_browser_tab_mut().address_input = value;
    }

    pub(super) fn update_browser_pane_address_changed(&mut self, tab_id: TabId, value: String) {
        if self.select_browser_tab_by_id(tab_id) {
            self.app.finish_active_browser_field_edit_preserving_value();
            self.app.active_browser_tab_mut().address_input = value;
        }
    }

    pub(super) fn update_open_active_browser_address(&mut self) {
        let target = self.app.active_browser_tab().address_input.clone();
        if !self.prompt_external_url_if_needed(target, None) {
            self.app.open_active_browser_address();
        }
    }

    pub(super) fn update_open_browser_pane_address(&mut self, tab_id: TabId) {
        if self.select_browser_tab_by_id(tab_id) {
            let target = self.app.active_browser_tab().address_input.clone();
            if !self.prompt_external_url_if_needed(target, Some(tab_id)) {
                self.app.open_active_browser_address();
            }
        }
    }

    pub(super) fn update_browser_pane_reload(&mut self, tab_id: TabId) {
        if self.select_browser_tab_by_id(tab_id) {
            self.app.reload_active_browser();
        }
    }

    pub(super) fn update_browser_pane_back(&mut self, tab_id: TabId) {
        if self.select_browser_tab_by_id(tab_id) {
            self.app.browser_back();
        }
    }

    pub(super) fn update_browser_pane_forward(&mut self, tab_id: TabId) {
        if self.select_browser_tab_by_id(tab_id) {
            self.app.browser_forward();
        }
    }

    pub(super) fn update_browser_pane_top(&mut self, tab_id: TabId) {
        if self.select_browser_tab_by_id(tab_id) {
            self.app.scroll_browser_tab_to_top(tab_id);
        }
    }

    pub(super) fn update_browser_pane_stop(&mut self, tab_id: TabId) {
        if self.select_browser_tab_by_id(tab_id) {
            self.app.cancel_active_browser_load();
        }
    }

    pub(super) fn update_browser_pane_inline_probe(&mut self, tab_id: TabId) {
        if self.select_browser_tab_by_id(tab_id) {
            self.app.probe_active_browser_page_fetch_inline();
        }
    }

    pub(super) fn update_browser_pane_live_probe(&mut self, tab_id: TabId) {
        if self.select_browser_tab_by_id(tab_id) {
            self.app.probe_active_browser_page_fetch(true);
        }
    }

    pub(super) fn update_browser_pane_warm_path(&mut self, tab_id: TabId) {
        if self.select_browser_tab_by_id(tab_id) {
            self.app.warm_active_browser_path();
        }
    }

    pub(super) fn update_browser_pane_retry_after_path(&mut self, tab_id: TabId) {
        if self.select_browser_tab_by_id(tab_id) {
            self.app.retry_active_browser_after_path_discovery();
        }
    }

    pub(super) fn update_browser_pane_path_diagnostics(&mut self, tab_id: TabId) {
        if self.select_browser_tab_by_id(tab_id) {
            self.app.set_diagnostics_target_for_browser_tab(tab_id);
            self.app.run_active_browser_path_discovery_diagnostics();
        }
    }

    pub(super) fn update_browser_pane_capture_render(&mut self, tab_id: TabId) {
        if self.select_browser_tab_by_id(tab_id) {
            self.app.export_active_browser_render_fixture();
        }
    }

    pub(super) fn update_browser_pane_dismiss_warning(&mut self, tab_id: TabId) {
        if self.select_browser_tab_by_id(tab_id) {
            self.app.dismiss_active_browser_live_warning();
        }
    }

    pub(super) fn update_browser_pane_dismiss_request(&mut self, tab_id: TabId) {
        if self.select_browser_tab_by_id(tab_id) {
            self.app.dismiss_active_browser_request_preview();
        }
    }

    pub(super) fn update_browser_pane_toggle_identify(&mut self, tab_id: TabId) {
        if self.select_browser_tab_by_id(tab_id) {
            self.app.toggle_active_browser_node_identify_on_connect();
        }
    }

    pub(super) fn update_open_setup_address(&mut self) {
        self.app.switch_section(WorkspaceSection::Browser);
        self.app.open_active_browser_address();
    }

    pub(super) fn update_active_browser_reload(&mut self) {
        self.app.reload_active_browser();
    }

    pub(super) fn update_active_browser_back(&mut self) {
        self.app.browser_back();
    }

    pub(super) fn update_active_browser_forward(&mut self) {
        self.app.browser_forward();
    }

    pub(super) fn update_active_browser_stop(&mut self) {
        self.app.cancel_active_browser_load();
    }

    pub(super) fn update_active_browser_inline_probe(&mut self) {
        self.app.probe_active_browser_page_fetch_inline();
    }

    pub(super) fn update_active_browser_live_probe(&mut self) {
        self.app.probe_active_browser_page_fetch(true);
    }

    pub(super) fn update_active_browser_warm_path(&mut self) {
        self.app.warm_active_browser_path();
    }

    pub(super) fn update_active_browser_retry_after_path(&mut self) {
        self.app.retry_active_browser_after_path_discovery();
    }

    pub(super) fn update_active_browser_path_diagnostics(&mut self) {
        self.app.run_active_browser_path_discovery_diagnostics();
    }

    pub(super) fn update_active_browser_capture_render(&mut self) {
        self.app.export_active_browser_render_fixture();
    }

    pub(super) fn update_browser_field_key(&mut self, key: BrowserFieldKey) {
        self.apply_browser_field_key(key);
    }

    pub(super) fn update_submit_browser_field_draft(&mut self) {
        self.app.submit_active_input();
    }

    pub(super) fn update_cancel_browser_field_draft(&mut self) {
        self.app.cancel_active_input();
    }

    pub(super) fn update_focus_browser_item(&mut self, reverse: bool) {
        if self.app.workspace.active_section == WorkspaceSection::Browser {
            self.app.focus_browser_item_with_viewport(
                self.app.browser_viewport_width(),
                self.app.browser_viewport_height(),
                reverse,
            );
        }
    }

    pub(super) fn update_activate_focused_browser_item(&mut self) -> Task<Message> {
        if self.app.workspace.active_section == WorkspaceSection::Browser {
            #[cfg(feature = "chat-client")]
            if let Some(task) = self.activate_focused_omenchat_link() {
                return task;
            }
            if self.prompt_focused_external_link_if_needed() {
                return Task::none();
            }
            if self.activate_focused_lxmf_link() {
                return Task::none();
            }
            self.app.activate_focused_browser_control();
        }
        Task::none()
    }

    pub(super) fn update_scroll_browser_page(&mut self, direction: isize) {
        if self.app.workspace.active_section == WorkspaceSection::Browser {
            self.app
                .scroll_active_browser_page(self.app.browser_viewport_height(), direction);
        }
    }

    pub(super) fn update_browser_zoom(&mut self, direction: isize) {
        if self.app.workspace.active_section == WorkspaceSection::Browser {
            let active = self.app.active_browser_tab().id;
            self.app.zoom_browser_tab(active, direction);
        }
    }

    pub(super) fn update_active_page_message(&mut self, page: PageMessage) -> Task<Message> {
        match page {
            PageMessage::Activate {
                row,
                col,
                width,
                action,
            } => {
                #[cfg(feature = "chat-client")]
                if let Some(task) = self.activate_omenchat_hit_action_if_needed(&action) {
                    return task;
                }
                if self.activate_lxmf_hit_action_if_needed(&action) {
                    return Task::none();
                }
                if self.prompt_external_hit_action_if_needed(&action, None) {
                    return Task::none();
                }
                if !self.app.activate_browser_hit_action(action) {
                    self.app.activate_browser_cell(row, col, width);
                }
            }
            PageMessage::Scroll {
                delta,
                width,
                height,
            } => {
                self.app.set_browser_viewport(width, height);
                if self.ui.ctrl_down {
                    let active = self.app.active_browser_tab().id;
                    let direction = if delta <= 0 { 1 } else { -1 };
                    self.app.zoom_browser_tab(active, direction);
                } else {
                    self.app.scroll_active_browser_lines(delta);
                }
            }
        }
        Task::none()
    }

    pub(super) fn update_page_message_for_tab(
        &mut self,
        tab_id: TabId,
        page: PageMessage,
    ) -> Task<Message> {
        match page {
            PageMessage::Activate {
                row,
                col,
                width,
                action,
            } => {
                if self.select_browser_tab_by_id(tab_id) {
                    #[cfg(feature = "chat-client")]
                    if let Some(task) = self.activate_omenchat_hit_action_if_needed(&action) {
                        return task;
                    }
                    if self.activate_lxmf_hit_action_if_needed(&action) {
                        return Task::none();
                    }
                    if !self.prompt_external_hit_action_if_needed(&action, Some(tab_id))
                        && !self.app.activate_browser_hit_action(action)
                    {
                        self.app.activate_browser_cell(row, col, width);
                    }
                }
            }
            PageMessage::Scroll {
                delta,
                width,
                height,
            } => {
                self.app.set_browser_tab_viewport(tab_id, width, height);
                if self.ui.ctrl_down {
                    let direction = if delta <= 0 { 1 } else { -1 };
                    self.app.zoom_browser_tab(tab_id, direction);
                } else {
                    self.app.scroll_browser_tab_lines(tab_id, delta);
                }
            }
        }
        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;

    #[tokio::test]
    async fn setup_open_address_switches_to_browser_and_uses_active_address() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-open-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);

        let _ = desktop.update(Message::Shell(ShellMessage::SwitchSection(
            WorkspaceSection::Settings,
        )));
        let _ = desktop.update(Message::Browser(BrowserMessage::AddressChanged(
            "mock.page:/page/gallery.mu".into(),
        )));
        let _ = desktop.update(Message::Browser(BrowserMessage::OpenSetupAddress));

        assert_eq!(
            desktop.app.workspace.active_section,
            WorkspaceSection::Browser
        );
        assert_eq!(
            desktop.app.active_browser_tab().address_input,
            "mock.page:/page/gallery.mu"
        );
    }
}
