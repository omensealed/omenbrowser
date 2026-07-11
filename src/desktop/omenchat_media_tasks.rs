use std::path::{Path, PathBuf};
use std::time::Duration;

use iced::Task;

use crate::chat::{ChatClientEvent, ChatClientRequest, ChatSessionId};
use crate::media::{
    decide_remote_media, RemoteMediaContext, RemoteMediaDecision, RemoteMediaTransport,
};
use crate::storage::files::next_available_download_path;

use super::{
    cached_media_is_animated_gif, fetch_clearweb_media_over_socks, omenchat_media_loading_state,
    omenchat_upload_cache_key, omenchat_upload_content_type, pick_omenchat_upload_file, DesktopApp,
    Message, OmenChatDraftCommandResult, OmenChatMediaLoadState,
};

impl DesktopApp {
    pub(super) fn update_open_cached_omenchat_media(&mut self, path: String) {
        self.open_local_file(PathBuf::from(path));
    }

    pub(super) fn update_load_omenchat_media(&mut self, url: String) -> Task<Message> {
        self.load_omenchat_media_task(url)
    }

    pub(super) fn update_fetch_omenchat_upload_resource(
        &mut self,
        session_id: ChatSessionId,
        resource_id: String,
    ) {
        self.omenchat.omenchat_media_cache.insert(
            omenchat_upload_cache_key(session_id, &resource_id),
            OmenChatMediaLoadState::Loading {
                message: "requested upload from server".into(),
                received: None,
                total: None,
            },
        );
        let room = self
            .omenchat
            .chat_client
            .session(session_id)
            .map(|session| session.active_room.name.clone())
            .unwrap_or_else(|| "lobby".into());
        let events = self.handle_omenchat_request(ChatClientRequest::RequestUpload {
            session_id,
            room,
            resource_id: resource_id.clone(),
        });
        self.apply_omenchat_client_events_status(&events);
        if let Some(message) = events.iter().find_map(|event| match event {
            ChatClientEvent::Error {
                session_id: Some(error_session_id),
                message,
            } if *error_session_id == session_id => Some(message.clone()),
            _ => None,
        }) {
            self.omenchat.omenchat_media_cache.insert(
                omenchat_upload_cache_key(session_id, &resource_id),
                OmenChatMediaLoadState::Failed { message },
            );
        }
    }

    pub(super) fn update_pick_omenchat_upload(
        &mut self,
        session_id: ChatSessionId,
    ) -> Task<Message> {
        self.set_omenchat_session_status(session_id, "opening file picker".into());
        Task::perform(
            async move {
                let result = tokio::task::spawn_blocking(pick_omenchat_upload_file)
                    .await
                    .map_err(|error| error.to_string())
                    .and_then(|result| result);
                (session_id, result)
            },
            |(session_id, result)| Message::OmenChatUploadPicked { session_id, result },
        )
    }

    pub(super) fn update_omenchat_upload_picked(
        &mut self,
        session_id: ChatSessionId,
        result: Result<Option<PathBuf>, String>,
    ) {
        match result {
            Ok(Some(path)) => match self.send_omenchat_upload_path(session_id, &path) {
                OmenChatDraftCommandResult::HandledClear => {
                    self.omenchat.chat_drafts.insert(session_id, String::new());
                }
                OmenChatDraftCommandResult::HandledKeep
                | OmenChatDraftCommandResult::NotCommand => {}
            },
            Ok(None) => {
                self.set_omenchat_session_status(session_id, "upload picker cancelled".into());
            }
            Err(error) => {
                self.set_omenchat_session_status(
                    session_id,
                    format!("upload picker failed: {error}"),
                );
            }
        }
    }

    pub(super) fn update_omenchat_gif_frames_loaded(
        &mut self,
        path: String,
        result: Result<Vec<u8>, String>,
    ) {
        match result {
            Ok(bytes) => match iced_gif::Frames::from_bytes(bytes) {
                Ok(frames) => {
                    self.omenchat
                        .omenchat_gif_frames
                        .insert(path.clone(), frames);
                    self.app.status.task = format!("OMENchat animated GIF ready: {path}");
                }
                Err(error) => {
                    self.app.status.task =
                        format!("OMENchat GIF animation decode failed for {path}: {error}");
                }
            },
            Err(error) => {
                self.app.status.task =
                    format!("OMENchat GIF animation load failed for {path}: {error}");
            }
        }
    }

    pub(super) fn update_omenchat_media_loaded(
        &mut self,
        url: String,
        result: Result<crate::browser::DownloadedFile, String>,
    ) -> Task<Message> {
        match result {
            Ok(file) => {
                let path = file.path.display().to_string();
                let animated = cached_media_is_animated_gif(&file.path, &file.content_type);
                self.omenchat.omenchat_media_cache.insert(
                    url.clone(),
                    OmenChatMediaLoadState::Cached {
                        path: path.clone(),
                        content_type: file.content_type,
                        animated,
                    },
                );
                self.app.status.task = if animated {
                    format!("OMENchat animated GIF cached: {path}")
                } else {
                    format!("OMENchat media cached: {path}")
                };
                if animated {
                    return self.load_omenchat_gif_frames_task(path);
                }
            }
            Err(error) => {
                self.omenchat.omenchat_media_cache.insert(
                    url.clone(),
                    OmenChatMediaLoadState::Failed {
                        message: error.clone(),
                    },
                );
                self.app.status.task = format!("OMENchat media load failed: {error}");
            }
        }
        Task::none()
    }

    pub(in crate::desktop) fn load_omenchat_media_task(&mut self, url: String) -> Task<Message> {
        let detected_socks_proxy = self
            .clearweb
            .clearweb_proxy_endpoint
            .as_ref()
            .map(|(host, port)| (host.as_str(), *port));
        let decision = decide_remote_media(RemoteMediaContext {
            url: &url,
            settings: &self.app.settings.clearweb,
            detected_socks_proxy,
        });

        match decision {
            RemoteMediaDecision::AutoInline {
                transport: RemoteMediaTransport::Reticulum,
                ..
            } => {
                self.omenchat.omenchat_media_cache.insert(
                    url.clone(),
                    omenchat_media_loading_state("loading over Reticulum/NomadNet"),
                );
                self.app.status.task = "loading OMENchat media over Reticulum/NomadNet".into();
                let runtime = self.app.runtime.clone();
                let media_cache_dir = self.app.paths.cache_dir.join("omenchat-media");
                Task::perform(
                    async move {
                        let result = runtime
                            .download_file(
                                &url,
                                &media_cache_dir,
                                crate::runtime::CancellationToken::new(),
                            )
                            .await
                            .map_err(|error| error.to_string());
                        (url, result)
                    },
                    |(url, result)| Message::OmenChatMediaLoaded { url, result },
                )
            }
            RemoteMediaDecision::AutoInline {
                transport: RemoteMediaTransport::Socks5 { host, port },
                ..
            } => {
                self.omenchat.omenchat_media_cache.insert(
                    url.clone(),
                    omenchat_media_loading_state(&format!("loading over SOCKS5 {host}:{port}")),
                );
                self.app.status.task = format!("loading OMENchat media over SOCKS5 {host}:{port}");
                let media_cache_dir = self.app.paths.cache_dir.join("omenchat-media");
                Task::perform(
                    async move {
                        let result = fetch_clearweb_media_over_socks(
                            &url,
                            &media_cache_dir,
                            &host,
                            port,
                            Duration::from_secs(30),
                        )
                        .await
                        .map_err(|error| error.to_string());
                        (url, result)
                    },
                    |(url, result)| Message::OmenChatMediaLoaded { url, result },
                )
            }
            RemoteMediaDecision::AutoInline {
                transport: RemoteMediaTransport::ExternalBrowser,
                ..
            } => {
                self.prompt_external_url_if_needed(url, None);
                Task::none()
            }
            RemoteMediaDecision::ManualInline { .. }
            | RemoteMediaDecision::ExternalPrompt { .. } => {
                self.prompt_external_url_if_needed(url, None);
                Task::none()
            }
            RemoteMediaDecision::Unsupported { reason } => {
                self.app.status.task = format!("OMENchat media not loaded: {reason}");
                Task::none()
            }
        }
    }

    pub(in crate::desktop) fn load_omenchat_gif_frames_task(
        &mut self,
        path: String,
    ) -> Task<Message> {
        Task::perform(
            async move {
                let result = tokio::fs::read(&path)
                    .await
                    .map_err(|error| error.to_string());
                (path, result)
            },
            |(path, result)| Message::OmenChatGifFramesLoaded { path, result },
        )
    }

    pub(in crate::desktop) fn cache_omenchat_upload_resource(
        &mut self,
        session_id: ChatSessionId,
        resource_id: &str,
        filename: &str,
        content_type: Option<&str>,
        bytes: &[u8],
    ) -> anyhow::Result<String> {
        let cache_dir = self.app.paths.cache_dir.join("omenchat-media");
        std::fs::create_dir_all(&cache_dir)?;
        let path = next_available_download_path(&cache_dir, filename)?;
        std::fs::write(&path, bytes)?;
        let path_label = path.display().to_string();
        let content_type = content_type
            .map(ToOwned::to_owned)
            .or_else(|| omenchat_upload_content_type(filename))
            .unwrap_or_else(|| "application/octet-stream".into());
        let animated = cached_media_is_animated_gif(&path, &content_type);
        if animated {
            self.cache_omenchat_uploaded_gif_frames(&path_label, bytes);
        }
        self.omenchat.omenchat_media_cache.insert(
            omenchat_upload_cache_key(session_id, resource_id),
            OmenChatMediaLoadState::Cached {
                path: path_label.clone(),
                content_type,
                animated,
            },
        );
        Ok(path_label)
    }

    pub(in crate::desktop) fn cache_omenchat_upload_source_file(
        &mut self,
        session_id: ChatSessionId,
        resource_id: &str,
        filename: &str,
        source_path: &Path,
    ) -> anyhow::Result<String> {
        let cache_dir = self.app.paths.cache_dir.join("omenchat-media");
        std::fs::create_dir_all(&cache_dir)?;
        let path = next_available_download_path(&cache_dir, filename)?;
        std::fs::copy(source_path, &path)?;
        let content_type = omenchat_upload_content_type(filename)
            .unwrap_or_else(|| "application/octet-stream".into());
        let path_label = path.display().to_string();
        let animated = cached_media_is_animated_gif(&path, &content_type);
        if animated {
            match std::fs::read(&path) {
                Ok(bytes) => self.cache_omenchat_uploaded_gif_frames(&path_label, &bytes),
                Err(error) => tracing::warn!(
                    path = path_label,
                    %error,
                    "failed to read cached OMENchat upload GIF for animation decode"
                ),
            }
        }
        self.omenchat.omenchat_media_cache.insert(
            omenchat_upload_cache_key(session_id, resource_id),
            OmenChatMediaLoadState::Cached {
                path: path_label.clone(),
                animated,
                content_type,
            },
        );
        Ok(path_label)
    }

    fn cache_omenchat_uploaded_gif_frames(&mut self, path_label: &str, bytes: &[u8]) {
        match iced_gif::Frames::from_bytes(bytes.to_vec()) {
            Ok(frames) => {
                self.omenchat
                    .omenchat_gif_frames
                    .insert(path_label.to_owned(), frames);
            }
            Err(error) => tracing::warn!(
                path = path_label,
                %error,
                "failed to decode cached OMENchat upload as animated GIF"
            ),
        }
    }
}
