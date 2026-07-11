use super::external_browser::ExternalBrowserChoice;
use super::message::ExternalLinkPrompt;

pub(in crate::desktop) struct ClearwebDesktopState {
    pub(in crate::desktop) external_link_prompt: Option<ExternalLinkPrompt>,
    pub(in crate::desktop) external_browsers: Vec<ExternalBrowserChoice>,
    pub(in crate::desktop) clearweb_proxy_reachable: bool,
    pub(in crate::desktop) clearweb_proxy_endpoint: Option<(String, u16)>,
}
