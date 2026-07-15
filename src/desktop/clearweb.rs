#[cfg(feature = "chat-client")]
use std::collections::VecDeque;
use std::net::{SocketAddr, TcpStream};
#[cfg(feature = "chat-client")]
use std::path::Path;
#[cfg(feature = "chat-client")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "chat-client")]
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

#[cfg(feature = "chat-client")]
use tokio::io::AsyncWriteExt;

#[cfg(feature = "chat-client")]
use crate::browser::DownloadedFile;
#[cfg(feature = "chat-client")]
use crate::storage::files::next_available_download_path;

use iced::Task;

#[cfg(feature = "chat-client")]
use super::omenchat_media_tasks::mark_omenchat_media_disk_dirty_async;
use super::{ClearwebMessage, DesktopApp, Message};

const COMMON_TOR_SOCKS_PORTS: &[u16] = &[9050, 9150];

#[cfg(feature = "chat-client")]
const SOCKS_MEDIA_CLIENT_CACHE_MAX_ITEMS: usize = 4;
#[cfg(feature = "chat-client")]
static SOCKS_MEDIA_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[cfg(feature = "chat-client")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct SocksMediaClientKey {
    host: String,
    port: u16,
    timeout_millis: u64,
}

#[cfg(feature = "chat-client")]
struct SocksMediaClientCache {
    clients: VecDeque<(SocksMediaClientKey, reqwest::Client)>,
}

#[cfg(feature = "chat-client")]
impl SocksMediaClientCache {
    fn new() -> Self {
        Self {
            clients: VecDeque::new(),
        }
    }

    fn get(&mut self, key: &SocksMediaClientKey) -> Option<reqwest::Client> {
        let position = self.clients.iter().position(|(stored, _)| stored == key)?;
        let entry = self.clients.remove(position)?;
        let client = entry.1.clone();
        self.clients.push_back(entry);
        Some(client)
    }

    fn insert(&mut self, key: SocksMediaClientKey, client: reqwest::Client) {
        self.clients.retain(|(stored, _)| stored != &key);
        while self.clients.len() >= SOCKS_MEDIA_CLIENT_CACHE_MAX_ITEMS {
            self.clients.pop_front();
        }
        self.clients.push_back((key, client));
    }
}

#[cfg(feature = "chat-client")]
static SOCKS_MEDIA_CLIENTS: LazyLock<Mutex<SocksMediaClientCache>> =
    LazyLock::new(|| Mutex::new(SocksMediaClientCache::new()));

impl DesktopApp {
    pub(super) fn dispatch_clearweb_message(
        &mut self,
        message: Message,
    ) -> Result<Task<Message>, Message> {
        match message {
            Message::Clearweb(ClearwebMessage::ToggleSocksProxy) => {
                self.update_toggle_clearweb_socks_proxy();
                Ok(Task::none())
            }
            Message::Clearweb(ClearwebMessage::ToggleRemoteMedia) => {
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

    let parsed_url = reqwest::Url::parse(url)?;
    validate_clearweb_media_url(&parsed_url)?;
    let client = socks_media_client(socks_host, socks_port, timeout)?;
    let mut response = client.get(parsed_url).send().await?.error_for_status()?;
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(';').next().unwrap_or(value).trim().to_string())
        .unwrap_or_else(|| "application/octet-stream".into());
    if !content_type.to_ascii_lowercase().starts_with("image/") {
        anyhow::bail!("remote media response was not an image: {content_type}");
    }

    let max_bytes = super::OMENCHAT_GIF_ENCODED_MAX_BYTES as u64;
    enforce_media_content_length(response.content_length(), max_bytes)?;

    let filename = clearweb_media_cache_filename(url, &content_type);
    let path = next_available_download_path(media_cache_dir, &filename)?;
    mark_omenchat_media_disk_dirty_async(media_cache_dir)
        .await
        .map_err(anyhow::Error::msg)?;
    stream_capped_media_response(&mut response, &path, max_bytes).await?;
    Ok(DownloadedFile {
        url: url.to_string(),
        path,
        content_type,
    })
}

#[cfg(feature = "chat-client")]
fn socks_media_client(host: &str, port: u16, timeout: Duration) -> anyhow::Result<reqwest::Client> {
    let key = SocksMediaClientKey {
        host: host.to_ascii_lowercase(),
        port,
        timeout_millis: u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX),
    };
    if let Some(client) = SOCKS_MEDIA_CLIENTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&key)
    {
        return Ok(client);
    }
    let proxy = reqwest::Proxy::all(format!("socks5h://{}:{}", key.host, key.port))?;
    let client = reqwest::Client::builder()
        .proxy(proxy)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if let Err(error) = validate_clearweb_media_redirect(attempt.previous(), attempt.url())
            {
                return attempt.error(error.to_string());
            }
            attempt.follow()
        }))
        .timeout(timeout)
        .user_agent("OMENbrowser_rs/0.1 OMENchat-media")
        .build()?;
    SOCKS_MEDIA_CLIENTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key, client.clone());
    Ok(client)
}

#[cfg(feature = "chat-client")]
fn validate_clearweb_media_redirect(
    previous: &[reqwest::Url],
    next: &reqwest::Url,
) -> anyhow::Result<()> {
    if previous.len() >= 5 {
        anyhow::bail!("clearweb media redirect limit exceeded");
    }
    validate_clearweb_media_url(next)?;
    if previous
        .last()
        .is_some_and(|url| url.scheme() == "https" && next.scheme() == "http")
    {
        anyhow::bail!("clearweb media redirect refused HTTPS downgrade");
    }
    Ok(())
}

#[cfg(feature = "chat-client")]
fn validate_clearweb_media_url(url: &reqwest::Url) -> anyhow::Result<()> {
    if !matches!(url.scheme(), "http" | "https") {
        anyhow::bail!("clearweb media URL must use HTTP or HTTPS");
    }
    if !url.username().is_empty() || url.password().is_some() {
        anyhow::bail!("clearweb media URL credentials are not allowed");
    }
    let host = url
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("clearweb media URL has no host"))?;
    let unsafe_host = match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(address)) => unsafe_ipv4(address),
        Ok(std::net::IpAddr::V6(address)) => unsafe_ipv6(address),
        Err(_) => {
            let domain = host.trim_end_matches('.').to_ascii_lowercase();
            domain == "localhost"
                || domain.ends_with(".localhost")
                || domain.ends_with(".local")
                || !domain.contains('.')
        }
    };
    if unsafe_host {
        anyhow::bail!("clearweb media URL targets a local or special-use destination");
    }
    Ok(())
}

#[cfg(feature = "chat-client")]
fn unsafe_ipv4(address: std::net::Ipv4Addr) -> bool {
    let [a, b, c, _] = address.octets();
    a == 0
        || a == 10
        || a == 127
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 0)
        || (a == 192 && b == 168)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || a >= 224
}

#[cfg(feature = "chat-client")]
fn unsafe_ipv6(address: std::net::Ipv6Addr) -> bool {
    address.is_unspecified()
        || address.is_loopback()
        || address.is_multicast()
        || (address.segments()[0] & 0xfe00) == 0xfc00
        || (address.segments()[0] & 0xffc0) == 0xfe80
        || address.to_ipv4_mapped().is_some_and(unsafe_ipv4)
}

#[cfg(feature = "chat-client")]
fn enforce_media_content_length(content_length: Option<u64>, max_bytes: u64) -> anyhow::Result<()> {
    if content_length.is_some_and(|length| length > max_bytes) {
        anyhow::bail!("remote image exceeds {max_bytes} byte limit");
    }
    Ok(())
}

#[cfg(feature = "chat-client")]
async fn stream_capped_media_response(
    response: &mut reqwest::Response,
    path: &Path,
    max_bytes: u64,
) -> anyhow::Result<u64> {
    if tokio::fs::try_exists(path).await? {
        anyhow::bail!("remote media destination already exists");
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("remote media destination has no safe file name"))?;
    let sequence = SOCKS_MEDIA_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = path.with_file_name(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        sequence
    ));
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .await?;
    let result = async {
        let mut written = 0u64;
        while let Some(chunk) = response.chunk().await? {
            let chunk_bytes = u64::try_from(chunk.len())
                .map_err(|_| anyhow::anyhow!("media chunk length overflow"))?;
            let next = written
                .checked_add(chunk_bytes)
                .ok_or_else(|| anyhow::anyhow!("remote image byte count overflow"))?;
            if next > max_bytes {
                anyhow::bail!("remote image exceeds {max_bytes} byte limit");
            }
            file.write_all(&chunk).await?;
            written = next;
        }
        file.flush().await?;
        file.sync_all().await?;
        Ok(written)
    }
    .await;
    drop(file);
    if result.is_ok() {
        if let Err(error) = tokio::fs::rename(&temporary, path).await {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(error.into());
        }
    } else {
        let _ = tokio::fs::remove_file(&temporary).await;
    }
    result
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

    fn local_response(body: &'static [u8]) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("loopback listener");
        let address = listener.local_addr().expect("loopback address");
        let handle = std::thread::spawn(move || {
            use std::io::{Read, Write};

            let (mut stream, _) = listener.accept().expect("loopback request");
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request).expect("request bytes");
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: image/gif\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .expect("response headers");
            stream.write_all(body).expect("response body");
        });
        (format!("http://{address}/media.gif"), handle)
    }

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
        assert_eq!(
            clearweb_media_cache_filename("https://cdn.example.test/images/preview", "image/avif"),
            "preview.avif"
        );
    }

    #[test]
    fn clearweb_media_url_and_redirect_policy_blocks_local_and_downgrade_targets() {
        for allowed in [
            "https://example.com/image.gif",
            "http://1.1.1.1/image.gif",
            "https://examplehiddenservice.onion/image.gif",
        ] {
            assert!(
                validate_clearweb_media_url(&reqwest::Url::parse(allowed).expect("allowed URL"))
                    .is_ok(),
                "allowed URL rejected: {allowed}"
            );
        }
        for blocked in [
            "file:///tmp/image.gif",
            "http://user:password@example.com/image.gif",
            "http://localhost/image.gif",
            "http://service.local/image.gif",
            "http://intranet/image.gif",
            "http://127.0.0.1/image.gif",
            "http://10.0.0.1/image.gif",
            "http://169.254.1.1/image.gif",
            "http://192.168.1.1/image.gif",
            "http://[::1]/image.gif",
            "http://[fc00::1]/image.gif",
            "http://[fe80::1]/image.gif",
            "http://[::ffff:127.0.0.1]/image.gif",
        ] {
            assert!(
                validate_clearweb_media_url(&reqwest::Url::parse(blocked).expect("blocked URL"))
                    .is_err(),
                "unsafe URL accepted: {blocked}"
            );
        }

        let https = reqwest::Url::parse("https://example.com/start.gif").expect("HTTPS URL");
        let http = reqwest::Url::parse("http://example.com/final.gif").expect("HTTP URL");
        assert!(validate_clearweb_media_redirect(std::slice::from_ref(&https), &http).is_err());
        assert!(validate_clearweb_media_redirect(std::slice::from_ref(&http), &https).is_ok());
        assert!(validate_clearweb_media_redirect(&vec![https.clone(); 5], &https).is_err());
        let local = reqwest::Url::parse("https://127.0.0.1/final.gif").expect("local URL");
        assert!(validate_clearweb_media_redirect(std::slice::from_ref(&https), &local).is_err());
    }

    #[test]
    fn socks_media_client_cache_is_lru_and_item_bounded() {
        let mut cache = SocksMediaClientCache::new();
        let key = |index| SocksMediaClientKey {
            host: format!("proxy-{index}.example"),
            port: 9000 + index,
            timeout_millis: 30_000,
        };
        for index in 0..=SOCKS_MEDIA_CLIENT_CACHE_MAX_ITEMS as u16 {
            cache.insert(key(index), reqwest::Client::new());
        }
        assert_eq!(cache.clients.len(), SOCKS_MEDIA_CLIENT_CACHE_MAX_ITEMS);
        assert!(cache.get(&key(0)).is_none());
        assert!(cache.get(&key(1)).is_some());
        cache.insert(key(9), reqwest::Client::new());
        assert!(cache.get(&key(2)).is_none());
        assert!(cache.get(&key(1)).is_some());
    }

    #[tokio::test]
    async fn capped_media_stream_accepts_exact_limit_and_removes_oversize_partial() {
        assert!(enforce_media_content_length(Some(4), 4).is_ok());
        assert!(enforce_media_content_length(None, 4).is_ok());
        assert!(enforce_media_content_length(Some(5), 4).is_err());
        let root = std::env::temp_dir().join(format!(
            "omenchat-clearweb-stream-{}-{}",
            std::process::id(),
            crate::app::current_epoch_ms()
        ));
        std::fs::create_dir_all(&root).expect("isolated stream root");

        let (url, server) = local_response(b"abcd");
        let mut response = reqwest::Client::new()
            .get(url)
            .send()
            .await
            .expect("exact response");
        let exact_path = root.join("exact.gif");
        assert_eq!(
            stream_capped_media_response(&mut response, &exact_path, 4)
                .await
                .expect("exact limit"),
            4
        );
        assert_eq!(std::fs::read(&exact_path).expect("exact file"), b"abcd");
        assert_eq!(
            std::fs::read_dir(&root)
                .expect("temporary listing")
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
                .count(),
            0
        );
        server.join().expect("exact server");

        let (url, server) = local_response(b"abcde");
        let mut response = reqwest::Client::new()
            .get(url)
            .send()
            .await
            .expect("oversize response");
        let oversize_path = root.join("oversize.gif");
        let error = stream_capped_media_response(&mut response, &oversize_path, 4)
            .await
            .expect_err("oversize stream");
        assert!(error.to_string().contains("byte limit"));
        assert!(!oversize_path.exists());
        assert_eq!(
            std::fs::read_dir(&root)
                .expect("failure temporary listing")
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
                .count(),
            0
        );
        server.join().expect("oversize server");

        let existing_path = root.join("existing.gif");
        std::fs::write(&existing_path, b"previous").expect("existing fixture");
        let (url, server) = local_response(b"replacement");
        let mut response = reqwest::Client::new()
            .get(url)
            .send()
            .await
            .expect("replacement response");
        let error = stream_capped_media_response(&mut response, &existing_path, 32)
            .await
            .expect_err("existing destination refusal");
        assert!(error.to_string().contains("already exists"));
        assert_eq!(
            std::fs::read(&existing_path).expect("preserved existing file"),
            b"previous"
        );
        server.join().expect("replacement server");
        std::fs::remove_dir_all(root).expect("remove isolated stream root");
    }
}
