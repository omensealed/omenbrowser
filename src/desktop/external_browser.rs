use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::app::TabId;
use crate::browser::BrowserSession;
use crate::micron::render::HitAction;

use super::{DesktopApp, ExternalBrowserMessage, ExternalLinkPrompt, Message};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::desktop) struct ExternalBrowserChoice {
    pub(in crate::desktop) label: String,
    pub(in crate::desktop) command: String,
    pub(in crate::desktop) kind: ExternalBrowserKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::desktop) enum ExternalBrowserKind {
    Default,
    Standard,
}

pub(in crate::desktop) fn detect_external_browsers(
    preferred_command: Option<&str>,
) -> Vec<ExternalBrowserChoice> {
    let candidates = [
        ("Default browser", "xdg-open"),
        ("Firefox", "firefox"),
        ("LibreWolf", "librewolf"),
        ("Mullvad Browser", "mullvad-browser"),
        ("Brave", "brave-browser"),
        ("Brave", "brave"),
        ("Chromium", "chromium"),
        ("Chromium", "chromium-browser"),
        ("Chrome", "google-chrome"),
        ("Chrome", "google-chrome-stable"),
        ("Chrome", "chrome"),
        ("Chrome", "chromium-freeworld"),
        ("Qutebrowser", "qutebrowser"),
    ];
    detect_external_browsers_from_candidates(preferred_command, &candidates, command_available)
}

pub(in crate::desktop) fn detect_external_browsers_from_candidates(
    preferred_command: Option<&str>,
    candidates: &[(&str, &str)],
    available: impl Fn(&str) -> bool,
) -> Vec<ExternalBrowserChoice> {
    let mut choices = Vec::new();
    let mut seen_labels = HashSet::new();
    if let Some(command) = preferred_command {
        if let Some((label, _)) = candidates
            .iter()
            .find(|(_, candidate)| *candidate == command)
        {
            if available(command) {
                choices.push(ExternalBrowserChoice {
                    label: (*label).into(),
                    command: command.into(),
                    kind: external_browser_kind(command),
                });
                seen_labels.insert((*label).to_string());
            }
        }
    }
    for (label, command) in candidates.iter().copied() {
        if !available(command) {
            continue;
        }
        if seen_labels.contains(label) {
            continue;
        }
        let kind = external_browser_kind(command);
        if !choices
            .iter()
            .any(|choice: &ExternalBrowserChoice| choice.command == command)
        {
            choices.push(ExternalBrowserChoice {
                label: label.into(),
                command: command.into(),
                kind,
            });
            seen_labels.insert(label.into());
        }
    }
    if choices.is_empty() {
        choices.push(ExternalBrowserChoice {
            label: "Default browser".into(),
            command: "xdg-open".into(),
            kind: ExternalBrowserKind::Default,
        });
    }
    if let Some(command) = preferred_command {
        choices.sort_by_key(|choice| usize::from(choice.command != command));
    }
    choices
}

pub(in crate::desktop) fn open_external_url_with_choice(
    choice: &ExternalBrowserChoice,
    url: &str,
) -> Result<(), String> {
    let candidates = external_browser_open_candidates(choice, url);
    let mut errors = Vec::new();
    for (program, args) in candidates {
        match Command::new(&program).args(&args).spawn() {
            Ok(_) => return Ok(()),
            Err(error) => {
                errors.push(format!("{program} {:?}: {error}", args));
            }
        }
    }
    Err(errors.join(" | "))
}

pub(in crate::desktop) fn external_browser_open_candidates(
    choice: &ExternalBrowserChoice,
    url: &str,
) -> Vec<(String, Vec<String>)> {
    vec![(choice.command.clone(), vec![url.into()])]
}

impl DesktopApp {
    pub(super) fn dispatch_external_browser_message(
        &mut self,
        message: Message,
    ) -> Result<iced::Task<Message>, Message> {
        match message {
            Message::ExternalBrowser(ExternalBrowserMessage::SelectPreferred(index)) => {
                self.update_select_preferred_external_browser(index);
                Ok(iced::Task::none())
            }
            Message::ExternalBrowser(ExternalBrowserMessage::ClearPreferred) => {
                self.update_clear_preferred_external_browser();
                Ok(iced::Task::none())
            }
            Message::ExternalBrowser(ExternalBrowserMessage::OpenWith(index)) => {
                self.update_open_external_link_with(index);
                Ok(iced::Task::none())
            }
            Message::ExternalBrowser(ExternalBrowserMessage::CopyUrl) => {
                Ok(self.update_copy_external_link_url())
            }
            Message::ExternalBrowser(ExternalBrowserMessage::PromptUrl(url)) => {
                self.update_prompt_external_url(url);
                Ok(iced::Task::none())
            }
            Message::ExternalBrowser(ExternalBrowserMessage::DismissPrompt) => {
                self.update_dismiss_external_link_prompt();
                Ok(iced::Task::none())
            }
            _ => Err(message),
        }
    }

    pub(in crate::desktop) fn prompt_external_hit_action_if_needed(
        &mut self,
        action: &HitAction,
        source_tab: Option<TabId>,
    ) -> bool {
        let HitAction::Link(link) = action else {
            return false;
        };
        self.prompt_external_url_if_needed(link.target.clone(), source_tab)
    }

    pub(in crate::desktop) fn prompt_external_url_if_needed(
        &mut self,
        url: String,
        source_tab: Option<TabId>,
    ) -> bool {
        if !BrowserSession::is_clearweb_url(&url) {
            return false;
        }
        self.clearweb.external_link_prompt = Some(ExternalLinkPrompt { url, source_tab });
        self.app.status.task = if self.app.settings.clearweb.socks_proxy_enabled {
            let endpoint = self
                .clearweb
                .clearweb_proxy_endpoint
                .as_ref()
                .map(|(host, port)| format!("{host}:{port}"))
                .unwrap_or_else(|| {
                    format!(
                        "{}:{} or :9150",
                        self.app.settings.clearweb.socks_proxy_host,
                        self.app.settings.clearweb.socks_proxy_port
                    )
                });
            format!(
                "choose external browser; SOCKS5 {} {}",
                endpoint,
                if self.clearweb.clearweb_proxy_reachable {
                    "detected"
                } else {
                    "not detected"
                }
            )
        } else {
            "choose an external browser for this URL".into()
        };
        true
    }

    pub(in crate::desktop) fn prompt_focused_external_link_if_needed(&mut self) -> bool {
        let Some((tab_id, target)) = self
            .app
            .active_browser_tab()
            .focused_link
            .as_ref()
            .map(|link| (self.app.active_browser_tab().id, link.target.clone()))
        else {
            return false;
        };
        self.prompt_external_url_if_needed(target, Some(tab_id))
    }

    pub(in crate::desktop) fn open_pending_external_link(&mut self, index: usize) {
        let Some(prompt) = self.clearweb.external_link_prompt.clone() else {
            return;
        };
        self.clearweb.external_link_prompt = None;
        let Some(choice) = self.clearweb.external_browsers.get(index).cloned() else {
            self.app.status.task = "selected external browser is no longer available".into();
            return;
        };
        match open_external_url_with_choice(&choice, &prompt.url) {
            Ok(_) => {
                self.app.status.task =
                    format!("opened external URL in {}: {}", choice.label, prompt.url);
            }
            Err(error) => {
                self.app.status.task =
                    format!("failed to open external URL with {}: {error}", choice.label);
            }
        }
    }

    pub(super) fn update_open_external_link_with(&mut self, index: usize) {
        self.open_pending_external_link(index);
    }

    pub(super) fn update_copy_external_link_url(&mut self) -> iced::Task<Message> {
        if let Some(prompt) = &self.clearweb.external_link_prompt {
            let url = prompt.url.clone();
            self.clearweb.external_link_prompt = None;
            self.app.status.task = "copied external URL to clipboard".into();
            return iced::clipboard::write(url);
        }
        iced::Task::none()
    }

    pub(super) fn update_prompt_external_url(&mut self, url: String) {
        self.prompt_external_url_if_needed(url, None);
    }

    pub(super) fn update_dismiss_external_link_prompt(&mut self) {
        self.clearweb.external_link_prompt = None;
        self.app.status.task = "external URL open cancelled".into();
    }

    pub(super) fn update_select_preferred_external_browser(&mut self, index: usize) {
        if let Some(choice) = self.clearweb.external_browsers.get(index).cloned() {
            self.app
                .set_preferred_external_browser_command(Some(choice.command));
            self.clearweb.external_browsers = detect_external_browsers(
                self.app
                    .settings
                    .clearweb
                    .preferred_external_browser_command
                    .as_deref(),
            );
        }
    }

    pub(super) fn update_clear_preferred_external_browser(&mut self) {
        self.app.set_preferred_external_browser_command(None);
        self.clearweb.external_browsers = detect_external_browsers(None);
    }

    pub(in crate::desktop) fn open_local_file(&mut self, path: PathBuf) {
        match Command::new("xdg-open").arg(&path).spawn() {
            Ok(_) => {
                self.app.status.task = format!("opened file: {}", path.display());
            }
            Err(error) => {
                self.app.status.task = format!("failed to open file {}: {error}", path.display());
            }
        }
    }
}

fn command_available(command: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {}", shell_word(command)))
        .output()
        .is_ok_and(|output| output.status.success())
}

fn external_browser_kind(command: &str) -> ExternalBrowserKind {
    let executable = Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(command);
    if executable == "xdg-open" {
        ExternalBrowserKind::Default
    } else {
        ExternalBrowserKind::Standard
    }
}

fn shell_word(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::desktop::{DesktopPane, ExternalBrowserMessage, Message};
    use crate::micron::LinkAction;
    use iced::widget::scrollable::RelativeOffset;

    fn desktop_with_temp_root(name: &str) -> DesktopApp {
        let root = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        DesktopApp::new(App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        }))
    }

    #[test]
    fn focused_clearweb_micron_link_prompts_external_browser() {
        let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-clearweb-focused-link");
        let tab_id = desktop.app.active_browser_tab().id;
        desktop.app.active_browser_tab_mut().focused_link = Some(crate::app::FocusedLink {
            target: "https://example.org/news".into(),
            fields: Vec::new(),
            region_index: 0,
        });

        assert!(desktop.prompt_focused_external_link_if_needed());

        let prompt = desktop
            .clearweb
            .external_link_prompt
            .expect("external prompt");
        assert_eq!(prompt.url, "https://example.org/news");
        assert_eq!(prompt.source_tab, Some(tab_id));
    }

    #[test]
    fn clicked_clearweb_micron_link_prompts_external_browser() {
        let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-clearweb-clicked-link");
        let tab_id = desktop.app.active_browser_tab().id;
        let action = HitAction::Link(LinkAction {
            target: "https://example.org/news".into(),
            fields: Vec::new(),
        });

        assert!(desktop.prompt_external_hit_action_if_needed(&action, Some(tab_id)));

        let prompt = desktop
            .clearweb
            .external_link_prompt
            .expect("external prompt");
        assert_eq!(prompt.url, "https://example.org/news");
        assert_eq!(prompt.source_tab, Some(tab_id));
    }

    #[test]
    fn external_url_copy_dismisses_prompt() {
        let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-external-copy-dismiss");

        assert!(desktop.prompt_external_url_if_needed("https://example.org/news".into(), None));
        assert!(desktop.clearweb.external_link_prompt.is_some());

        let _ = desktop.update(Message::ExternalBrowser(ExternalBrowserMessage::CopyUrl));

        assert!(desktop.clearweb.external_link_prompt.is_none());
        assert_eq!(desktop.app.status.task, "copied external URL to clipboard");
    }

    #[test]
    fn external_url_prompt_does_not_arm_workspace_scroll_restore() {
        let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-external-scroll-stable");
        let conversation_id = desktop.app.active_conversation().id;
        desktop.ensure_pane_for_active_conversation();
        desktop
            .conversation
            .scroll_offsets
            .insert(conversation_id, RelativeOffset { x: 0.0, y: 0.72 });
        desktop.workspace.restore_workspace_scrolls_pending = false;
        desktop.workspace.restore_workspace_scrolls_remaining = 0;
        desktop
            .workspace
            .restore_workspace_scroll_locks_release_pending = false;
        desktop.conversation.scroll_restore_locks.clear();

        assert!(desktop.prompt_external_url_if_needed("https://example.org/news".into(), None));
        assert!(!desktop.workspace.restore_workspace_scrolls_pending);
        assert_eq!(desktop.workspace.restore_workspace_scrolls_remaining, 0);
        assert!(
            !desktop
                .workspace
                .restore_workspace_scroll_locks_release_pending
        );
        assert!(desktop.conversation.scroll_restore_locks.is_empty());
        assert_eq!(
            desktop.conversation.scroll_offsets.get(&conversation_id),
            Some(&RelativeOffset { x: 0.0, y: 0.72 })
        );

        let _ = desktop.update(Message::ExternalBrowser(ExternalBrowserMessage::CopyUrl));

        assert!(!desktop.workspace.restore_workspace_scrolls_pending);
        assert_eq!(desktop.workspace.restore_workspace_scrolls_remaining, 0);
        assert!(
            !desktop
                .workspace
                .restore_workspace_scroll_locks_release_pending
        );
        assert!(desktop.conversation.scroll_restore_locks.is_empty());
        assert_eq!(
            desktop.conversation.scroll_offsets.get(&conversation_id),
            Some(&RelativeOffset { x: 0.0, y: 0.72 })
        );
    }

    #[test]
    fn external_url_browser_choice_dismisses_prompt_after_selection() {
        let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-external-open-dismiss");
        desktop.clearweb.external_browsers = vec![ExternalBrowserChoice {
            label: "Missing browser".into(),
            command: "omenbrowser-rs-missing-browser-command".into(),
            kind: ExternalBrowserKind::Standard,
        }];

        assert!(desktop.prompt_external_url_if_needed("https://example.org/news".into(), None));
        assert!(desktop.clearweb.external_link_prompt.is_some());

        let _ = desktop.update(Message::ExternalBrowser(ExternalBrowserMessage::OpenWith(
            0,
        )));

        assert!(desktop.clearweb.external_link_prompt.is_none());
        assert!(desktop
            .app
            .status
            .task
            .starts_with("failed to open external URL with Missing browser:"));
    }

    #[test]
    fn external_browser_choices_do_not_launch_tor_browser() {
        let choices = detect_external_browsers(None);
        let commands = choices
            .iter()
            .map(|choice| choice.command.as_str())
            .collect::<Vec<_>>();

        assert!(
            !commands.iter().any(|command| {
                command.contains("torbrowser-launcher")
                    || command.contains("tor-browser")
                    || command.contains("start-tor-browser")
            }),
            "Tor Browser should use the Copy URL flow, not a launcher button: {commands:?}"
        );
    }

    #[test]
    fn external_browser_choices_keep_one_entry_per_browser_label() {
        let candidates = [
            ("Default browser", "xdg-open"),
            ("Chrome", "google-chrome"),
            ("Chrome", "google-chrome-stable"),
            ("Brave", "brave-browser"),
            ("Brave", "brave"),
        ];

        let choices = detect_external_browsers_from_candidates(None, &candidates, |_| true);

        assert_eq!(
            choices
                .iter()
                .map(|choice| (choice.label.as_str(), choice.command.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("Default browser", "xdg-open"),
                ("Chrome", "google-chrome"),
                ("Brave", "brave-browser"),
            ]
        );
    }

    #[test]
    fn external_browser_choices_preserve_preferred_duplicate_command() {
        let candidates = [
            ("Chrome", "google-chrome"),
            ("Chrome", "google-chrome-stable"),
            ("Brave", "brave-browser"),
        ];

        let choices = detect_external_browsers_from_candidates(
            Some("google-chrome-stable"),
            &candidates,
            |_| true,
        );

        assert_eq!(
            choices
                .iter()
                .map(|choice| (choice.label.as_str(), choice.command.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("Chrome", "google-chrome-stable"),
                ("Brave", "brave-browser"),
            ]
        );
    }

    #[test]
    fn standard_external_browser_candidate_uses_url_argument_only() {
        let choice = ExternalBrowserChoice {
            label: "Default browser".into(),
            command: "xdg-open".into(),
            kind: ExternalBrowserKind::Default,
        };

        assert_eq!(
            external_browser_open_candidates(&choice, "https://example.org"),
            vec![("xdg-open".into(), vec!["https://example.org".into()])]
        );
    }

    #[test]
    fn desktop_browser_titles_put_page_name_before_browser_label_and_strip_controls() {
        let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-pane-title");
        let tab_id = desktop.app.active_browser_tab().id;
        desktop.app.active_browser_tab_mut().title = "Node\u{7} Home".into();

        assert_eq!(
            desktop.workspace_pane_title(&DesktopPane::Browser(tab_id)),
            "Node Home - Browser"
        );
    }
}
