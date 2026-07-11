use std::net::{SocketAddr, TcpStream};
#[cfg(feature = "chat-client")]
use std::path::Path;
use std::time::Duration;

#[cfg(feature = "chat-client")]
use crate::browser::DownloadedFile;
#[cfg(feature = "chat-client")]
use crate::storage::files::next_available_download_path;

use iced::Task;

use super::{DesktopApp, Message};

const COMMON_TOR_SOCKS_PORTS: &[u16] = &[9050, 9150];

impl DesktopApp {
    pub(super) fn dispatch_clearweb_message(
        &mut self,
        message: Message,
    ) -> Result<Task<Message>, Message> {
        match message {
            Message::ToggleClearwebSocksProxy => {
                self.update_toggle_clearweb_socks_proxy();
                Ok(Task::none())
            }
            Message::ToggleClearwebRemoteMedia => {
                self.update_toggle_clearweb_remote_media();
                Ok(Task::none())
            }
            _ => Err(message),
        }
    }

    pub(super) fn update_toggle_clearweb_socks_proxy(&mut self) {
        self.app.toggle_clearweb_socks_proxy();
    }

    pub(super) fn update_toggle_clearweb_remote_media(&mut self) {
        self.app.toggle_clearweb_remote_media();
    }
}

pub(super) fn detect_clearweb_socks_proxy(
    host: &str,
    configured_port: u16,
) -> Option<(String, u16)> {
    let timeout = Duration::from_millis(75);
    std::iter::once(configured_port)
        .chain(COMMON_TOR_SOCKS_PORTS.iter().copied())
        .find(|port| local_tcp_reachable(host, *port, timeout))
        .map(|port| (host.into(), port))
}

fn local_tcp_reachable(host: &str, port: u16, timeout: Duration) -> bool {
    let Ok(addr) = format!("{host}:{port}").parse::<SocketAddr>() else {
        return false;
    };
    TcpStream::connect_timeout(&addr, timeout).is_ok()
}

#[cfg(feature = "chat-client")]
pub(super) async fn fetch_clearweb_media_over_socks(
    url: &str,
    media_cache_dir: &Path,
    socks_host: &str,
    socks_port: u16,
    timeout: Duration,
) -> anyhow::Result<DownloadedFile> {
    if !crate::media::is_clearweb_url(url) {
        anyhow::bail!("clearweb media fetch requires http or https URL");
    }

    let proxy = reqwest::Proxy::all(format!("socks5h://{socks_host}:{socks_port}"))?;
    let client = reqwest::Client::builder()
        .proxy(proxy)
        .timeout(timeout)
        .user_agent("OMENbrowser_rs/0.1 OMENchat-media")
        .build()?;
    let response = client.get(url).send().await?.error_for_status()?;
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(';').next().unwrap_or(value).trim().to_string())
        .unwrap_or_else(|| "application/octet-stream".into());
    if !content_type.to_ascii_lowercase().starts_with("image/") {
        anyhow::bail!("remote media response was not an image: {content_type}");
    }

    let bytes = response.bytes().await?;
    const OMENCHAT_MEDIA_MAX_BYTES: usize = 8 * 1024 * 1024;
    if bytes.len() > OMENCHAT_MEDIA_MAX_BYTES {
        anyhow::bail!("remote image is too large: {} bytes", bytes.len());
    }

    let filename = clearweb_media_cache_filename(url, &content_type);
    let path = next_available_download_path(media_cache_dir, &filename)?;
    tokio::fs::write(&path, bytes.as_ref()).await?;
    Ok(DownloadedFile {
        url: url.to_string(),
        path,
        content_type,
    })
}

#[cfg(feature = "chat-client")]
pub(super) fn clearweb_media_cache_filename(url: &str, content_type: &str) -> String {
    let path = url
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split(['?', '#']).next())
        .and_then(|rest| rest.rsplit('/').next())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("omenchat-media");
    let mut filename = path.to_string();
    if !filename.contains('.') {
        filename.push('.');
        filename.push_str(match content_type.to_ascii_lowercase().as_str() {
            "image/png" => "png",
            "image/jpeg" => "jpg",
            "image/gif" => "gif",
            "image/webp" => "webp",
            "image/bmp" => "bmp",
            "image/avif" => "avif",
            _ => "img",
        });
    }
    filename
}

#[cfg(all(test, feature = "chat-client"))]
mod tests {
    use super::*;

    #[test]
    fn clearweb_media_cache_filename_infers_extension_when_missing() {
        assert_eq!(
            clearweb_media_cache_filename("https://cdn.example.test/images/cat", "image/png"),
            "cat.png"
        );
        assert_eq!(
            clearweb_media_cache_filename(
                "https://cdn.example.test/images/loop?cache=false",
                "image/gif"
            ),
            "loop.gif"
        );
        assert_eq!(
            clearweb_media_cache_filename("https://cdn.example.test/images/cat.jpg", "image/png"),
            "cat.jpg"
        );
    }
}
