use crate::browser::BrowserAddress;
use crate::storage::settings::ClearwebPrivacySettings;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteMediaKind {
    Image,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteMediaTransport {
    Reticulum,
    Socks5 { host: String, port: u16 },
    ExternalBrowser,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteMediaDecision {
    AutoInline {
        transport: RemoteMediaTransport,
        kind: RemoteMediaKind,
    },
    ManualInline {
        transport: RemoteMediaTransport,
        kind: RemoteMediaKind,
        reason: String,
    },
    ExternalPrompt {
        transport: RemoteMediaTransport,
        reason: String,
    },
    Unsupported {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteMediaContext<'a> {
    pub url: &'a str,
    pub settings: &'a ClearwebPrivacySettings,
    pub detected_socks_proxy: Option<(&'a str, u16)>,
}

pub fn decide_remote_media(context: RemoteMediaContext<'_>) -> RemoteMediaDecision {
    let url = context.url.trim();
    if url.is_empty() {
        return RemoteMediaDecision::Unsupported {
            reason: "empty media URL".into(),
        };
    }

    let kind = media_kind(url);
    let is_clearweb = is_clearweb_url(url);
    if !is_clearweb && is_reticulum_media_url(url) {
        return match kind {
            RemoteMediaKind::Image => RemoteMediaDecision::AutoInline {
                transport: RemoteMediaTransport::Reticulum,
                kind,
            },
            RemoteMediaKind::Other => RemoteMediaDecision::ExternalPrompt {
                transport: RemoteMediaTransport::Reticulum,
                reason: "non-image Reticulum/NomadNet media should open through the normal browser/download flow"
                    .into(),
            },
        };
    }

    if !is_clearweb {
        return RemoteMediaDecision::Unsupported {
            reason: "unsupported media URL scheme".into(),
        };
    }

    let is_onion = clearweb_host(url).is_some_and(|host| host.ends_with(".onion"));
    if kind != RemoteMediaKind::Image {
        return RemoteMediaDecision::ExternalPrompt {
            transport: RemoteMediaTransport::ExternalBrowser,
            reason: if is_onion {
                "onion links require a Tor-capable external browser".into()
            } else {
                "clearweb non-image links open through the external browser prompt".into()
            },
        };
    }

    if let (true, true, Some((host, port))) = (
        context.settings.remote_media_enabled,
        context.settings.socks_proxy_enabled,
        context.detected_socks_proxy,
    ) {
        return RemoteMediaDecision::AutoInline {
            transport: RemoteMediaTransport::Socks5 {
                host: host.into(),
                port,
            },
            kind,
        };
    }

    let reason = if !context.settings.remote_media_enabled {
        "clearweb image previews are disabled; click to load/open explicitly"
    } else if !context.settings.socks_proxy_enabled {
        "SOCKS5/Tor preference is disabled; click to open explicitly"
    } else if is_onion {
        "Tor/SOCKS5 proxy is not reachable; onion image was not fetched"
    } else {
        "SOCKS5/Tor proxy is not reachable; clearweb image was not fetched"
    };

    let transport = if context.settings.socks_proxy_enabled {
        RemoteMediaTransport::Socks5 {
            host: context.settings.socks_proxy_host.clone(),
            port: context.settings.socks_proxy_port,
        }
    } else {
        RemoteMediaTransport::ExternalBrowser
    };

    RemoteMediaDecision::ManualInline {
        transport,
        kind,
        reason: reason.into(),
    }
}

pub fn media_kind(url: &str) -> RemoteMediaKind {
    let path = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .trim()
        .to_ascii_lowercase();
    if matches!(
        path.rsplit('.').next(),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "avif")
    ) {
        RemoteMediaKind::Image
    } else {
        RemoteMediaKind::Other
    }
}

pub fn is_clearweb_url(url: &str) -> bool {
    let lowered = url.trim().to_ascii_lowercase();
    lowered.starts_with("http://") || lowered.starts_with("https://")
}

pub fn extract_link_candidates(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter_map(|part| {
            let candidate = part.trim_matches(|ch: char| {
                matches!(
                    ch,
                    '<' | '>' | '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}' | ','
                )
            });
            let candidate = candidate.trim_end_matches(['.', ',', ';', '!', '?']);
            if is_clearweb_url(candidate) || is_reticulum_media_url(candidate) {
                Some(candidate.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn is_reticulum_media_url(url: &str) -> bool {
    let trimmed = url.trim();
    if trimmed.to_ascii_lowercase().starts_with("nomadnet://") {
        return true;
    }
    let Some(address) = BrowserAddress::parse(trimmed) else {
        return false;
    };
    is_reticulum_destination_hash(&address.destination)
}

fn is_reticulum_destination_hash(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.len() == 32 && trimmed.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn clearweb_host(url: &str) -> Option<String> {
    let trimmed = url.trim();
    let (_, rest) = trimmed.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?.trim();
    let host = authority
        .rsplit_once('@')
        .map(|(_, host)| host)
        .unwrap_or(authority)
        .split(':')
        .next()?
        .trim()
        .trim_matches(['[', ']'])
        .to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_RETICULUM_HASH: &str = "00112233445566778899aabbccddeeff";

    fn settings(remote_media_enabled: bool) -> ClearwebPrivacySettings {
        ClearwebPrivacySettings {
            remote_media_enabled,
            ..ClearwebPrivacySettings::default()
        }
    }

    #[test]
    fn reticulum_images_auto_inline_without_clearweb_proxy() {
        let settings = settings(false);
        let decision = decide_remote_media(RemoteMediaContext {
            url: &format!("{FIXTURE_RETICULUM_HASH}:/files/logo.png"),
            settings: &settings,
            detected_socks_proxy: None,
        });

        assert_eq!(
            decision,
            RemoteMediaDecision::AutoInline {
                transport: RemoteMediaTransport::Reticulum,
                kind: RemoteMediaKind::Image,
            }
        );
    }

    #[test]
    fn clearweb_images_do_not_auto_load_without_remote_media_permission() {
        let settings = settings(false);
        let decision = decide_remote_media(RemoteMediaContext {
            url: "https://example.com/cat.png",
            settings: &settings,
            detected_socks_proxy: Some(("127.0.0.1", 9150)),
        });

        assert!(matches!(
            decision,
            RemoteMediaDecision::ManualInline {
                transport: RemoteMediaTransport::Socks5 { .. },
                kind: RemoteMediaKind::Image,
                ..
            }
        ));
    }

    #[test]
    fn clearweb_images_auto_load_through_reachable_socks_when_enabled() {
        let settings = settings(true);
        let decision = decide_remote_media(RemoteMediaContext {
            url: "https://example.com/cat.webp?size=small",
            settings: &settings,
            detected_socks_proxy: Some(("127.0.0.1", 9150)),
        });

        assert_eq!(
            decision,
            RemoteMediaDecision::AutoInline {
                transport: RemoteMediaTransport::Socks5 {
                    host: "127.0.0.1".into(),
                    port: 9150,
                },
                kind: RemoteMediaKind::Image,
            }
        );
    }

    #[test]
    fn onion_images_never_fall_back_to_direct_fetch() {
        let settings = settings(true);
        let decision = decide_remote_media(RemoteMediaContext {
            url: "http://examplehiddenservice.onion/image.jpg",
            settings: &settings,
            detected_socks_proxy: None,
        });

        assert!(matches!(
            decision,
            RemoteMediaDecision::ManualInline {
                transport: RemoteMediaTransport::Socks5 { .. },
                reason,
                ..
            } if reason.contains("onion")
        ));
    }

    #[test]
    fn extracts_clearweb_and_reticulum_links_from_chat_text() {
        let links = extract_link_candidates(&format!(
            r#"look <https://example.org/a.png>, {FIXTURE_RETICULUM_HASH}:/files/pic.jpg and "nope""#
        ));
        assert_eq!(
            links,
            vec![
                "https://example.org/a.png".to_string(),
                format!("{FIXTURE_RETICULUM_HASH}:/files/pic.jpg")
            ]
        );
    }

    #[test]
    fn chat_colon_reactions_are_not_reticulum_media_links() {
        let links = extract_link_candidates("OMENbrowser_dev o: :o x: not-a-node:/file.png");
        assert!(links.is_empty());
    }

    #[test]
    fn explicit_nomadnet_scheme_still_counts_as_reticulum_media() {
        let links = extract_link_candidates(
            "portal nomadnet://00112233445566778899aabbccddeeff/page/index.mu",
        );
        assert_eq!(
            links,
            vec!["nomadnet://00112233445566778899aabbccddeeff/page/index.mu"]
        );
    }
}
