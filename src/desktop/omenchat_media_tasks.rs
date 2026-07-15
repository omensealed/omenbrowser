use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use std::{fs::File, io::Read};

use iced::Task;
use serde::{Deserialize, Serialize};

use crate::chat::{ChatClientEvent, ChatClientRequest, ChatSessionId};
use crate::media::{
    decide_remote_media, RemoteMediaContext, RemoteMediaDecision, RemoteMediaTransport,
};
use crate::storage::files::atomic_replace;
use crate::storage::files::next_available_download_path;

use super::omenchat_desktop_state::DecodedOmenChatGif;
use super::omenchat_desktop_state::{
    CachedOmenChatMedia, OmenChatMediaCacheJob, OmenChatMediaCacheSource,
};
use super::{
    cached_media_is_animated_gif, fetch_clearweb_media_over_socks, gif_image_descriptor_count,
    image_dimensions_from_bytes, omenchat_media_loading_state, omenchat_upload_cache_key,
    omenchat_upload_content_type, pick_omenchat_upload_file, DesktopApp, Message,
    OmenChatDraftCommandResult, OmenChatMediaLoadState, OMENCHAT_GIF_DECODED_MAX_BYTES,
    OMENCHAT_GIF_ENCODED_MAX_BYTES, OMENCHAT_GIF_MAX_DIMENSION, OMENCHAT_GIF_MAX_FRAMES,
    OMENCHAT_MEDIA_DISK_DIRTY_MARKER, OMENCHAT_MEDIA_DISK_MAX_BYTES, OMENCHAT_MEDIA_DISK_MAX_ITEMS,
};

static OMENCHAT_GIF_DECODE_PERMITS: LazyLock<Arc<tokio::sync::Semaphore>> =
    LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(2)));
static OMENCHAT_MEDIA_DISK_LOCK: LazyLock<std::sync::Mutex<()>> =
    LazyLock::new(|| std::sync::Mutex::new(()));
const OMENCHAT_MEDIA_DISK_INDEX: &str = ".omenchat-media-index.json";

#[derive(Debug, Default, Deserialize, Serialize)]
struct OmenChatMediaDiskIndex {
    entries: BTreeMap<String, OmenChatMediaDiskEntry>,
    total_bytes: u64,
    next_sequence: u64,
}

#[derive(Debug, Deserialize, Serialize)]
struct OmenChatMediaDiskEntry {
    bytes: u64,
    sequence: u64,
}

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
            |(session_id, result)| {
                Message::OmenChatMediaCompletion(Box::new(
                    super::OmenChatMediaCompletionMessage::UploadPicked { session_id, result },
                ))
            },
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
        result: Result<DecodedOmenChatGif, String>,
    ) {
        match result {
            Ok(decoded) => {
                if self
                    .omenchat
                    .omenchat_gif_frames
                    .insert(path.clone(), decoded)
                {
                    self.app.status.task = format!("OMENchat animated GIF ready: {path}");
                } else {
                    self.app.status.task = format!(
                        "OMENchat GIF animation skipped because the decoded cache budget is full: {path}"
                    );
                }
            }
            Err(error) => {
                self.app.status.task =
                    format!("OMENchat GIF animation load failed for {path}: {error}");
            }
        }
    }

    pub(super) fn update_omenchat_media_loaded(
        &mut self,
        url: String,
        result: Result<(crate::browser::DownloadedFile, Vec<String>), String>,
    ) -> Task<Message> {
        match result {
            Ok((file, evicted_paths)) => {
                self.omenchat
                    .omenchat_media_cache
                    .remove_cached_paths(&evicted_paths);
                let path = file.path.display().to_string();
                let animated = cfg!(feature = "chat-client-gif")
                    && cached_media_is_animated_gif(&file.path, &file.content_type);
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
                        let result =
                            match mark_omenchat_media_disk_dirty_async(&media_cache_dir).await {
                                Ok(()) => runtime
                                    .download_file(
                                        &url,
                                        &media_cache_dir,
                                        crate::runtime::CancellationToken::new(),
                                    )
                                    .await
                                    .map_err(|error| error.to_string()),
                                Err(error) => Err(error),
                            };
                        let result = prune_downloaded_omenchat_media(result).await;
                        (url, result)
                    },
                    |(url, result)| {
                        Message::OmenChatMediaCompletion(Box::new(
                            super::OmenChatMediaCompletionMessage::MediaLoaded { url, result },
                        ))
                    },
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
                        let result = prune_downloaded_omenchat_media(result).await;
                        (url, result)
                    },
                    |(url, result)| {
                        Message::OmenChatMediaCompletion(Box::new(
                            super::OmenChatMediaCompletionMessage::MediaLoaded { url, result },
                        ))
                    },
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
                let permit = OMENCHAT_GIF_DECODE_PERMITS
                    .clone()
                    .acquire_owned()
                    .await
                    .map_err(|_| "OMENchat GIF decoder is shutting down".to_string());
                let result = match permit {
                    Ok(permit) => {
                        let worker_path = path.clone();
                        tokio::task::spawn_blocking(move || {
                            let _permit = permit;
                            decode_omenchat_gif_path(Path::new(&worker_path))
                        })
                        .await
                        .map_err(|error| error.to_string())
                        .and_then(|result| result)
                    }
                    Err(error) => Err(error),
                };
                (path, result)
            },
            |(path, result)| {
                Message::OmenChatMediaCompletion(Box::new(
                    super::OmenChatMediaCompletionMessage::GifFramesLoaded { path, result },
                ))
            },
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
        let content_type = content_type
            .map(ToOwned::to_owned)
            .or_else(|| omenchat_upload_content_type(filename))
            .unwrap_or_else(|| "application/octet-stream".into());
        let cache_key = omenchat_upload_cache_key(session_id, resource_id);
        self.omenchat.omenchat_media_cache.insert(
            cache_key.clone(),
            omenchat_media_loading_state("caching upload resource"),
        );
        if !self
            .omenchat
            .enqueue_media_cache_job(OmenChatMediaCacheJob {
                session_id,
                cache_key: cache_key.clone(),
                filename: filename.to_owned(),
                content_type,
                source: OmenChatMediaCacheSource::Bytes(bytes.to_vec()),
                reserved_bytes: bytes.len(),
                generation: 0,
                cancellation: crate::runtime::CancellationToken::new(),
            })
        {
            self.omenchat.omenchat_media_cache.insert(
                cache_key,
                OmenChatMediaLoadState::Failed {
                    message: "OMENchat media cache queue is full".into(),
                },
            );
            anyhow::bail!("OMENchat media cache queue is full");
        }
        Ok("background cache queued".into())
    }

    pub(in crate::desktop) fn cache_omenchat_upload_source_file(
        &mut self,
        session_id: ChatSessionId,
        resource_id: &str,
        filename: &str,
        source_path: &Path,
    ) -> anyhow::Result<String> {
        let content_type = omenchat_upload_content_type(filename)
            .unwrap_or_else(|| "application/octet-stream".into());
        let cache_key = omenchat_upload_cache_key(session_id, resource_id);
        self.omenchat.omenchat_media_cache.insert(
            cache_key.clone(),
            omenchat_media_loading_state("caching local upload"),
        );
        if !self
            .omenchat
            .enqueue_media_cache_job(OmenChatMediaCacheJob {
                session_id,
                cache_key: cache_key.clone(),
                filename: filename.to_owned(),
                content_type,
                source: OmenChatMediaCacheSource::File(source_path.to_path_buf()),
                reserved_bytes: OMENCHAT_GIF_ENCODED_MAX_BYTES,
                generation: 0,
                cancellation: crate::runtime::CancellationToken::new(),
            })
        {
            self.omenchat.omenchat_media_cache.insert(
                cache_key,
                OmenChatMediaLoadState::Failed {
                    message: "OMENchat media cache queue is full".into(),
                },
            );
            anyhow::bail!("OMENchat media cache queue is full");
        }
        Ok("background cache queued".into())
    }

    pub(in crate::desktop) fn drain_omenchat_media_cache_tasks(&mut self) -> Task<Message> {
        let cache_dir = self.app.paths.cache_dir.join("omenchat-media");
        Task::batch(
            self.omenchat
                .take_media_cache_jobs()
                .into_iter()
                .map(|job| {
                    let cache_dir = cache_dir.clone();
                    Task::perform(
                        async move {
                            let session_id = job.session_id;
                            let cache_key = job.cache_key.clone();
                            let generation = job.generation;
                            let permit = OMENCHAT_GIF_DECODE_PERMITS
                                .clone()
                                .acquire_owned()
                                .await
                                .map_err(|_| {
                                    "OMENchat media cache worker is shutting down".to_string()
                                });
                            let result = match permit {
                                Ok(permit) => tokio::task::spawn_blocking(move || {
                                    let _permit = permit;
                                    cache_omenchat_media_job(&cache_dir, job)
                                })
                                .await
                                .map_err(|error| error.to_string())
                                .and_then(|result| result),
                                Err(error) => Err(error),
                            };
                            (session_id, cache_key, generation, result)
                        },
                        |(session_id, cache_key, generation, result)| {
                            Message::OmenChatMediaCompletion(Box::new(
                                super::OmenChatMediaCompletionMessage::CacheCompleted(Box::new(
                                    super::OmenChatMediaCacheCompletion {
                                        session_id,
                                        cache_key,
                                        generation,
                                        result,
                                    },
                                )),
                            ))
                        },
                    )
                }),
        )
    }

    pub(super) fn update_omenchat_media_cache_completed(
        &mut self,
        session_id: ChatSessionId,
        cache_key: String,
        generation: u64,
        result: Result<CachedOmenChatMedia, String>,
    ) -> Task<Message> {
        if !self
            .omenchat
            .accept_media_cache_completion(session_id, &cache_key, generation)
        {
            tracing::debug!(
                session_id,
                cache_key,
                generation,
                "ignored stale OMENchat media cache completion"
            );
            return match result {
                Ok(cached) => Task::perform(
                    async move {
                        let _ = tokio::fs::remove_file(cached.path).await;
                    },
                    |()| {
                        Message::OmenChatMediaCompletion(Box::new(
                            super::OmenChatMediaCompletionMessage::StaleMediaRemoved,
                        ))
                    },
                ),
                Err(_) => Task::none(),
            };
        }
        match result {
            Ok(cached) => {
                self.omenchat
                    .omenchat_media_cache
                    .remove_cached_paths(&cached.evicted_paths);
                if let Some(decoded) = cached.decoded_gif {
                    let _ = self
                        .omenchat
                        .omenchat_gif_frames
                        .insert(cached.path.clone(), decoded);
                }
                self.omenchat.omenchat_media_cache.insert(
                    cache_key,
                    OmenChatMediaLoadState::Cached {
                        path: cached.path.clone(),
                        content_type: cached.content_type,
                        animated: cached.animated,
                    },
                );
                self.set_omenchat_session_status(
                    session_id,
                    format!("upload cached locally: {}", cached.path),
                );
            }
            Err(error) => {
                self.omenchat.omenchat_media_cache.insert(
                    cache_key,
                    OmenChatMediaLoadState::Failed {
                        message: error.clone(),
                    },
                );
                self.set_omenchat_session_status(
                    session_id,
                    format!("upload accepted; local cache failed: {error}"),
                );
            }
        }
        Task::none()
    }
}

async fn prune_downloaded_omenchat_media(
    result: Result<crate::browser::DownloadedFile, String>,
) -> Result<(crate::browser::DownloadedFile, Vec<String>), String> {
    let file = result?;
    let path = file.path.clone();
    let cache_dir = path
        .parent()
        .ok_or_else(|| "OMENchat media cache path has no parent".to_string())?
        .to_path_buf();
    let worker_path = path.clone();
    let evicted = tokio::task::spawn_blocking(move || {
        let result = prune_omenchat_media_disk_cache(&cache_dir, &worker_path);
        if result.is_err() {
            let _ = std::fs::remove_file(&worker_path);
        }
        result
    })
    .await
    .map_err(|error| error.to_string())??;
    Ok((file, evicted))
}

fn cache_omenchat_media_job(
    cache_dir: &Path,
    job: OmenChatMediaCacheJob,
) -> Result<CachedOmenChatMedia, String> {
    ensure_media_job_active(&job.cancellation)?;
    std::fs::create_dir_all(cache_dir).map_err(|error| error.to_string())?;
    mark_omenchat_media_disk_dirty(cache_dir)?;
    let path = next_available_download_path(cache_dir, &job.filename)
        .map_err(|error| error.to_string())?;
    let result = (|| {
        ensure_media_job_active(&job.cancellation)?;
        let source_bytes = match job.source {
            OmenChatMediaCacheSource::Bytes(bytes) => {
                if bytes.len() > OMENCHAT_GIF_ENCODED_MAX_BYTES {
                    return Err(format!(
                        "upload media exceeds {} byte cache limit",
                        OMENCHAT_GIF_ENCODED_MAX_BYTES
                    ));
                }
                std::fs::write(&path, bytes.as_slice()).map_err(|error| error.to_string())?;
                Some(bytes)
            }
            OmenChatMediaCacheSource::File(source) => {
                let source_len = std::fs::metadata(&source)
                    .map_err(|error| error.to_string())?
                    .len();
                if source_len > OMENCHAT_GIF_ENCODED_MAX_BYTES as u64 {
                    return Err(format!(
                        "upload media exceeds {} byte cache limit",
                        OMENCHAT_GIF_ENCODED_MAX_BYTES
                    ));
                }
                std::fs::copy(&source, &path).map_err(|error| error.to_string())?;
                None
            }
        };
        let animated = cfg!(feature = "chat-client-gif")
            && cached_media_is_animated_gif(&path, &job.content_type);
        ensure_media_job_active(&job.cancellation)?;
        let decoded_gif = if animated {
            let bytes = match source_bytes {
                Some(bytes) => bytes,
                None => read_bounded_media_file(&path)?,
            };
            Some(decode_omenchat_gif(bytes, &job.cancellation)?)
        } else {
            None
        };
        ensure_media_job_active(&job.cancellation)?;
        let evicted_paths = prune_omenchat_media_disk_cache(cache_dir, &path)?;
        ensure_media_job_active(&job.cancellation)?;
        Ok(CachedOmenChatMedia {
            path: path.display().to_string(),
            content_type: job.content_type,
            animated,
            decoded_gif,
            evicted_paths,
        })
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&path);
    }
    result
}

fn ensure_media_job_active(cancellation: &crate::runtime::CancellationToken) -> Result<(), String> {
    if cancellation.is_cancelled() {
        Err("OMENchat media cache job cancelled".into())
    } else {
        Ok(())
    }
}

fn prune_omenchat_media_disk_cache(
    cache_dir: &Path,
    protected: &Path,
) -> Result<Vec<String>, String> {
    let _guard = OMENCHAT_MEDIA_DISK_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    prune_omenchat_media_disk_cache_locked(cache_dir, protected)
}

fn prune_omenchat_media_disk_cache_locked(
    cache_dir: &Path,
    protected: &Path,
) -> Result<Vec<String>, String> {
    let protected_name = media_cache_file_name(protected)?;
    let protected_bytes = std::fs::metadata(protected)
        .map_err(|error| error.to_string())?
        .len();
    let mut index = load_or_rebuild_media_disk_index(cache_dir)?;
    if let Some(previous) = index.entries.remove(&protected_name) {
        index.total_bytes = index.total_bytes.saturating_sub(previous.bytes);
    }
    index.next_sequence = index.next_sequence.saturating_add(1);
    index.entries.insert(
        protected_name.clone(),
        OmenChatMediaDiskEntry {
            bytes: protected_bytes,
            sequence: index.next_sequence,
        },
    );
    index.total_bytes = index.total_bytes.saturating_add(protected_bytes);
    let mut removed = Vec::new();
    while index.entries.len() > OMENCHAT_MEDIA_DISK_MAX_ITEMS
        || index.total_bytes > OMENCHAT_MEDIA_DISK_MAX_BYTES
    {
        let candidate = index
            .entries
            .iter()
            .filter(|(name, _)| *name != &protected_name)
            .min_by_key(|(name, entry)| (entry.sequence, *name))
            .map(|(name, _)| name.clone());
        let Some(name) = candidate else { break };
        let entry = index
            .entries
            .remove(&name)
            .expect("indexed candidate exists");
        index.total_bytes = index.total_bytes.saturating_sub(entry.bytes);
        let path = cache_dir.join(name);
        if path.exists() {
            std::fs::remove_file(&path).map_err(|error| error.to_string())?;
            removed.push(path.display().to_string());
        }
    }
    save_media_disk_index(cache_dir, &index)?;
    clear_omenchat_media_disk_dirty(cache_dir)?;
    Ok(removed)
}

fn mark_omenchat_media_disk_dirty(cache_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(cache_dir).map_err(|error| error.to_string())?;
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(cache_dir.join(OMENCHAT_MEDIA_DISK_DIRTY_MARKER))
        .and_then(|file| file.sync_all())
        .map_err(|error| error.to_string())
}

pub(super) async fn mark_omenchat_media_disk_dirty_async(cache_dir: &Path) -> Result<(), String> {
    tokio::fs::create_dir_all(cache_dir)
        .await
        .map_err(|error| error.to_string())?;
    let file = tokio::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(cache_dir.join(OMENCHAT_MEDIA_DISK_DIRTY_MARKER))
        .await
        .map_err(|error| error.to_string())?;
    file.sync_all().await.map_err(|error| error.to_string())
}

fn clear_omenchat_media_disk_dirty(cache_dir: &Path) -> Result<(), String> {
    match std::fs::remove_file(cache_dir.join(OMENCHAT_MEDIA_DISK_DIRTY_MARKER)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn media_cache_file_name(path: &Path) -> Result<String, String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty() && *name != OMENCHAT_MEDIA_DISK_INDEX)
        .map(str::to_owned)
        .ok_or_else(|| "OMENchat media cache path has no safe file name".to_string())
}

fn load_or_rebuild_media_disk_index(cache_dir: &Path) -> Result<OmenChatMediaDiskIndex, String> {
    let index_path = cache_dir.join(OMENCHAT_MEDIA_DISK_INDEX);
    let repair_required = cache_dir.join(OMENCHAT_MEDIA_DISK_DIRTY_MARKER).exists();
    if !repair_required {
        if let Ok(raw) = std::fs::read(&index_path) {
            if let Ok(index) = serde_json::from_slice::<OmenChatMediaDiskIndex>(&raw) {
                let entries_are_safe = index.entries.iter().all(|(name, entry)| {
                    Path::new(name).file_name().and_then(|value| value.to_str())
                        == Some(name.as_str())
                        && name != OMENCHAT_MEDIA_DISK_INDEX
                        && entry.bytes <= OMENCHAT_MEDIA_DISK_MAX_BYTES
                });
                let total_is_valid = index.total_bytes
                    == index
                        .entries
                        .values()
                        .map(|entry| entry.bytes)
                        .fold(0u64, u64::saturating_add);
                if entries_are_safe && total_is_valid {
                    return Ok(index);
                }
            }
        }
    }
    let mut files = Vec::new();
    for entry in std::fs::read_dir(cache_dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == OMENCHAT_MEDIA_DISK_INDEX || name == OMENCHAT_MEDIA_DISK_DIRTY_MARKER {
            continue;
        }
        if name.ends_with(".tmp") {
            let _ = std::fs::remove_file(entry.path());
            continue;
        }
        if !entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_file()
        {
            continue;
        }
        let metadata = entry.metadata().map_err(|error| error.to_string())?;
        let modified = metadata
            .modified()
            .unwrap_or(std::time::UNIX_EPOCH)
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        files.push((modified, name, metadata.len()));
    }
    files.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    let mut index = OmenChatMediaDiskIndex::default();
    for (_, name, bytes) in files {
        index.next_sequence = index.next_sequence.saturating_add(1);
        index.total_bytes = index.total_bytes.saturating_add(bytes);
        index.entries.insert(
            name,
            OmenChatMediaDiskEntry {
                bytes,
                sequence: index.next_sequence,
            },
        );
    }
    Ok(index)
}

fn save_media_disk_index(cache_dir: &Path, index: &OmenChatMediaDiskIndex) -> Result<(), String> {
    let path = cache_dir.join(OMENCHAT_MEDIA_DISK_INDEX);
    let temporary = cache_dir.join(format!(
        "{OMENCHAT_MEDIA_DISK_INDEX}.{}.tmp",
        std::process::id()
    ));
    let raw = serde_json::to_vec(index).map_err(|error| error.to_string())?;
    std::fs::write(&temporary, raw).map_err(|error| error.to_string())?;
    if let Err(error) = atomic_replace(&temporary, &path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.to_string());
    }
    Ok(())
}

fn read_bounded_media_file(path: &Path) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::with_capacity(OMENCHAT_GIF_ENCODED_MAX_BYTES.min(256 * 1024));
    file.take(OMENCHAT_GIF_ENCODED_MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > OMENCHAT_GIF_ENCODED_MAX_BYTES {
        return Err(format!(
            "cached media exceeds {} byte limit",
            OMENCHAT_GIF_ENCODED_MAX_BYTES
        ));
    }
    Ok(bytes)
}

fn decode_omenchat_gif_path(path: &Path) -> Result<DecodedOmenChatGif, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::with_capacity(OMENCHAT_GIF_ENCODED_MAX_BYTES.min(256 * 1024));
    file.take(OMENCHAT_GIF_ENCODED_MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    decode_omenchat_gif(bytes, &crate::runtime::CancellationToken::new())
}

fn decode_omenchat_gif(
    bytes: Vec<u8>,
    cancellation: &crate::runtime::CancellationToken,
) -> Result<DecodedOmenChatGif, String> {
    ensure_media_job_active(cancellation)?;
    if bytes.len() > OMENCHAT_GIF_ENCODED_MAX_BYTES {
        return Err(format!(
            "encoded GIF exceeds {} byte limit",
            OMENCHAT_GIF_ENCODED_MAX_BYTES
        ));
    }
    let (width, height) = image_dimensions_from_bytes(&bytes)
        .ok_or_else(|| "GIF dimensions are missing or malformed".to_string())?;
    if width == 0
        || height == 0
        || width > OMENCHAT_GIF_MAX_DIMENSION
        || height > OMENCHAT_GIF_MAX_DIMENSION
    {
        return Err(format!(
            "GIF dimensions {width}x{height} exceed {} pixel limit",
            OMENCHAT_GIF_MAX_DIMENSION
        ));
    }
    ensure_media_job_active(cancellation)?;
    let frames = gif_image_descriptor_count(&bytes, OMENCHAT_GIF_MAX_FRAMES + 1);
    if frames == 0 || frames > OMENCHAT_GIF_MAX_FRAMES {
        return Err(format!(
            "GIF frame count {frames} is outside 1..={OMENCHAT_GIF_MAX_FRAMES}"
        ));
    }
    ensure_media_job_active(cancellation)?;
    let decoded_bytes = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|frame_bytes| frame_bytes.checked_mul(frames as u64))
        .ok_or_else(|| "GIF decoded size overflow".to_string())?;
    if decoded_bytes > OMENCHAT_GIF_DECODED_MAX_BYTES {
        return Err(format!(
            "decoded GIF estimate {decoded_bytes} exceeds {OMENCHAT_GIF_DECODED_MAX_BYTES} byte limit"
        ));
    }
    ensure_media_job_active(cancellation)?;
    let frames = decode_omenchat_gif_frames(bytes)?;
    ensure_media_job_active(cancellation)?;
    Ok(DecodedOmenChatGif {
        frames: Arc::new(frames),
        decoded_bytes,
    })
}

#[cfg(feature = "chat-client-gif")]
fn decode_omenchat_gif_frames(
    bytes: Vec<u8>,
) -> Result<super::omenchat_desktop_state::OmenChatGifFrames, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        iced_gif::Frames::from_bytes(bytes)
    }))
    .map_err(|_| "GIF decoder rejected malformed frame data".to_string())?
    .map_err(|error| error.to_string())
}

#[cfg(not(feature = "chat-client-gif"))]
fn decode_omenchat_gif_frames(
    _bytes: Vec<u8>,
) -> Result<super::omenchat_desktop_state::OmenChatGifFrames, String> {
    Err("animated GIF support is disabled in this build".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_pixel_gif() -> Vec<u8> {
        vec![
            0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0x00,
            0x00, 0x00, 0xff, 0xff, 0xff, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00,
            0x00, 0x02, 0x01, 0x4c, 0x00, 0x3b,
        ]
    }

    fn gif_with_descriptors(width: u16, height: u16, descriptors: usize) -> Vec<u8> {
        let mut bytes = b"GIF89a".to_vec();
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&height.to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 0]);
        for _ in 0..descriptors {
            bytes.extend_from_slice(&[0x2C, 0, 0, 0, 0]);
            bytes.extend_from_slice(&width.to_le_bytes());
            bytes.extend_from_slice(&height.to_le_bytes());
            bytes.extend_from_slice(&[0, 2, 1, 0, 0]);
        }
        bytes.push(0x3B);
        bytes
    }

    #[test]
    fn gif_decode_policy_rejects_dimensions_frames_and_decoded_bytes_before_decode() {
        let cancellation = crate::runtime::CancellationToken::new();
        let dimensions = decode_omenchat_gif(gif_with_descriptors(4097, 1, 1), &cancellation)
            .expect_err("oversized dimensions");
        assert!(dimensions.contains("dimensions"));

        let frames = decode_omenchat_gif(
            gif_with_descriptors(1, 1, OMENCHAT_GIF_MAX_FRAMES + 1),
            &cancellation,
        )
        .expect_err("oversized frame count");
        assert!(frames.contains("frame count"));

        let decoded = decode_omenchat_gif(gif_with_descriptors(4096, 4096, 2), &cancellation)
            .expect_err("oversized decoded estimate");
        assert!(decoded.contains("decoded GIF estimate"));
    }

    #[test]
    #[cfg(feature = "chat-client-gif")]
    fn gif_decode_policy_accepts_bounded_one_pixel_gif() {
        let decoded =
            decode_omenchat_gif(one_pixel_gif(), &crate::runtime::CancellationToken::new())
                .expect("bounded one pixel GIF");
        assert_eq!(decoded.decoded_bytes, 4);
    }

    #[test]
    #[cfg(feature = "chat-client-gif")]
    fn adversarial_gif_corpus_never_unwinds_or_exceeds_admission_budget() {
        let named = [
            ("empty", Vec::new()),
            ("header-only", b"GIF89a".to_vec()),
            ("truncated-screen", b"GIF89a\x01\x00".to_vec()),
            ("zero-dimension", gif_with_descriptors(0, 1, 1)),
            ("oversize-dimension", gif_with_descriptors(4097, 1, 1)),
            (
                "excessive-frames",
                gif_with_descriptors(1, 1, OMENCHAT_GIF_MAX_FRAMES + 1),
            ),
            ("malformed-lzw", gif_with_descriptors(1, 1, 2)),
        ];
        for (name, bytes) in named {
            let outcome = std::panic::catch_unwind(|| {
                decode_omenchat_gif(bytes, &crate::runtime::CancellationToken::new())
            });
            assert!(outcome.is_ok(), "named GIF corpus case unwound: {name}");
            assert!(
                outcome.expect("checked unwind").is_err(),
                "named GIF corpus case unexpectedly decoded: {name}"
            );
        }

        let seed = one_pixel_gif();
        let mut state = 0x6f6d_656e_6769_6631_u64;
        for case in 0..512usize {
            let mut bytes = seed.clone();
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let edits = 1 + (state as usize % 4);
            for edit in 0..edits {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let index = state as usize % bytes.len();
                bytes[index] ^= ((state >> (edit * 8)) as u8).max(1);
            }
            if case % 7 == 0 {
                bytes.truncate(case % bytes.len());
            }
            let outcome = std::panic::catch_unwind(|| {
                decode_omenchat_gif(bytes, &crate::runtime::CancellationToken::new())
            });
            let decoded = outcome.unwrap_or_else(|_| panic!("mutated GIF case {case} unwound"));
            if let Ok(decoded) = decoded {
                assert!(decoded.decoded_bytes <= OMENCHAT_GIF_DECODED_MAX_BYTES);
            }
        }
    }

    #[test]
    #[cfg(feature = "chat-client-gif")]
    #[ignore = "native release-mode media measurement only"]
    fn measure_omenchat_gif_decode_latency() {
        fn rss_kib() -> Option<u64> {
            std::fs::read_to_string("/proc/self/status")
                .ok()?
                .lines()
                .find_map(|line| line.strip_prefix("VmRSS:"))?
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        }

        let single = one_pixel_gif();
        let image_block = single[19..single.len() - 1].to_vec();
        let mut fixture = single[..single.len() - 1].to_vec();
        fixture.extend_from_slice(&image_block);
        fixture.push(0x3b);
        for _ in 0..20 {
            decode_omenchat_gif(fixture.clone(), &crate::runtime::CancellationToken::new())
                .expect("measurement warmup fixture");
        }
        let rss_before = rss_kib();
        let mut samples = Vec::with_capacity(200);
        for _ in 0..200 {
            let started = std::time::Instant::now();
            let decoded =
                decode_omenchat_gif(fixture.clone(), &crate::runtime::CancellationToken::new())
                    .expect("measurement fixture");
            assert_eq!(decoded.decoded_bytes, 8);
            samples.push(started.elapsed().as_nanos() as u64);
        }
        samples.sort_unstable();
        let median = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95).div_ceil(100) - 1];
        println!("decode_samples={}", samples.len());
        println!("decode_median_ns={median}");
        println!("decode_p95_ns={p95}");
        println!(
            "rss_kib_before={}",
            rss_before.map_or_else(|| "pending".into(), |value| value.to_string())
        );
        println!(
            "rss_kib_after={}",
            rss_kib().map_or_else(|| "pending".into(), |value| value.to_string())
        );
    }

    #[test]
    fn disk_media_cache_enforces_item_and_byte_budgets_and_protects_current_file() {
        let root = std::env::temp_dir().join(format!(
            "omenchat-media-prune-{}-{}",
            std::process::id(),
            crate::app::current_epoch_ms()
        ));
        std::fs::create_dir_all(&root).expect("isolated media root");
        let protected = root.join("protected.bin");
        std::fs::write(&protected, b"current").expect("protected file");
        for index in 0..OMENCHAT_MEDIA_DISK_MAX_ITEMS {
            std::fs::write(root.join(format!("item-{index:03}.bin")), b"x").expect("cache fixture");
        }
        let removed = prune_omenchat_media_disk_cache(&root, &protected).expect("item prune");
        assert_eq!(removed.len(), 1);
        assert!(protected.exists());
        assert_eq!(
            std::fs::read_dir(&root)
                .expect("cache listing")
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry.file_name() != OMENCHAT_MEDIA_DISK_INDEX
                        && entry.file_type().is_ok_and(|kind| kind.is_file())
                })
                .count(),
            OMENCHAT_MEDIA_DISK_MAX_ITEMS
        );
        std::fs::remove_dir_all(&root).expect("cleanup item fixture");

        let root = std::env::temp_dir().join(format!(
            "omenchat-media-byte-prune-{}-{}",
            std::process::id(),
            crate::app::current_epoch_ms()
        ));
        std::fs::create_dir_all(&root).expect("isolated byte root");
        let protected = root.join("protected.bin");
        std::fs::File::create(&protected)
            .and_then(|file| file.set_len(65 * 1024 * 1024))
            .expect("protected sparse file");
        for name in ["older-a.bin", "older-b.bin"] {
            std::fs::File::create(root.join(name))
                .and_then(|file| file.set_len(65 * 1024 * 1024))
                .expect("sparse cache fixture");
        }
        let removed = prune_omenchat_media_disk_cache(&root, &protected).expect("byte prune");
        assert_eq!(removed.len(), 2);
        assert!(protected.exists());
        std::fs::remove_dir_all(&root).expect("cleanup byte fixture");
    }

    #[test]
    fn disk_media_cache_uses_index_and_rejects_unsafe_index_names() {
        let root = std::env::temp_dir().join(format!(
            "omenchat-media-index-{}-{}",
            std::process::id(),
            crate::app::current_epoch_ms()
        ));
        std::fs::create_dir_all(&root).expect("isolated media root");
        let protected = root.join("protected.bin");
        std::fs::write(&protected, b"current").expect("protected file");
        prune_omenchat_media_disk_cache(&root, &protected).expect("build index");

        let sentinel = root.join("unindexed-sentinel.bin");
        std::fs::write(&sentinel, b"sentinel").expect("sentinel file");
        prune_omenchat_media_disk_cache(&root, &protected).expect("indexed prune");
        assert!(
            sentinel.exists(),
            "normal prune must not enumerate the directory"
        );

        let orphan = root.join("committed-before-index.bin");
        let abandoned_temporary = root.join(".abandoned.1.tmp");
        std::fs::write(&orphan, b"orphan").expect("committed orphan fixture");
        std::fs::write(&abandoned_temporary, b"partial").expect("temporary fixture");
        mark_omenchat_media_disk_dirty(&root).expect("dirty marker");
        prune_omenchat_media_disk_cache(&root, &protected).expect("crash repair");
        let repaired: OmenChatMediaDiskIndex = serde_json::from_slice(
            &std::fs::read(root.join(OMENCHAT_MEDIA_DISK_INDEX)).expect("repaired index"),
        )
        .expect("decode repaired index");
        assert!(repaired.entries.contains_key("committed-before-index.bin"));
        assert!(!abandoned_temporary.exists());
        assert!(!root.join(OMENCHAT_MEDIA_DISK_DIRTY_MARKER).exists());

        let outside = root
            .parent()
            .expect("root parent")
            .join(format!("omenchat-media-outside-{}", std::process::id()));
        std::fs::write(&outside, b"outside").expect("outside fixture");
        let unsafe_index = serde_json::json!({
            "entries": {
                (format!("../{}", outside.file_name().expect("outside name").to_string_lossy())): {
                    "bytes": 1,
                    "sequence": 0
                }
            },
            "total_bytes": 1,
            "next_sequence": 0
        });
        std::fs::write(
            root.join(OMENCHAT_MEDIA_DISK_INDEX),
            serde_json::to_vec(&unsafe_index).expect("unsafe index fixture"),
        )
        .expect("write unsafe index");
        prune_omenchat_media_disk_cache(&root, &protected).expect("repair unsafe index");
        assert!(outside.exists(), "cache repair must not escape its root");

        std::fs::remove_file(outside).expect("cleanup outside fixture");
        std::fs::remove_dir_all(root).expect("cleanup media fixture");
    }

    #[test]
    #[ignore = "release-mode cache latency measurement"]
    fn measure_cache_index_latency() {
        let root = std::env::temp_dir().join(format!(
            "omenchat-media-index-measure-{}-{}",
            std::process::id(),
            crate::app::current_epoch_ms()
        ));
        std::fs::create_dir_all(&root).expect("isolated measurement root");
        let protected = root.join("protected.bin");
        std::fs::write(&protected, b"current").expect("protected fixture");
        for index in 1..OMENCHAT_MEDIA_DISK_MAX_ITEMS {
            std::fs::write(root.join(format!("item-{index:03}.bin")), b"x").expect("media fixture");
        }
        prune_omenchat_media_disk_cache(&root, &protected).expect("initial index build");

        let mut indexed = Vec::with_capacity(200);
        let mut scan_shape = Vec::with_capacity(200);
        for _ in 0..200 {
            let started = std::time::Instant::now();
            prune_omenchat_media_disk_cache(&root, &protected).expect("indexed prune");
            indexed.push(started.elapsed().as_nanos() as u64);

            let started = std::time::Instant::now();
            let mut files = std::fs::read_dir(&root)
                .expect("scan-shape listing")
                .filter_map(Result::ok)
                .filter(|entry| {
                    let name = entry.file_name();
                    name != OMENCHAT_MEDIA_DISK_INDEX
                        && name != OMENCHAT_MEDIA_DISK_DIRTY_MARKER
                        && entry.file_type().is_ok_and(|kind| kind.is_file())
                })
                .filter_map(|entry| {
                    entry.metadata().ok().map(|metadata| {
                        (
                            metadata.modified().unwrap_or(std::time::UNIX_EPOCH),
                            entry.path(),
                            metadata.len(),
                        )
                    })
                })
                .collect::<Vec<_>>();
            files.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| right.1.cmp(&left.1)));
            assert_eq!(files.len(), OMENCHAT_MEDIA_DISK_MAX_ITEMS);
            let kept = files
                .iter()
                .map(|(_, path, _)| path.clone())
                .collect::<std::collections::HashSet<_>>();
            let second_pass = std::fs::read_dir(&root)
                .expect("scan-shape second listing")
                .filter_map(Result::ok)
                .filter(|entry| {
                    let name = entry.file_name();
                    name != OMENCHAT_MEDIA_DISK_INDEX
                        && name != OMENCHAT_MEDIA_DISK_DIRTY_MARKER
                        && entry.file_type().is_ok_and(|kind| kind.is_file())
                        && kept.contains(&entry.path())
                })
                .count();
            assert_eq!(second_pass, OMENCHAT_MEDIA_DISK_MAX_ITEMS);
            scan_shape.push(started.elapsed().as_nanos() as u64);
        }
        indexed.sort_unstable();
        scan_shape.sort_unstable();
        println!("media_cache_entries={OMENCHAT_MEDIA_DISK_MAX_ITEMS}");
        println!("media_indexed_median_ns={}", indexed[indexed.len() / 2]);
        println!(
            "media_indexed_p95_ns={}",
            indexed[(indexed.len() * 95).div_ceil(100) - 1]
        );
        println!(
            "media_scan_shape_median_ns={}",
            scan_shape[scan_shape.len() / 2]
        );
        println!(
            "media_scan_shape_p95_ns={}",
            scan_shape[(scan_shape.len() * 95).div_ceil(100) - 1]
        );
        std::fs::remove_dir_all(root).expect("cleanup measurement cache");
    }

    #[test]
    fn media_cache_worker_bounds_sources_and_cleans_failed_decode() {
        let root = std::env::temp_dir().join(format!(
            "omenchat-media-worker-{}-{}",
            std::process::id(),
            crate::app::current_epoch_ms()
        ));
        std::fs::create_dir_all(&root).expect("isolated media root");

        let cancellation = crate::runtime::CancellationToken::new();
        cancellation.cancel();
        let cancelled = cache_omenchat_media_job(
            &root,
            OmenChatMediaCacheJob {
                session_id: 1,
                cache_key: "cancelled".into(),
                filename: "cancelled.bin".into(),
                content_type: "application/octet-stream".into(),
                source: OmenChatMediaCacheSource::Bytes(b"cancelled".to_vec()),
                reserved_bytes: 9,
                generation: 1,
                cancellation,
            },
        )
        .expect_err("cancelled worker");
        assert!(cancelled.contains("cancelled"));
        assert!(!root.join("cancelled.bin").exists());

        let cached = cache_omenchat_media_job(
            &root,
            OmenChatMediaCacheJob {
                session_id: 1,
                cache_key: "plain".into(),
                filename: "plain.bin".into(),
                content_type: "application/octet-stream".into(),
                source: OmenChatMediaCacheSource::Bytes(b"plain".to_vec()),
                reserved_bytes: 5,
                generation: 1,
                cancellation: crate::runtime::CancellationToken::new(),
            },
        )
        .expect("bounded byte source");
        assert_eq!(std::fs::read(&cached.path).expect("cached bytes"), b"plain");

        let malformed_job = || OmenChatMediaCacheJob {
            session_id: 1,
            cache_key: "bad-gif".into(),
            filename: "bad.gif".into(),
            content_type: "image/gif".into(),
            source: OmenChatMediaCacheSource::Bytes(gif_with_descriptors(1, 1, 2)),
            reserved_bytes: 64,
            generation: 1,
            cancellation: crate::runtime::CancellationToken::new(),
        };
        #[cfg(feature = "chat-client-gif")]
        {
            let error = cache_omenchat_media_job(&root, malformed_job())
                .expect_err("malformed animated GIF");
            assert!(error.contains("color table") || error.contains("malformed"));
            assert!(!root.join("bad.gif").exists());
        }
        #[cfg(not(feature = "chat-client-gif"))]
        {
            let cached = cache_omenchat_media_job(&root, malformed_job())
                .expect("GIF is retained as a static image when animation is disabled");
            assert!(!cached.animated);
            assert!(cached.decoded_gif.is_none());
            assert!(root.join("bad.gif").exists());
        }

        let oversized = root.join("oversized-source.bin");
        let file = File::create(&oversized).expect("sparse source");
        file.set_len(OMENCHAT_GIF_ENCODED_MAX_BYTES as u64 + 1)
            .expect("oversized sparse source");
        drop(file);
        let error = cache_omenchat_media_job(
            &root,
            OmenChatMediaCacheJob {
                session_id: 1,
                cache_key: "oversized".into(),
                filename: "oversized-copy.bin".into(),
                content_type: "application/octet-stream".into(),
                source: OmenChatMediaCacheSource::File(oversized),
                reserved_bytes: OMENCHAT_GIF_ENCODED_MAX_BYTES,
                generation: 1,
                cancellation: crate::runtime::CancellationToken::new(),
            },
        )
        .expect_err("oversized source");
        assert!(error.contains("byte cache limit"));
        assert!(!root.join("oversized-copy.bin").exists());
        std::fs::remove_dir_all(root).expect("remove isolated media root");
    }
}
