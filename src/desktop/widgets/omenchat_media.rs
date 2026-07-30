use std::collections::HashMap;
use std::path::{Path, PathBuf};

use iced::widget::{container, image, row, Text};
use iced::{ContentFit, Element, Length};

use crate::browser::BrowserAddress;
use crate::media::{
    decide_remote_media, extract_link_candidates, RemoteMediaContext, RemoteMediaDecision,
    RemoteMediaTransport,
};

use super::super::{
    safe_timeline_text, ChatSessionId, ChatTimelineUpload, ExternalBrowserMessage, Message,
    OmenChatMediaLoadState, OmenChatMessage, RoomId, ICON_DOWNLOAD, ICON_OMENCHAT_RECONNECT,
    ICON_OPEN, OMENCHAT_INLINE_MEDIA_MAX_HEIGHT, OMENCHAT_INLINE_MEDIA_MAX_WIDTH,
};
use super::human_bytes;
use super::inline_icon_button_owned;
pub(in crate::desktop) use super::omenchat_media_format::cached_media_is_animated_gif;
use super::omenchat_media_format::inline_media_size;
pub(in crate::desktop) use super::omenchat_media_format::{
    gif_image_descriptor_count, image_dimensions_from_bytes,
};

#[cfg(feature = "omenchat-room-media-policy-qualification")]
const QUALIFICATION_OMENCHAT_UPLOAD_PATH_ENV: &str =
    "OMENBROWSER_QUALIFICATION_OMENCHAT_UPLOAD_PATH";
#[cfg(test)]
pub(in crate::desktop) use super::omenchat_media_format::{
    read_media_header_bytes, scale_media_dimensions,
};

pub(in crate::desktop) fn omenchat_upload_cache_key(
    session_id: ChatSessionId,
    resource_id: &str,
) -> String {
    format!("upload:{session_id}:{resource_id}")
}

pub(in crate::desktop) fn pick_omenchat_upload_file() -> Result<Option<PathBuf>, String> {
    #[cfg(feature = "omenchat-room-media-policy-qualification")]
    if let Some(path) =
        qualification_omenchat_upload_path(std::env::var_os(QUALIFICATION_OMENCHAT_UPLOAD_PATH_ENV))
    {
        return Ok(Some(path));
    }
    Ok(rfd::FileDialog::new()
        .set_title("Select OMENchat upload")
        .pick_file())
}

#[cfg(feature = "omenchat-room-media-policy-qualification")]
fn qualification_omenchat_upload_path(value: Option<std::ffi::OsString>) -> Option<PathBuf> {
    value.filter(|value| !value.is_empty()).map(PathBuf::from)
}

#[derive(Clone)]
pub(in crate::desktop) struct OmenChatMediaHint {
    pub(in crate::desktop) label: String,
    pub(in crate::desktop) caption: Option<String>,
    pub(in crate::desktop) open_url: Option<String>,
    pub(in crate::desktop) open_path: Option<String>,
    pub(in crate::desktop) load_url: Option<String>,
    pub(in crate::desktop) image_path: Option<String>,
    pub(in crate::desktop) animated: bool,
}

pub(in crate::desktop) fn omenchat_media_hints(
    text: &str,
    settings: &crate::storage::settings::ClearwebPrivacySettings,
    detected_proxy: Option<&(String, u16)>,
    server_trusted: bool,
    cache: &HashMap<String, OmenChatMediaLoadState>,
) -> Vec<OmenChatMediaHint> {
    let detected_socks_proxy = detected_proxy.map(|(host, port)| (host.as_str(), *port));
    extract_link_candidates(text)
        .into_iter()
        .filter_map(|url| {
            if let Some(state) = cache.get(&url) {
                return Some(OmenChatMediaHint {
                    label: omenchat_media_state_label(state),
                    caption: omenchat_media_caption(&url, server_trusted),
                    open_url: None,
                    open_path: omenchat_media_state_open_path(state),
                    load_url: None,
                    image_path: omenchat_media_state_image_path(state),
                    animated: omenchat_media_state_is_animated(state),
                });
            }
            let decision = decide_remote_media(RemoteMediaContext {
                url: &url,
                settings,
                detected_socks_proxy,
            });
            if crate::media::is_clearweb_url(&url)
                && !server_trusted
                && matches!(
                    decision,
                    RemoteMediaDecision::AutoInline {
                        transport: RemoteMediaTransport::Socks5 { .. },
                        ..
                    }
                )
            {
                let label = if let Some((host, port)) = detected_socks_proxy {
                    format!(
                        "media blocked: untrusted OMENchat server; Load uses SOCKS5 {host}:{port}"
                    )
                } else {
                    "media blocked: untrusted OMENchat server".into()
                };
                return Some(OmenChatMediaHint {
                    label,
                    caption: None,
                    open_url: None,
                    open_path: None,
                    load_url: Some(url),
                    image_path: None,
                    animated: false,
                });
            }
            match decision {
                RemoteMediaDecision::AutoInline { transport, .. } => {
                    let load_url = matches!(
                        transport,
                        RemoteMediaTransport::Reticulum | RemoteMediaTransport::Socks5 { .. }
                    )
                    .then(|| url.clone());
                    let open_url = matches!(transport, RemoteMediaTransport::ExternalBrowser)
                        .then(|| url.clone());
                    Some(OmenChatMediaHint {
                        label: format!("media: {}", omenchat_media_transport_label(&transport)),
                        caption: None,
                        open_url,
                        open_path: None,
                        load_url,
                        image_path: None,
                        animated: false,
                    })
                }
                RemoteMediaDecision::ManualInline { reason, .. } => Some(OmenChatMediaHint {
                    label: format!("media blocked: {reason}"),
                    caption: None,
                    open_url: Some(url),
                    open_path: None,
                    load_url: None,
                    image_path: None,
                    animated: false,
                }),
                RemoteMediaDecision::ExternalPrompt { reason, .. } => Some(OmenChatMediaHint {
                    label: reason,
                    caption: None,
                    open_url: Some(url),
                    open_path: None,
                    load_url: None,
                    image_path: None,
                    animated: false,
                }),
                RemoteMediaDecision::Unsupported { .. } => None,
            }
        })
        .collect()
}

pub(in crate::desktop) fn omenchat_media_state_label(state: &OmenChatMediaLoadState) -> String {
    match state {
        OmenChatMediaLoadState::Loading {
            message,
            received,
            total,
        } => match (received, total) {
            (Some(received), Some(total)) if *total > 0 => format!(
                "media loading: {} / {}",
                human_bytes(*received),
                human_bytes(*total)
            ),
            _ => format!("media: {message}"),
        },
        OmenChatMediaLoadState::Cached { .. } => String::new(),
        OmenChatMediaLoadState::Failed { message } => format!("media failed: {message}"),
    }
}

pub(in crate::desktop) fn omenchat_upload_state_label(state: &OmenChatMediaLoadState) -> String {
    match state {
        OmenChatMediaLoadState::Loading {
            message,
            received,
            total,
        } => match (received, total) {
            (Some(received), Some(total)) if *total > 0 => format!(
                "loading: {} / {}",
                human_bytes(*received),
                human_bytes(*total)
            ),
            _ if message.trim().is_empty() => "loading".into(),
            _ if message.trim() == "requested upload from server" => "waiting for server".into(),
            _ => format!("loading: {}", compact_status_message(message.trim(), 72)),
        },
        OmenChatMediaLoadState::Cached { .. } => String::new(),
        OmenChatMediaLoadState::Failed { message } => {
            format!("failed: {}", compact_status_message(message.trim(), 72))
        }
    }
}

pub(in crate::desktop) fn compact_status_message(message: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if message.chars().count() <= max_chars {
        return message.to_owned();
    }
    if max_chars <= 3 {
        return message.chars().take(max_chars).collect();
    }
    let mut compact: String = message.chars().take(max_chars - 3).collect();
    compact.push_str("...");
    compact
}

pub(in crate::desktop) fn omenchat_media_loading_state(message: &str) -> OmenChatMediaLoadState {
    OmenChatMediaLoadState::Loading {
        message: message.to_owned(),
        received: None,
        total: None,
    }
}

pub(in crate::desktop) fn omenchat_media_state_image_path(
    state: &OmenChatMediaLoadState,
) -> Option<String> {
    match state {
        OmenChatMediaLoadState::Cached {
            path, content_type, ..
        } if content_type.to_ascii_lowercase().starts_with("image/") => Some(path.clone()),
        _ => None,
    }
}

pub(in crate::desktop) fn omenchat_media_state_open_path(
    state: &OmenChatMediaLoadState,
) -> Option<String> {
    match state {
        OmenChatMediaLoadState::Cached { path, animated, .. } if *animated => Some(path.clone()),
        _ => None,
    }
}

pub(in crate::desktop) fn omenchat_media_state_is_animated(state: &OmenChatMediaLoadState) -> bool {
    matches!(state, OmenChatMediaLoadState::Cached { animated: true, .. })
}

pub(in crate::desktop) fn omenchat_media_caption(
    url: &str,
    server_trusted: bool,
) -> Option<String> {
    if crate::media::is_clearweb_url(url) {
        let source = clearweb_media_host(url).unwrap_or("clearweb");
        let trust = if server_trusted { "trusted" } else { "manual" };
        return Some(format!("{trust} clearweb image from {source}"));
    }
    if url.trim().to_ascii_lowercase().starts_with("nomadnet://")
        || BrowserAddress::parse(url).is_some()
    {
        return Some("Reticulum/NomadNet image".into());
    }
    None
}

pub(in crate::desktop) fn clearweb_media_host(url: &str) -> Option<&str> {
    url.split_once("://")
        .map(|(_, rest)| rest)
        .and_then(|rest| rest.split(['/', '?', '#']).next())
        .map(|host| host.trim())
        .filter(|host| !host.is_empty())
}

fn omenchat_media_transport_label(transport: &RemoteMediaTransport) -> String {
    match transport {
        RemoteMediaTransport::Reticulum => "inline via Reticulum/NomadNet".into(),
        RemoteMediaTransport::Socks5 { host, port } => {
            format!("inline via SOCKS5 {host}:{port}")
        }
        RemoteMediaTransport::ExternalBrowser => "external browser required".into(),
    }
}

pub(in crate::desktop) fn omenchat_upload_action_row<'a>(
    line: Text<'a>,
    upload: ChatTimelineUpload,
    state: Option<OmenChatMediaLoadState>,
) -> Element<'a, Message> {
    let mut upload_line = row![line].spacing(6).align_y(iced::Alignment::Center);
    match state.as_ref() {
        Some(OmenChatMediaLoadState::Cached { path, .. }) => {
            upload_line = upload_line.push(inline_icon_button_owned(
                ICON_OPEN,
                "Open attachment",
                Message::OmenChat(OmenChatMessage::OpenCachedMedia(path.clone())),
            ));
        }
        Some(state @ OmenChatMediaLoadState::Loading { .. }) => {
            upload_line =
                upload_line.push(safe_timeline_text(omenchat_upload_state_label(state), 11));
        }
        Some(state @ OmenChatMediaLoadState::Failed { .. }) => {
            upload_line =
                upload_line.push(safe_timeline_text(omenchat_upload_state_label(state), 11));
            upload_line = upload_line.push(inline_icon_button_owned(
                ICON_DOWNLOAD,
                "Retry attachment download",
                Message::OmenChat(OmenChatMessage::FetchUploadResource {
                    session_id: upload.session_id,
                    resource_id: upload.resource_id.clone(),
                }),
            ));
        }
        None => {
            upload_line = upload_line.push(inline_icon_button_owned(
                ICON_DOWNLOAD,
                "Download attachment",
                Message::OmenChat(OmenChatMessage::FetchUploadResource {
                    session_id: upload.session_id,
                    resource_id: upload.resource_id.clone(),
                }),
            ));
        }
    }
    upload_line.wrap().into()
}

pub(in crate::desktop) fn omenchat_resend_action_row<'a>(
    line: Text<'a>,
    session_id: ChatSessionId,
    room_id: RoomId,
    event_id: u64,
    body: String,
    action: bool,
) -> Element<'a, Message> {
    row![
        line,
        inline_icon_button_owned(
            ICON_OMENCHAT_RECONNECT,
            "Resend message",
            Message::OmenChat(OmenChatMessage::ResendLocalEcho {
                session_id,
                room_id,
                event_id,
                body,
                action,
            }),
        )
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center)
    .wrap()
    .into()
}

pub(in crate::desktop) fn omenchat_media_hint_row<'a>(
    hint: OmenChatMediaHint,
) -> Option<Element<'a, Message>> {
    let mut hint_row = row![].spacing(8).align_y(iced::Alignment::Center);
    let mut has_hint_row = false;
    if !hint.label.is_empty() {
        hint_row = hint_row.push(safe_timeline_text(hint.label.to_string(), 11));
        has_hint_row = true;
    }
    if let Some(url) = hint.open_url {
        hint_row = hint_row.push(inline_icon_button_owned(
            ICON_OPEN,
            "Open",
            Message::ExternalBrowser(ExternalBrowserMessage::PromptUrl(url)),
        ));
        has_hint_row = true;
    }
    if let Some(path) = hint.open_path {
        hint_row = hint_row.push(inline_icon_button_owned(
            ICON_OPEN,
            "Open",
            Message::OmenChat(OmenChatMessage::OpenCachedMedia(path)),
        ));
        has_hint_row = true;
    }
    if let Some(url) = hint.load_url {
        hint_row = hint_row.push(inline_icon_button_owned(
            ICON_OPEN,
            "Load",
            Message::OmenChat(OmenChatMessage::LoadMedia(url)),
        ));
        has_hint_row = true;
    }
    has_hint_row.then(|| hint_row.wrap().into())
}

pub(in crate::desktop) fn omenchat_media_hint_preview<'a>(
    path: &str,
    animated: bool,
    frames: Option<&'a super::super::omenchat_desktop_state::OmenChatGifFrames>,
) -> Option<Element<'a, Message>> {
    let media = omenchat_inline_media_element(path, animated, frames);
    Some(container(media).width(Length::Fill).into())
}

pub(in crate::desktop) fn omenchat_upload_preview<'a>(
    state: &OmenChatMediaLoadState,
    frames: Option<&'a super::super::omenchat_desktop_state::OmenChatGifFrames>,
) -> Option<Element<'a, Message>> {
    let path = omenchat_media_state_image_path(state)?;
    let media =
        omenchat_inline_media_element(&path, omenchat_media_state_is_animated(state), frames);
    Some(container(media).width(Length::Fill).into())
}

pub(in crate::desktop) fn omenchat_inline_media_element<'a>(
    path: &str,
    animated: bool,
    frames: Option<&'a super::super::omenchat_desktop_state::OmenChatGifFrames>,
) -> Element<'a, Message> {
    let (width, height) = omenchat_inline_media_size(Path::new(path))
        .unwrap_or((OMENCHAT_INLINE_MEDIA_MAX_WIDTH, 240.0));
    #[cfg(feature = "chat-client-gif")]
    {
        if animated {
            if let Some(frames) = frames {
                return iced_gif::Gif::new(frames)
                    .width(Length::Fixed(width))
                    .height(Length::Fixed(height))
                    .content_fit(ContentFit::Contain)
                    .into();
            }
        }
    }
    #[cfg(not(feature = "chat-client-gif"))]
    let _ = (animated, frames);
    image(path.to_owned())
        .width(Length::Fixed(width))
        .height(Length::Fixed(height))
        .content_fit(ContentFit::Contain)
        .into()
}

pub(in crate::desktop) fn omenchat_inline_media_size(path: &Path) -> Option<(f32, f32)> {
    inline_media_size(
        path,
        OMENCHAT_INLINE_MEDIA_MAX_WIDTH,
        OMENCHAT_INLINE_MEDIA_MAX_HEIGHT,
    )
}

#[cfg(all(test, feature = "chat-client"))]
#[path = "omenchat_media_tests.rs"]
mod tests;
