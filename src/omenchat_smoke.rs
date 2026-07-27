//! Isolated OMENchat live smoke orchestration for the CLI binary.
//!
//! This module owns bounded Link, reconnect, upload, reaction, and message-revision
//! qualification flows. It does not own production runtime or protocol state.

use super::OmenChatSmokeCommandInput;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use super::{apply_smoke_overrides, load_config_for_smoke, parse_16_byte_hex_hash};
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use anyhow::Context;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use omenbrowser_rs::app::App;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use omenbrowser_rs::chat::rns::ChatLinkTransport;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use omenbrowser_rs::runtime::{CancellationToken, RuntimeBusEvent};
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use std::collections::{BTreeMap, VecDeque};
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use std::path::PathBuf;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
#[derive(Clone, Debug, Default)]
struct OmenChatSmokeTransport {
    incoming_frames: VecDeque<Vec<u8>>,
    resources: BTreeMap<String, Vec<u8>>,
    pending_resource_offers: BTreeMap<String, VecDeque<Vec<u8>>>,
    outgoing_frames: Vec<Vec<u8>>,
    outgoing_resources: Vec<(String, Vec<u8>)>,
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
impl OmenChatSmokeTransport {
    fn push_incoming_frame(&mut self, frame: Vec<u8>) {
        self.incoming_frames.push_back(frame);
    }

    fn push_resource(&mut self, metadata: Option<Vec<u8>>, data: Vec<u8>) {
        let Some(resource_id) =
            omenbrowser_rs::chat::rns::resource_id_from_metadata(metadata.as_deref())
        else {
            return;
        };
        self.resources.insert(resource_id.clone(), data);
        if let Some(mut offers) = self.pending_resource_offers.remove(&resource_id) {
            while let Some(frame) = offers.pop_back() {
                self.incoming_frames.push_front(frame);
            }
        }
    }

    fn take_outgoing_frames(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.outgoing_frames)
    }

    fn take_outgoing_resources(&mut self) -> Vec<(String, Vec<u8>)> {
        std::mem::take(&mut self.outgoing_resources)
    }
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
impl ChatLinkTransport for OmenChatSmokeTransport {
    fn send_frame(&mut self, frame_bytes: Vec<u8>) -> anyhow::Result<()> {
        self.outgoing_frames.push(frame_bytes);
        Ok(())
    }

    fn send_resource(&mut self, resource_id: &str, payload: Vec<u8>) -> anyhow::Result<()> {
        self.outgoing_resources
            .push((resource_id.to_owned(), payload));
        Ok(())
    }

    fn recv_frame(&mut self) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.incoming_frames.pop_front())
    }

    fn fetch_resource(&mut self, resource_id: &str) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.resources.get(resource_id).cloned())
    }

    fn defer_resource_offer(
        &mut self,
        resource_id: &str,
        frame_bytes: Vec<u8>,
    ) -> anyhow::Result<()> {
        self.pending_resource_offers
            .entry(resource_id.to_owned())
            .or_default()
            .push_back(frame_bytes);
        Ok(())
    }
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
pub(super) async fn run(input: OmenChatSmokeCommandInput) -> anyhow::Result<()> {
    use omenbrowser_rs::chat::{
        ChatClient, ChatClientEvent, ChatClientRequest, OmenChatDescriptor,
    };

    let OmenChatSmokeCommandInput {
        destination,
        room,
        message,
        announcement_rejection_smoke,
        reaction_smoke,
        revision_smoke,
        pin_smoke,
        upload_file,
        fetch_upload_filename,
        fetch_upload_bytes,
        reconnect_ready_file,
        reconnect_wait_secs,
        link_timeout_secs,
        response_wait_secs,
        warmup,
        output,
        stdout,
        overrides,
    } = input;
    parse_16_byte_hex_hash(&destination)?;

    let mut config = load_config_for_smoke(overrides.app_root().cloned())
        .context("failed to load OMENchat smoke app configuration")?;
    let known_destinations_path = overrides.known_destinations_path().cloned();
    let interface_override = apply_smoke_overrides(&mut config, overrides.clone());
    let default_output = output.is_none() && !stdout;
    let diagnostics_dir = config.paths.diagnostics_dir.clone();
    let mut app = App::try_new(config).context("failed to initialize application services")?;
    let mut runtime_events = app
        .runtime
        .subscribe_events()
        .ok_or_else(|| anyhow::anyhow!("configured runtime does not expose runtime events"))?;

    let mut stages = Vec::new();
    app.start_runtime_for_smoke_test_with_interfaces(interface_override)
        .await
        .context("failed to start runtime for OMENchat smoke")?;
    stages.push(serde_json::json!({
        "stage": "runtime_start",
        "ok": true,
        "status": app.runtime_status,
    }));

    if let Some(path) = known_destinations_path.clone() {
        let loaded = app
            .preload_known_destinations_for_smoke_test(&path)
            .await
            .with_context(|| {
                format!(
                    "failed to preload known destinations from {}",
                    path.display()
                )
            })?;
        stages.push(serde_json::json!({
            "stage": "known_destinations_preload",
            "ok": true,
            "path": path,
            "loaded": loaded,
        }));
    }

    if let Some(warmup) = warmup {
        let requested = app
            .runtime
            .request_path(&destination, "omenchat_smoke", true)
            .await;
        stages.push(serde_json::json!({
            "stage": "path_request",
            "ok": requested.is_ok(),
            "queued": requested.as_ref().ok().copied(),
            "error": requested.err().map(|error| error.to_string()),
            "wait_secs": warmup.wait_secs,
        }));
        let warmup_events = collect_runtime_trace(
            &mut runtime_events,
            Duration::from_secs(warmup.wait_secs),
            Some(&destination),
        )
        .await;
        stages.push(serde_json::json!({
            "stage": "path_wait",
            "ok": true,
            "events": warmup_events,
        }));
    }

    let opened = match app
        .runtime
        .open_omenchat_link(
            &destination,
            Duration::from_secs(link_timeout_secs),
            CancellationToken::new(),
        )
        .await
    {
        Ok(opened) => {
            stages.push(serde_json::json!({
                "stage": "link_open",
                "ok": true,
                "link_id": hex_bytes(&opened.link_id),
                "rtt_millis": opened.rtt_millis,
            }));
            opened
        }
        Err(error) => {
            stages.push(serde_json::json!({
                "stage": "link_open",
                "ok": false,
                "error": error.to_string(),
            }));
            let report = omenchat_smoke_report(
                false,
                "link_open",
                OmenChatSmokeReportContext {
                    destination: &destination,
                    room: &room,
                    message: &message,
                    announcement_rejection_smoke,
                },
                stages,
                None,
            );
            write_omenchat_smoke_report(report, output, stdout, default_output, &diagnostics_dir)?;
            let _ = app.runtime.stop_runtime().await;
            return Ok(());
        }
    };

    let mut client = ChatClient::new();
    let mut live_state = omenbrowser_rs::chat::live::LiveChatClientState::default();
    let client_instance_store =
        omenbrowser_rs::chat::client_instance::ClientInstanceIdStore::for_identity_storage_root(
            app.paths.identity_storage_root(),
        );
    let client_instance_id = client_instance_store.load_or_create().with_context(|| {
        format!(
            "failed to initialize isolated OMENchat smoke client instance at {}",
            client_instance_store.path().display()
        )
    })?;
    live_state.set_client_instance_id(Some(client_instance_id));
    let mut transport = OmenChatSmokeTransport::default();
    let descriptor = OmenChatDescriptor {
        server_destination: destination.clone(),
        display_name: Some("OMENchat smoke".into()),
        local_display_name: Some("OMENbrowser_rs smoke".into()),
        rooms_hint: vec![room.clone()],
        ..OmenChatDescriptor::default()
    };
    let open_events = omenbrowser_rs::chat::live::handle_live_request(
        &mut client,
        &mut live_state,
        &mut transport,
        ChatClientRequest::OpenServer(descriptor.clone()),
    );
    let session_id = open_events.iter().find_map(|event| match event {
        ChatClientEvent::ServerOpened { session_id, .. } => Some(*session_id),
        _ => None,
    });
    stages.push(serde_json::json!({
        "stage": "session_open_frames",
        "ok": session_id.is_some(),
        "events": open_events.iter().map(format_chat_event).collect::<Vec<_>>(),
    }));
    send_omenchat_smoke_outgoing(&*app.runtime, opened.link_id, &mut transport).await?;

    let Some(session_id) = session_id else {
        let report = omenchat_smoke_report(
            false,
            "session_open",
            OmenChatSmokeReportContext {
                destination: &destination,
                room: &room,
                message: &message,
                announcement_rejection_smoke,
            },
            stages,
            None,
        );
        write_omenchat_smoke_report(report, output, stdout, default_output, &diagnostics_dir)?;
        let _ = app.runtime.close_omenchat_link(opened.link_id).await;
        let _ = app.runtime.stop_runtime().await;
        return Ok(());
    };

    let join_events = wait_for_omenchat_condition(
        &*app.runtime,
        &mut runtime_events,
        &mut client,
        &mut live_state,
        &mut transport,
        OmenChatWaitOptions {
            link_id: opened.link_id,
            session_id,
            wait: Duration::from_secs(response_wait_secs),
        },
        |client| {
            client
                .session(session_id)
                .is_some_and(|session| session.active_room.joined)
        },
    )
    .await;
    send_omenchat_smoke_outgoing(&*app.runtime, opened.link_id, &mut transport).await?;
    let joined = client
        .session(session_id)
        .is_some_and(|session| session.active_room.joined);
    stages.push(serde_json::json!({
        "stage": "join_wait",
        "ok": joined,
        "events": join_events,
    }));
    stages.push(serde_json::json!({
        "stage": "capability_observation",
        "ok": true,
        "durable_mutations_negotiated": live_state.durable_mutations_negotiated(session_id),
        "durable_notice_ack_negotiated": live_state
            .durable_notice_ack_negotiated(session_id),
        "reply_mentions_negotiated": live_state.reply_mentions_negotiated(session_id),
        "reactions_negotiated": live_state.reactions_negotiated(session_id),
        "message_revisions_negotiated": live_state.message_revisions_negotiated(session_id),
        "local_user_id_bound": live_state.local_user_id(session_id).is_some(),
    }));

    if joined {
        let send_events = omenbrowser_rs::chat::live::handle_live_request(
            &mut client,
            &mut live_state,
            &mut transport,
            ChatClientRequest::SendMessage {
                session_id,
                room: room.clone(),
                body: message.clone(),
            },
        );
        stages.push(serde_json::json!({
            "stage": "message_send_frame",
            "ok": !send_events.iter().any(|event| matches!(event, ChatClientEvent::Error { .. })),
            "events": send_events.iter().map(format_chat_event).collect::<Vec<_>>(),
        }));
        send_omenchat_smoke_outgoing(&*app.runtime, opened.link_id, &mut transport).await?;
    }

    let message_events = if joined {
        wait_for_omenchat_condition(
            &*app.runtime,
            &mut runtime_events,
            &mut client,
            &mut live_state,
            &mut transport,
            OmenChatWaitOptions {
                link_id: opened.link_id,
                session_id,
                wait: Duration::from_secs(response_wait_secs),
            },
            |client| omenchat_session_contains_message(client, session_id, &message),
        )
        .await
    } else {
        Vec::new()
    };
    let message_seen = omenchat_session_contains_message(&client, session_id, &message);
    let announcement_rejected =
        omenchat_smoke_events_contain_announcement_policy_rejection(&message_events);
    stages.push(serde_json::json!({
        "stage": if announcement_rejection_smoke {
            "announcement_rejection_wait"
        } else {
            "message_echo_wait"
        },
        "ok": if announcement_rejection_smoke {
            announcement_rejected && !message_seen
        } else {
            message_seen
        },
        "announcement_rejected": announcement_rejected,
        "committed_message_seen": message_seen,
        "events": message_events,
    }));

    let mut reaction_ok = true;
    let mutation_identity_storage_root = app.paths.identity_storage_root();
    let mutation_identity_hash = if reaction_smoke || revision_smoke || pin_smoke {
        app.runtime_status
            .active_identity
            .as_ref()
            .map(|identity| parse_16_byte_hex_hash(&identity.hash_hex))
            .transpose()?
    } else {
        None
    };
    if joined && message_seen && reaction_smoke {
        let target_event_id = omenchat_session_message_event_id(&client, session_id, &message)
            .context("OMENchat reaction smoke target message was not retained")?;
        let authenticated_identity_hash =
            mutation_identity_hash.context("OMENchat reaction smoke has no active identity")?;
        let room_id = client
            .session(session_id)
            .map(|session| session.active_room.room_id)
            .unwrap_or(1);
        let (passed, reaction_stages) = run_omenchat_reaction_smoke(
            &*app.runtime,
            &mut runtime_events,
            &mut client,
            &mut live_state,
            &mut transport,
            OmenChatReactionSmokeOptions {
                link_id: opened.link_id,
                session_id,
                room_id,
                target_event_id,
                server_destination: &destination,
                identity_storage_root: &mutation_identity_storage_root,
                authenticated_identity_hash,
                wait: Duration::from_secs(response_wait_secs),
            },
        )
        .await?;
        reaction_ok = passed;
        stages.extend(reaction_stages);
    }

    let mut revision_ok = true;
    if joined && message_seen && revision_smoke {
        let target_event_id = omenchat_session_message_event_id(&client, session_id, &message)
            .context("OMENchat revision smoke target message was not retained")?;
        let authenticated_identity_hash =
            mutation_identity_hash.context("OMENchat revision smoke has no active identity")?;
        let room_id = client
            .session(session_id)
            .map(|session| session.active_room.room_id)
            .unwrap_or(1);
        let (passed, revision_stages) = run_omenchat_revision_smoke(
            &*app.runtime,
            &mut runtime_events,
            &mut client,
            &mut live_state,
            &mut transport,
            OmenChatRevisionSmokeOptions {
                link_id: opened.link_id,
                session_id,
                room_id,
                target_event_id,
                server_destination: &destination,
                identity_storage_root: &mutation_identity_storage_root,
                authenticated_identity_hash,
                wait: Duration::from_secs(response_wait_secs),
            },
        )
        .await?;
        revision_ok = passed;
        stages.extend(revision_stages);
    }

    let mut pin_ok = true;
    if joined && message_seen && pin_smoke {
        let target_event_id = omenchat_session_message_event_id(&client, session_id, &message)
            .context("OMENchat pin smoke target message was not retained")?;
        let authenticated_identity_hash =
            mutation_identity_hash.context("OMENchat pin smoke has no active identity")?;
        let room_id = client
            .session(session_id)
            .map(|session| session.active_room.room_id)
            .unwrap_or(1);
        let (passed, pin_stages) = run_omenchat_pin_smoke(
            &*app.runtime,
            &mut runtime_events,
            &mut client,
            &mut live_state,
            &mut transport,
            OmenChatPinSmokeOptions {
                link_id: opened.link_id,
                session_id,
                room_id,
                target_event_id,
                server_destination: &destination,
                identity_storage_root: &mutation_identity_storage_root,
                authenticated_identity_hash,
                wait: Duration::from_secs(response_wait_secs),
            },
        )
        .await?;
        pin_ok = passed;
        stages.extend(pin_stages);
    }

    let mut upload_ok = true;
    if joined && message_seen {
        if let Some(upload_file) = upload_file {
            let upload_bytes = std::fs::read(&upload_file).with_context(|| {
                format!(
                    "failed to read OMENchat smoke upload file {}",
                    upload_file.display()
                )
            })?;
            let upload_filename = upload_file
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| !name.trim().is_empty())
                .unwrap_or("omenchat-smoke-upload.bin")
                .to_owned();
            let upload_len = upload_bytes.len() as u64;
            let send_upload_events = omenbrowser_rs::chat::live::handle_live_request(
                &mut client,
                &mut live_state,
                &mut transport,
                ChatClientRequest::SendUpload {
                    session_id,
                    room: room.clone(),
                    filename: upload_filename.clone(),
                    content_type: Some("application/octet-stream".into()),
                    bytes: upload_bytes.clone(),
                },
            );
            stages.push(serde_json::json!({
                "stage": "upload_offer_frame",
                "ok": !send_upload_events.iter().any(|event| matches!(event, ChatClientEvent::Error { .. })),
                "filename": upload_filename.clone(),
                "bytes": upload_len,
                "events": send_upload_events.iter().map(format_chat_event).collect::<Vec<_>>(),
            }));
            send_omenchat_smoke_outgoing(&*app.runtime, opened.link_id, &mut transport).await?;

            let upload_complete_events = wait_for_omenchat_condition(
                &*app.runtime,
                &mut runtime_events,
                &mut client,
                &mut live_state,
                &mut transport,
                OmenChatWaitOptions {
                    link_id: opened.link_id,
                    session_id,
                    wait: Duration::from_secs(response_wait_secs),
                },
                |client| {
                    omenchat_session_upload_resource_id(
                        client,
                        session_id,
                        &upload_filename,
                        Some(upload_len),
                    )
                    .is_some()
                },
            )
            .await;
            let upload_resource_id = omenchat_session_upload_resource_id(
                &client,
                session_id,
                &upload_filename,
                Some(upload_len),
            );
            let upload_completed = upload_resource_id.is_some()
                && omenchat_smoke_events_contain_decoded_event(
                    &upload_complete_events,
                    "upload_completed",
                );
            stages.push(serde_json::json!({
                "stage": "upload_complete_wait",
                "ok": upload_completed,
                "resource_id": upload_resource_id.clone(),
                "events": upload_complete_events,
            }));

            if let Some(resource_id) = upload_resource_id {
                let (upload_resource_available, fetch_stages) =
                    run_omenchat_smoke_upload_fetch(OmenchatSmokeUploadFetch {
                        runtime: &*app.runtime,
                        runtime_events: &mut runtime_events,
                        link_id: opened.link_id,
                        client: &mut client,
                        live_state: &mut live_state,
                        transport: &mut transport,
                        session_id,
                        room: &room,
                        resource_id,
                        filename: &upload_filename,
                        bytes: Some(upload_len),
                        wait: Duration::from_secs(response_wait_secs),
                    })
                    .await?;
                stages.extend(fetch_stages);
                upload_ok = upload_completed && upload_resource_available;
            } else {
                upload_ok = false;
            }
        }
    }

    if joined && message_seen {
        if let Some(fetch_filename) = fetch_upload_filename {
            let existing_resource_id = omenchat_session_upload_resource_id(
                &client,
                session_id,
                &fetch_filename,
                fetch_upload_bytes,
            );
            stages.push(serde_json::json!({
                "stage": "existing_upload_lookup",
                "ok": existing_resource_id.is_some(),
                "filename": fetch_filename.clone(),
                "bytes": fetch_upload_bytes,
                "resource_id": existing_resource_id.clone(),
            }));
            if let Some(resource_id) = existing_resource_id {
                let (fetched_existing_upload, fetch_stages) =
                    run_omenchat_smoke_upload_fetch(OmenchatSmokeUploadFetch {
                        runtime: &*app.runtime,
                        runtime_events: &mut runtime_events,
                        link_id: opened.link_id,
                        client: &mut client,
                        live_state: &mut live_state,
                        transport: &mut transport,
                        session_id,
                        room: &room,
                        resource_id,
                        filename: &fetch_filename,
                        bytes: fetch_upload_bytes,
                        wait: Duration::from_secs(response_wait_secs),
                    })
                    .await?;
                stages.extend(fetch_stages);
                upload_ok = upload_ok && fetched_existing_upload;
            } else {
                upload_ok = false;
            }
        }
    }

    let mut active_link_id = opened.link_id;
    let mut reconnect_ok = true;
    if joined && message_seen {
        if let Some(ready_file) = reconnect_ready_file.as_deref() {
            let (passed, reconnected_link, reconnect_stages) =
                run_omenchat_continuous_reconnect_smoke(OmenChatContinuousReconnectSmoke {
                    runtime: &*app.runtime,
                    runtime_events: &mut runtime_events,
                    client: &mut client,
                    live_state: &mut live_state,
                    transport: &mut transport,
                    descriptor,
                    old_link_id: opened.link_id,
                    session_id,
                    room: &room,
                    message: &message,
                    ready_file,
                    wait: Duration::from_secs(reconnect_wait_secs),
                    link_timeout: Duration::from_secs(link_timeout_secs),
                    response_wait: Duration::from_secs(response_wait_secs),
                    reaction_smoke,
                    revision_smoke,
                    pin_smoke,
                    server_destination: &destination,
                    identity_storage_root: &mutation_identity_storage_root,
                    authenticated_identity_hash: mutation_identity_hash,
                })
                .await?;
            reconnect_ok = passed;
            stages.extend(reconnect_stages);
            if let Some(link_id) = reconnected_link {
                active_link_id = link_id;
            }
        }
    }

    let session_summary = client.session(session_id).map(|session| {
        serde_json::json!({
            "session_id": session.session_id,
            "server": {
                "server_id": session.server.server_id.clone(),
                "destination": session.server.destination.clone(),
                "display_name": session.server.display_name.clone(),
            },
            "room": {
                "server_id": session.active_room.server_id.clone(),
                "room_id": session.active_room.room_id,
                "name": session.active_room.name.clone(),
                "unread": session.active_room.unread,
                "joined": session.active_room.joined,
            },
            "user_count": session.users.len(),
            "event_count": session.events.len(),
            "status": session.status.clone(),
        })
    });
    let message_outcome = if announcement_rejection_smoke {
        announcement_rejected && !message_seen
    } else {
        message_seen
    };
    let outcome = joined
        && message_outcome
        && reaction_ok
        && revision_ok
        && pin_ok
        && upload_ok
        && reconnect_ok;
    let failed_stage = if !joined {
        "join_wait"
    } else if !message_outcome && announcement_rejection_smoke {
        "announcement_rejection_wait"
    } else if !message_outcome {
        "message_echo_wait"
    } else if !reaction_ok {
        "reaction_smoke"
    } else if !revision_ok {
        "revision_smoke"
    } else if !pin_ok {
        "pin_smoke"
    } else if !upload_ok {
        "upload_fetch_wait"
    } else if !reconnect_ok {
        "continuous_reconnect"
    } else {
        "complete"
    };
    let report = omenchat_smoke_report(
        outcome,
        failed_stage,
        OmenChatSmokeReportContext {
            destination: &destination,
            room: &room,
            message: &message,
            announcement_rejection_smoke,
        },
        stages,
        session_summary,
    );
    write_omenchat_smoke_report(report, output, stdout, default_output, &diagnostics_dir)?;
    let _ = app.runtime.close_omenchat_link(active_link_id).await;
    let _ = app.runtime.stop_runtime().await;
    Ok(())
}

#[cfg(not(any(feature = "chat-client-rns", feature = "chat-client-rns-clean")))]
pub(super) async fn run(_input: OmenChatSmokeCommandInput) -> anyhow::Result<()> {
    anyhow::bail!("OMENchat smoke requires --features chat-client-rns or chat-client-rns-clean")
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
async fn send_omenchat_smoke_outgoing(
    runtime: &dyn omenbrowser_rs::runtime::NetworkRuntime,
    link_id: [u8; 16],
    transport: &mut OmenChatSmokeTransport,
) -> anyhow::Result<()> {
    for frame in transport.take_outgoing_frames() {
        runtime
            .send_omenchat_frame(link_id, frame)
            .await
            .context("failed to send OMENchat smoke frame")?;
    }
    for (resource_id, payload) in transport.take_outgoing_resources() {
        runtime
            .send_omenchat_resource(link_id, resource_id, payload)
            .await
            .context("failed to send OMENchat smoke resource")?;
    }
    Ok(())
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
#[derive(Clone, Copy)]
struct OmenChatWaitOptions {
    link_id: [u8; 16],
    session_id: omenbrowser_rs::chat::ChatSessionId,
    wait: Duration,
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
async fn wait_for_omenchat_condition(
    runtime: &dyn omenbrowser_rs::runtime::NetworkRuntime,
    runtime_events: &mut tokio::sync::broadcast::Receiver<RuntimeBusEvent>,
    client: &mut omenbrowser_rs::chat::ChatClient,
    live_state: &mut omenbrowser_rs::chat::live::LiveChatClientState,
    transport: &mut OmenChatSmokeTransport,
    options: OmenChatWaitOptions,
    condition: impl Fn(&omenbrowser_rs::chat::ChatClient) -> bool,
) -> Vec<serde_json::Value> {
    let OmenChatWaitOptions {
        link_id,
        session_id,
        wait,
    } = options;
    let deadline = tokio::time::Instant::now() + wait;
    let mut events = Vec::new();
    let mut announcement_policy_rejected = false;
    while tokio::time::Instant::now() < deadline
        && !condition(client)
        && !announcement_policy_rejected
    {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let received = tokio::time::timeout(remaining, runtime_events.recv()).await;
        let event = match received {
            Ok(Ok(event)) => event,
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(count))) => {
                events.push(serde_json::json!({
                    "event": "lagged",
                    "count": count,
                }));
                continue;
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                events.push(serde_json::json!({"event": "closed"}));
                break;
            }
            Err(_) => {
                events.push(serde_json::json!({"event": "timeout"}));
                break;
            }
        };
        match event {
            RuntimeBusEvent::OmenChatLinkData(data) if data.link_id == link_id => {
                let bytes = data.frame_bytes.len();
                transport.push_incoming_frame(data.frame_bytes);
                let decoded = omenbrowser_rs::chat::live::drain_live_events_with_state(
                    client,
                    live_state,
                    transport,
                    Some(session_id),
                );
                announcement_policy_rejected =
                    decoded.iter().any(is_announcement_policy_rejection_event);
                events.push(serde_json::json!({
                    "event": "link_data",
                    "bytes": bytes,
                    "decoded": decoded.iter().map(format_chat_event).collect::<Vec<_>>(),
                }));
                if let Err(error) = send_omenchat_smoke_outgoing(runtime, link_id, transport).await
                {
                    events.push(serde_json::json!({
                        "event": "flush_error",
                        "error": error.to_string(),
                    }));
                    break;
                }
            }
            RuntimeBusEvent::OmenChatResourceData(data) if data.link_id == link_id => {
                let bytes = data.data.len();
                let metadata_len = data.metadata.as_ref().map_or(0, Vec::len);
                transport.push_resource(data.metadata, data.data);
                let decoded = omenbrowser_rs::chat::live::drain_live_events_with_state(
                    client,
                    live_state,
                    transport,
                    Some(session_id),
                );
                announcement_policy_rejected =
                    decoded.iter().any(is_announcement_policy_rejection_event);
                events.push(serde_json::json!({
                    "event": "resource_data",
                    "bytes": bytes,
                    "metadata_len": metadata_len,
                    "decoded": decoded.iter().map(format_chat_event).collect::<Vec<_>>(),
                }));
                if let Err(error) = send_omenchat_smoke_outgoing(runtime, link_id, transport).await
                {
                    events.push(serde_json::json!({
                        "event": "flush_error",
                        "error": error.to_string(),
                    }));
                    break;
                }
            }
            RuntimeBusEvent::Debug(message) => {
                events.push(serde_json::json!({"event": "debug", "message": message}));
            }
            RuntimeBusEvent::Error(message) => {
                events.push(serde_json::json!({"event": "error", "message": message}));
            }
            RuntimeBusEvent::PathUpdated(path) => {
                events.push(serde_json::json!({"event": "path", "value": path}));
            }
            RuntimeBusEvent::Announce(announce) => {
                events.push(serde_json::json!({"event": "announce", "value": announce}));
            }
            _ => {}
        }
    }
    events
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
struct OmenChatContinuousReconnectSmoke<'a> {
    runtime: &'a dyn omenbrowser_rs::runtime::NetworkRuntime,
    runtime_events: &'a mut tokio::sync::broadcast::Receiver<RuntimeBusEvent>,
    client: &'a mut omenbrowser_rs::chat::ChatClient,
    live_state: &'a mut omenbrowser_rs::chat::live::LiveChatClientState,
    transport: &'a mut OmenChatSmokeTransport,
    descriptor: omenbrowser_rs::chat::OmenChatDescriptor,
    old_link_id: [u8; 16],
    session_id: omenbrowser_rs::chat::ChatSessionId,
    room: &'a str,
    message: &'a str,
    ready_file: &'a std::path::Path,
    wait: Duration,
    link_timeout: Duration,
    response_wait: Duration,
    reaction_smoke: bool,
    revision_smoke: bool,
    pin_smoke: bool,
    server_destination: &'a str,
    identity_storage_root: &'a std::path::Path,
    authenticated_identity_hash: Option<[u8; 16]>,
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
async fn run_omenchat_continuous_reconnect_smoke(
    input: OmenChatContinuousReconnectSmoke<'_>,
) -> anyhow::Result<(bool, Option<[u8; 16]>, Vec<serde_json::Value>)> {
    use omenbrowser_rs::chat::{ChatClientEvent, ChatClientRequest};

    create_omenchat_reconnect_ready_file(input.ready_file)?;
    let deadline = tokio::time::Instant::now() + input.wait;
    let mut stages = Vec::new();
    let mut close_seen = false;
    while tokio::time::Instant::now() < deadline && !close_seen {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, input.runtime_events.recv()).await {
            Ok(Ok(RuntimeBusEvent::OmenChatLinkClosed(closed)))
                if closed.link_id == input.old_link_id =>
            {
                close_seen = true;
                stages.push(serde_json::json!({
                    "stage": "continuous_link_close_wait",
                    "ok": true,
                    "reason": closed.reason,
                }));
            }
            Ok(Ok(_)) | Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => {}
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) | Err(_) => break,
        }
    }
    if !close_seen {
        stages.push(serde_json::json!({
            "stage": "continuous_link_close_wait",
            "ok": false,
            "reason": "old link did not close before the bounded deadline",
        }));
        return Ok((false, None, stages));
    }

    let mut opened = None;
    let mut attempts = 0u8;
    while tokio::time::Instant::now() < deadline && attempts < 6 {
        attempts = attempts.saturating_add(1);
        let _ = input
            .runtime
            .request_path(
                &input.descriptor.server_destination,
                "omenchat_continuous_reconnect_smoke",
                true,
            )
            .await;
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let attempt_timeout = input.link_timeout.min(remaining);
        if attempt_timeout.is_zero() {
            break;
        }
        match input
            .runtime
            .open_omenchat_link(
                &input.descriptor.server_destination,
                attempt_timeout,
                CancellationToken::new(),
            )
            .await
        {
            Ok(link) => {
                stages.push(serde_json::json!({
                    "stage": "continuous_link_reopen",
                    "ok": true,
                    "attempt": attempts,
                    "link_changed": link.link_id != input.old_link_id,
                }));
                opened = Some(link);
                break;
            }
            Err(error) => {
                stages.push(serde_json::json!({
                    "stage": "continuous_link_reopen_attempt",
                    "ok": false,
                    "attempt": attempts,
                    "error": error.to_string(),
                }));
                let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                if remaining.is_zero() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(500).min(remaining)).await;
            }
        }
    }
    let Some(opened) = opened else {
        stages.push(serde_json::json!({
            "stage": "continuous_link_reopen",
            "ok": false,
            "attempts": attempts,
        }));
        return Ok((false, None, stages));
    };

    *input.transport = OmenChatSmokeTransport::default();
    let reconnect_events = omenbrowser_rs::chat::live::reconnect_live_server(
        input.client,
        input.live_state,
        input.transport,
        input.session_id,
        input.descriptor,
    );
    let reconnect_started = !reconnect_events
        .iter()
        .any(|event| matches!(event, ChatClientEvent::Error { .. }));
    stages.push(serde_json::json!({
        "stage": "continuous_session_reconnect",
        "ok": reconnect_started,
        "events": reconnect_events.iter().map(format_chat_event).collect::<Vec<_>>(),
    }));
    send_omenchat_smoke_outgoing(input.runtime, opened.link_id, input.transport).await?;

    let reconnect_message = format!("{} (after continuous reconnect)", input.message);
    let send_events = omenbrowser_rs::chat::live::handle_live_request(
        input.client,
        input.live_state,
        input.transport,
        ChatClientRequest::SendMessage {
            session_id: input.session_id,
            room: input.room.to_owned(),
            body: reconnect_message.clone(),
        },
    );
    let send_ok = !send_events
        .iter()
        .any(|event| matches!(event, ChatClientEvent::Error { .. }));
    stages.push(serde_json::json!({
        "stage": "continuous_message_send",
        "ok": send_ok,
        "events": send_events.iter().map(format_chat_event).collect::<Vec<_>>(),
    }));
    send_omenchat_smoke_outgoing(input.runtime, opened.link_id, input.transport).await?;

    let message_events = wait_for_omenchat_condition(
        input.runtime,
        input.runtime_events,
        input.client,
        input.live_state,
        input.transport,
        OmenChatWaitOptions {
            link_id: opened.link_id,
            session_id: input.session_id,
            wait: input.response_wait,
        },
        |client| omenchat_session_contains_message(client, input.session_id, &reconnect_message),
    )
    .await;
    let message_seen =
        omenchat_session_contains_message(input.client, input.session_id, &reconnect_message);
    stages.push(serde_json::json!({
        "stage": "continuous_message_echo_wait",
        "ok": message_seen,
        "events": message_events,
    }));
    let mut reaction_ok = true;
    if input.reaction_smoke && message_seen {
        let target_event_id =
            omenchat_session_message_event_id(input.client, input.session_id, &reconnect_message)
                .context("continuous reconnect reaction target message was not retained")?;
        let room_id = input
            .client
            .session(input.session_id)
            .map(|session| session.active_room.room_id)
            .unwrap_or(1);
        let Some(authenticated_identity_hash) = input.authenticated_identity_hash else {
            stages.push(serde_json::json!({
                "stage": "continuous_reaction_identity",
                "ok": false,
                "reason": "active identity was unavailable",
            }));
            return Ok((false, Some(opened.link_id), stages));
        };
        let (passed, reaction_stages) = run_omenchat_reaction_smoke(
            input.runtime,
            input.runtime_events,
            input.client,
            input.live_state,
            input.transport,
            OmenChatReactionSmokeOptions {
                link_id: opened.link_id,
                session_id: input.session_id,
                room_id,
                target_event_id,
                server_destination: input.server_destination,
                identity_storage_root: input.identity_storage_root,
                authenticated_identity_hash,
                wait: input.response_wait,
            },
        )
        .await?;
        reaction_ok = passed;
        stages.extend(reaction_stages.into_iter().map(|mut stage| {
            if let Some(name) = stage
                .get("stage")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
            {
                stage["stage"] = serde_json::Value::String(format!("continuous_{name}"));
            }
            stage
        }));
    }
    let mut revision_ok = true;
    if input.revision_smoke && message_seen {
        let target_event_id =
            omenchat_session_message_event_id(input.client, input.session_id, &reconnect_message)
                .context("continuous reconnect revision target message was not retained")?;
        let room_id = input
            .client
            .session(input.session_id)
            .map(|session| session.active_room.room_id)
            .unwrap_or(1);
        let Some(authenticated_identity_hash) = input.authenticated_identity_hash else {
            stages.push(serde_json::json!({
                "stage": "continuous_revision_identity",
                "ok": false,
                "reason": "active identity was unavailable",
            }));
            return Ok((false, Some(opened.link_id), stages));
        };
        let (passed, revision_stages) = run_omenchat_revision_smoke(
            input.runtime,
            input.runtime_events,
            input.client,
            input.live_state,
            input.transport,
            OmenChatRevisionSmokeOptions {
                link_id: opened.link_id,
                session_id: input.session_id,
                room_id,
                target_event_id,
                server_destination: input.server_destination,
                identity_storage_root: input.identity_storage_root,
                authenticated_identity_hash,
                wait: input.response_wait,
            },
        )
        .await?;
        revision_ok = passed;
        stages.extend(revision_stages.into_iter().map(|mut stage| {
            if let Some(name) = stage
                .get("stage")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
            {
                stage["stage"] = serde_json::Value::String(format!("continuous_{name}"));
            }
            stage
        }));
    }
    let mut pin_ok = true;
    if input.pin_smoke && message_seen {
        let target_event_id =
            omenchat_session_message_event_id(input.client, input.session_id, &reconnect_message)
                .context("continuous reconnect pin target message was not retained")?;
        let room_id = input
            .client
            .session(input.session_id)
            .map(|session| session.active_room.room_id)
            .unwrap_or(1);
        let Some(authenticated_identity_hash) = input.authenticated_identity_hash else {
            stages.push(serde_json::json!({
                "stage": "continuous_pin_identity",
                "ok": false,
                "reason": "active identity was unavailable",
            }));
            return Ok((false, Some(opened.link_id), stages));
        };
        let (passed, pin_stages) = run_omenchat_pin_smoke(
            input.runtime,
            input.runtime_events,
            input.client,
            input.live_state,
            input.transport,
            OmenChatPinSmokeOptions {
                link_id: opened.link_id,
                session_id: input.session_id,
                room_id,
                target_event_id,
                server_destination: input.server_destination,
                identity_storage_root: input.identity_storage_root,
                authenticated_identity_hash,
                wait: input.response_wait,
            },
        )
        .await?;
        pin_ok = passed;
        stages.extend(pin_stages.into_iter().map(|mut stage| {
            if let Some(name) = stage
                .get("stage")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
            {
                stage["stage"] = serde_json::Value::String(format!("continuous_{name}"));
            }
            stage
        }));
    }
    Ok((
        reconnect_started
            && send_ok
            && message_seen
            && reaction_ok
            && revision_ok
            && pin_ok
            && opened.link_id != input.old_link_id,
        Some(opened.link_id),
        stages,
    ))
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
struct OmenChatReactionSmokeOptions<'a> {
    link_id: [u8; 16],
    session_id: omenbrowser_rs::chat::ChatSessionId,
    room_id: u32,
    target_event_id: u64,
    server_destination: &'a str,
    identity_storage_root: &'a std::path::Path,
    authenticated_identity_hash: [u8; 16],
    wait: Duration,
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn prepare_omenchat_smoke_reaction(
    store: &omenbrowser_rs::chat::mutation_intents::MutationIntentStore,
    options: &OmenChatReactionSmokeOptions<'_>,
    client_instance_id: omenbrowser_rs::chat::protocol::ClientInstanceId,
    action: omenbrowser_rs::chat::protocol::ReactionAction,
) -> anyhow::Result<omenbrowser_rs::chat::mutation_intents::OutboundMutationIntent> {
    use omenbrowser_rs::chat::mutation_intents::{
        IntentTransition, OutboundMutationState, PrepareOutboundMutation,
    };
    use omenbrowser_rs::chat::protocol::{ChatOp, ReactionRequest, ReactionToken};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default();
    let body = ReactionRequest {
        target_event_id: options.target_event_id,
        token: ReactionToken::Heart,
        action,
    }
    .into_frame_body()
    .context("encode OMENchat smoke reaction")?;
    let prepared = store.persist_prepared(PrepareOutboundMutation {
        server_destination: options.server_destination,
        authenticated_identity_hash: &options.authenticated_identity_hash,
        client_instance_id,
        op: ChatOp::RoomReaction,
        room_id: Some(options.room_id),
        body,
        created_at: now,
        expires_at: now.saturating_add(60 * 60),
        correlation_id: Some("release-reaction-smoke"),
    })?;
    match store.transition(
        prepared.mutation_id,
        OutboundMutationState::Prepared,
        OutboundMutationState::SentUncertain,
    )? {
        IntentTransition::Updated(intent) => Ok(intent),
        other => anyhow::bail!("OMENchat smoke reaction transition failed: {other:?}"),
    }
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
async fn send_omenchat_smoke_reaction(
    runtime: &dyn omenbrowser_rs::runtime::NetworkRuntime,
    client: &mut omenbrowser_rs::chat::ChatClient,
    live_state: &mut omenbrowser_rs::chat::live::LiveChatClientState,
    transport: &mut OmenChatSmokeTransport,
    options: &OmenChatReactionSmokeOptions<'_>,
    intent: &omenbrowser_rs::chat::mutation_intents::OutboundMutationIntent,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let events = omenbrowser_rs::chat::live::send_uncertain_durable_reaction(
        client,
        live_state,
        transport,
        options.session_id,
        intent,
    );
    if events
        .iter()
        .any(|event| matches!(event, omenbrowser_rs::chat::ChatClientEvent::Error { .. }))
    {
        anyhow::bail!("OMENchat smoke reaction was rejected before transmission");
    }
    send_omenchat_smoke_outgoing(runtime, options.link_id, transport).await?;
    Ok(events.iter().map(format_chat_event).collect())
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
async fn discard_omenchat_reaction_ack(
    runtime_events: &mut tokio::sync::broadcast::Receiver<RuntimeBusEvent>,
    link_id: [u8; 16],
    wait: Duration,
) -> anyhow::Result<serde_json::Value> {
    let deadline = tokio::time::Instant::now() + wait;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, runtime_events.recv()).await {
            Ok(Ok(RuntimeBusEvent::OmenChatLinkData(data))) if data.link_id == link_id => {
                let frame = omenbrowser_rs::chat::codec::decode_frame(&data.frame_bytes)
                    .context("decode deliberately discarded OMENchat reaction response")?;
                if frame.op == omenbrowser_rs::chat::protocol::ChatOp::ReactionAck {
                    return Ok(serde_json::json!({
                        "stage": "reaction_lost_ack",
                        "ok": true,
                        "bytes": data.frame_bytes.len(),
                        "sequence": frame.seq,
                    }));
                }
            }
            Ok(Ok(_)) | Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => {}
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) | Err(_) => break,
        }
    }
    anyhow::bail!("OMENchat smoke did not observe the acknowledgement selected for loss")
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
async fn run_omenchat_reaction_smoke(
    runtime: &dyn omenbrowser_rs::runtime::NetworkRuntime,
    runtime_events: &mut tokio::sync::broadcast::Receiver<RuntimeBusEvent>,
    client: &mut omenbrowser_rs::chat::ChatClient,
    live_state: &mut omenbrowser_rs::chat::live::LiveChatClientState,
    transport: &mut OmenChatSmokeTransport,
    options: OmenChatReactionSmokeOptions<'_>,
) -> anyhow::Result<(bool, Vec<serde_json::Value>)> {
    use omenbrowser_rs::chat::mutation_intents::{MutationIntentStore, OutboundMutationState};
    use omenbrowser_rs::chat::protocol::{ReactionAction, ReactionToken};
    use omenbrowser_rs::chat::ChatClientRequest;

    let mut stages = Vec::new();
    let negotiated = live_state.durable_mutations_negotiated(options.session_id)
        && live_state.reactions_negotiated(options.session_id);
    stages.push(serde_json::json!({
        "stage": "reaction_capability",
        "ok": negotiated,
    }));
    let Some(client_instance_id) = live_state.client_instance_id() else {
        return Ok((false, stages));
    };
    if !negotiated {
        return Ok((false, stages));
    }
    let store = MutationIntentStore::open_for_identity_storage_root(options.identity_storage_root)
        .context("open isolated OMENchat smoke mutation store")?;

    let add =
        prepare_omenchat_smoke_reaction(&store, &options, client_instance_id, ReactionAction::Add)?;
    let sent = send_omenchat_smoke_reaction(runtime, client, live_state, transport, &options, &add)
        .await?;
    stages.push(serde_json::json!({
        "stage": "reaction_add_send",
        "ok": true,
        "events": sent,
    }));
    stages
        .push(discard_omenchat_reaction_ack(runtime_events, options.link_id, options.wait).await?);

    let replayed =
        send_omenchat_smoke_reaction(runtime, client, live_state, transport, &options, &add)
            .await?;
    let replay_events = wait_for_omenchat_condition(
        runtime,
        runtime_events,
        client,
        live_state,
        transport,
        OmenChatWaitOptions {
            link_id: options.link_id,
            session_id: options.session_id,
            wait: options.wait,
        },
        |client| {
            client
                .session(options.session_id)
                .is_some_and(|session| session.status == "reaction accepted by server")
        },
    )
    .await;
    let replay_acknowledged = client
        .session(options.session_id)
        .is_some_and(|session| session.status == "reaction accepted by server");
    stages.push(serde_json::json!({
        "stage": "reaction_exact_replay",
        "ok": replay_acknowledged,
        "send_events": replayed,
        "events": replay_events,
    }));
    if replay_acknowledged {
        let _ = store.transition(
            add.mutation_id,
            OutboundMutationState::SentUncertain,
            OutboundMutationState::Acknowledged,
        )?;
    }

    let sync_events = omenbrowser_rs::chat::live::handle_live_request(
        client,
        live_state,
        transport,
        ChatClientRequest::SyncRecent {
            session_id: options.session_id,
        },
    );
    send_omenchat_smoke_outgoing(runtime, options.link_id, transport).await?;
    let snapshot_events = wait_for_omenchat_condition(
        runtime,
        runtime_events,
        client,
        live_state,
        transport,
        OmenChatWaitOptions {
            link_id: options.link_id,
            session_id: options.session_id,
            wait: options.wait,
        },
        |client| {
            client
                .reactions_for_targets(
                    options.session_id,
                    options.room_id,
                    &[options.target_event_id],
                )
                .iter()
                .any(|reaction| reaction.token == ReactionToken::Heart)
        },
    )
    .await;
    let resource_snapshot = snapshot_events.iter().any(|event| {
        event.get("event").and_then(serde_json::Value::as_str) == Some("resource_data")
            && omenchat_smoke_events_contain_decoded_event(
                std::slice::from_ref(event),
                "reaction_snapshot_applied",
            )
    });
    stages.push(serde_json::json!({
        "stage": "reaction_resource_snapshot",
        "ok": resource_snapshot,
        "request_events": sync_events.iter().map(format_chat_event).collect::<Vec<_>>(),
        "events": snapshot_events,
    }));

    let no_op =
        prepare_omenchat_smoke_reaction(&store, &options, client_instance_id, ReactionAction::Add)?;
    let _ = send_omenchat_smoke_reaction(runtime, client, live_state, transport, &options, &no_op)
        .await?;
    let no_op_events = wait_for_omenchat_condition(
        runtime,
        runtime_events,
        client,
        live_state,
        transport,
        OmenChatWaitOptions {
            link_id: options.link_id,
            session_id: options.session_id,
            wait: options.wait,
        },
        |client| {
            client.session(options.session_id).is_some_and(|session| {
                session.status == "reaction already matched the requested state"
            })
        },
    )
    .await;
    let no_op_ok = client
        .session(options.session_id)
        .is_some_and(|session| session.status == "reaction already matched the requested state");
    if no_op_ok {
        let _ = store.transition(
            no_op.mutation_id,
            OutboundMutationState::SentUncertain,
            OutboundMutationState::Acknowledged,
        )?;
    }
    stages.push(serde_json::json!({
        "stage": "reaction_noop_add",
        "ok": no_op_ok,
        "events": no_op_events,
    }));

    let remove = prepare_omenchat_smoke_reaction(
        &store,
        &options,
        client_instance_id,
        ReactionAction::Remove,
    )?;
    let _ = send_omenchat_smoke_reaction(runtime, client, live_state, transport, &options, &remove)
        .await?;
    let remove_events = wait_for_omenchat_condition(
        runtime,
        runtime_events,
        client,
        live_state,
        transport,
        OmenChatWaitOptions {
            link_id: options.link_id,
            session_id: options.session_id,
            wait: options.wait,
        },
        |client| {
            client
                .session(options.session_id)
                .is_some_and(|session| session.status == "reaction accepted by server")
        },
    )
    .await;
    let remove_ok = client
        .session(options.session_id)
        .is_some_and(|session| session.status == "reaction accepted by server");
    if remove_ok {
        let _ = store.transition(
            remove.mutation_id,
            OutboundMutationState::SentUncertain,
            OutboundMutationState::Acknowledged,
        )?;
    }
    stages.push(serde_json::json!({
        "stage": "reaction_remove",
        "ok": remove_ok,
        "events": remove_events,
    }));

    let remove_sync_events = omenbrowser_rs::chat::live::handle_live_request(
        client,
        live_state,
        transport,
        ChatClientRequest::SyncRecent {
            session_id: options.session_id,
        },
    );
    send_omenchat_smoke_outgoing(runtime, options.link_id, transport).await?;
    let remove_snapshot_events = wait_for_omenchat_condition(
        runtime,
        runtime_events,
        client,
        live_state,
        transport,
        OmenChatWaitOptions {
            link_id: options.link_id,
            session_id: options.session_id,
            wait: options.wait,
        },
        |client| {
            client.session(options.session_id).is_some_and(|session| {
                session.status != "requested recent room history"
                    && client
                        .reactions_for_targets(
                            options.session_id,
                            options.room_id,
                            &[options.target_event_id],
                        )
                        .iter()
                        .all(|reaction| reaction.token != ReactionToken::Heart)
            })
        },
    )
    .await;
    let remove_snapshot_ok = client
        .reactions_for_targets(
            options.session_id,
            options.room_id,
            &[options.target_event_id],
        )
        .iter()
        .all(|reaction| reaction.token != ReactionToken::Heart);
    stages.push(serde_json::json!({
        "stage": "reaction_remove_snapshot",
        "ok": remove_snapshot_ok,
        "request_events": remove_sync_events.iter().map(format_chat_event).collect::<Vec<_>>(),
        "events": remove_snapshot_events,
    }));

    let recovered = store.recover_nonterminal()?;
    let persistence_ok = recovered.is_empty();
    stages.push(serde_json::json!({
        "stage": "reaction_intent_persistence",
        "ok": persistence_ok,
        "nonterminal_count": recovered.len(),
    }));
    Ok((
        replay_acknowledged
            && resource_snapshot
            && no_op_ok
            && remove_ok
            && remove_snapshot_ok
            && persistence_ok,
        stages,
    ))
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
struct OmenChatPinSmokeOptions<'a> {
    link_id: [u8; 16],
    session_id: omenbrowser_rs::chat::ChatSessionId,
    room_id: u32,
    target_event_id: u64,
    server_destination: &'a str,
    identity_storage_root: &'a std::path::Path,
    authenticated_identity_hash: [u8; 16],
    wait: Duration,
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn prepare_omenchat_smoke_pin(
    store: &omenbrowser_rs::chat::mutation_intents::MutationIntentStore,
    options: &OmenChatPinSmokeOptions<'_>,
    client_instance_id: omenbrowser_rs::chat::protocol::ClientInstanceId,
    action: omenbrowser_rs::chat::protocol::PinAction,
) -> anyhow::Result<omenbrowser_rs::chat::mutation_intents::OutboundMutationIntent> {
    use omenbrowser_rs::chat::mutation_intents::{
        IntentTransition, OutboundMutationState, PrepareOutboundMutation,
    };
    use omenbrowser_rs::chat::protocol::{ChatOp, PinRequest};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default();
    let body = PinRequest {
        target_event_id: options.target_event_id,
        action,
    }
    .into_frame_body()
    .context("encode OMENchat smoke pin")?;
    let prepared = store.persist_prepared(PrepareOutboundMutation {
        server_destination: options.server_destination,
        authenticated_identity_hash: &options.authenticated_identity_hash,
        client_instance_id,
        op: ChatOp::RoomPin,
        room_id: Some(options.room_id),
        body,
        created_at: now,
        expires_at: now.saturating_add(60 * 60),
        correlation_id: Some("release-pin-smoke"),
    })?;
    match store.transition(
        prepared.mutation_id,
        OutboundMutationState::Prepared,
        OutboundMutationState::SentUncertain,
    )? {
        IntentTransition::Updated(intent) => Ok(intent),
        other => anyhow::bail!("OMENchat smoke pin transition failed: {other:?}"),
    }
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
async fn send_omenchat_smoke_pin(
    runtime: &dyn omenbrowser_rs::runtime::NetworkRuntime,
    client: &mut omenbrowser_rs::chat::ChatClient,
    live_state: &mut omenbrowser_rs::chat::live::LiveChatClientState,
    transport: &mut OmenChatSmokeTransport,
    options: &OmenChatPinSmokeOptions<'_>,
    intent: &omenbrowser_rs::chat::mutation_intents::OutboundMutationIntent,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let events = omenbrowser_rs::chat::live::send_uncertain_durable_pin(
        client,
        live_state,
        transport,
        options.session_id,
        intent,
    );
    if events
        .iter()
        .any(|event| matches!(event, omenbrowser_rs::chat::ChatClientEvent::Error { .. }))
    {
        anyhow::bail!("OMENchat smoke pin was rejected before transmission");
    }
    send_omenchat_smoke_outgoing(runtime, options.link_id, transport).await?;
    Ok(events.iter().map(format_chat_event).collect())
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
async fn discard_omenchat_pin_ack(
    runtime_events: &mut tokio::sync::broadcast::Receiver<RuntimeBusEvent>,
    link_id: [u8; 16],
    wait: Duration,
) -> anyhow::Result<serde_json::Value> {
    let deadline = tokio::time::Instant::now() + wait;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, runtime_events.recv()).await {
            Ok(Ok(RuntimeBusEvent::OmenChatLinkData(data))) if data.link_id == link_id => {
                let frame = omenbrowser_rs::chat::codec::decode_frame(&data.frame_bytes)
                    .context("decode deliberately discarded OMENchat pin response")?;
                if frame.op == omenbrowser_rs::chat::protocol::ChatOp::PinAck {
                    return Ok(serde_json::json!({
                        "stage": "pin_lost_ack",
                        "ok": true,
                        "bytes": data.frame_bytes.len(),
                        "sequence": frame.seq,
                    }));
                }
            }
            Ok(Ok(_)) | Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => {}
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) | Err(_) => break,
        }
    }
    anyhow::bail!("OMENchat smoke did not observe the pin acknowledgement selected for loss")
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
async fn run_omenchat_pin_smoke(
    runtime: &dyn omenbrowser_rs::runtime::NetworkRuntime,
    runtime_events: &mut tokio::sync::broadcast::Receiver<RuntimeBusEvent>,
    client: &mut omenbrowser_rs::chat::ChatClient,
    live_state: &mut omenbrowser_rs::chat::live::LiveChatClientState,
    transport: &mut OmenChatSmokeTransport,
    options: OmenChatPinSmokeOptions<'_>,
) -> anyhow::Result<(bool, Vec<serde_json::Value>)> {
    use omenbrowser_rs::chat::mutation_intents::{MutationIntentStore, OutboundMutationState};
    use omenbrowser_rs::chat::protocol::PinAction;
    use omenbrowser_rs::chat::ChatClientRequest;

    let mut stages = Vec::new();
    let negotiated = live_state.durable_mutations_negotiated(options.session_id)
        && live_state.pins_negotiated(options.session_id);
    stages.push(serde_json::json!({
        "stage": "pin_capability",
        "ok": negotiated,
    }));
    let Some(client_instance_id) = live_state.client_instance_id() else {
        return Ok((false, stages));
    };
    if !negotiated {
        return Ok((false, stages));
    }
    let store = MutationIntentStore::open_for_identity_storage_root(options.identity_storage_root)
        .context("open isolated OMENchat pin smoke mutation store")?;

    let authority_request = omenbrowser_rs::chat::live::handle_live_request(
        client,
        live_state,
        transport,
        ChatClientRequest::SyncRecent {
            session_id: options.session_id,
        },
    );
    send_omenchat_smoke_outgoing(runtime, options.link_id, transport).await?;
    let authority_events = wait_for_omenchat_condition(
        runtime,
        runtime_events,
        client,
        live_state,
        transport,
        OmenChatWaitOptions {
            link_id: options.link_id,
            session_id: options.session_id,
            wait: options.wait,
        },
        |client| {
            client.pin_target_authoritative(
                options.session_id,
                options.room_id,
                options.target_event_id,
            )
        },
    )
    .await;
    let target_authoritative = client.pin_target_authoritative(
        options.session_id,
        options.room_id,
        options.target_event_id,
    );
    stages.push(serde_json::json!({
        "stage": "pin_authority_sync",
        "ok": target_authoritative,
        "request_events": authority_request.iter().map(format_chat_event).collect::<Vec<_>>(),
        "events": authority_events,
    }));
    if !target_authoritative {
        return Ok((false, stages));
    }

    // The authority sync can finish as soon as the pin snapshot is applied while
    // unrelated history resources are still completing. Start the deliberate
    // lost-ack observation at the current event tail so those bounded diagnostics
    // cannot evict the acknowledgement we intend to withhold.
    *runtime_events = runtime
        .subscribe_events()
        .ok_or_else(|| anyhow::anyhow!("configured runtime does not expose runtime events"))?;

    let pin = prepare_omenchat_smoke_pin(&store, &options, client_instance_id, PinAction::Pin)?;
    let sent =
        send_omenchat_smoke_pin(runtime, client, live_state, transport, &options, &pin).await?;
    stages.push(serde_json::json!({
        "stage": "pin_send",
        "ok": true,
        "events": sent,
    }));
    stages.push(discard_omenchat_pin_ack(runtime_events, options.link_id, options.wait).await?);
    live_state.cancel_session_transfers(options.session_id);

    let replayed =
        send_omenchat_smoke_pin(runtime, client, live_state, transport, &options, &pin).await?;
    let replay_events = wait_for_omenchat_condition(
        runtime,
        runtime_events,
        client,
        live_state,
        transport,
        OmenChatWaitOptions {
            link_id: options.link_id,
            session_id: options.session_id,
            wait: options.wait,
        },
        |client| {
            client.session(options.session_id).is_some_and(|session| {
                session.status == "pin mutation accepted by server; awaiting room event"
            })
        },
    )
    .await;
    let replay_acknowledged = client.session(options.session_id).is_some_and(|session| {
        session.status == "pin mutation accepted by server; awaiting room event"
    });
    stages.push(serde_json::json!({
        "stage": "pin_exact_replay",
        "ok": replay_acknowledged,
        "send_events": replayed,
        "events": replay_events,
    }));
    if replay_acknowledged {
        let _ = store.transition(
            pin.mutation_id,
            OutboundMutationState::SentUncertain,
            OutboundMutationState::Acknowledged,
        )?;
    }

    let sync_events = omenbrowser_rs::chat::live::handle_live_request(
        client,
        live_state,
        transport,
        ChatClientRequest::SyncRecent {
            session_id: options.session_id,
        },
    );
    send_omenchat_smoke_outgoing(runtime, options.link_id, transport).await?;
    let snapshot_events = wait_for_omenchat_condition(
        runtime,
        runtime_events,
        client,
        live_state,
        transport,
        OmenChatWaitOptions {
            link_id: options.link_id,
            session_id: options.session_id,
            wait: options.wait,
        },
        |client| {
            client
                .pin_for_target(options.session_id, options.room_id, options.target_event_id)
                .is_some()
                && client.pin_target_authoritative(
                    options.session_id,
                    options.room_id,
                    options.target_event_id,
                )
        },
    )
    .await;
    let authoritative_snapshot =
        omenchat_smoke_events_contain_decoded_event(&snapshot_events, "pin_snapshot_applied")
            && client
                .pin_for_target(options.session_id, options.room_id, options.target_event_id)
                .is_some();
    stages.push(serde_json::json!({
        "stage": "pin_snapshot",
        "ok": authoritative_snapshot,
        "request_events": sync_events.iter().map(format_chat_event).collect::<Vec<_>>(),
        "events": snapshot_events,
    }));

    let no_op = prepare_omenchat_smoke_pin(&store, &options, client_instance_id, PinAction::Pin)?;
    let _ =
        send_omenchat_smoke_pin(runtime, client, live_state, transport, &options, &no_op).await?;
    let no_op_events = wait_for_omenchat_condition(
        runtime,
        runtime_events,
        client,
        live_state,
        transport,
        OmenChatWaitOptions {
            link_id: options.link_id,
            session_id: options.session_id,
            wait: options.wait,
        },
        |client| {
            client
                .session(options.session_id)
                .is_some_and(|session| session.status == "pin already matched the requested state")
        },
    )
    .await;
    let no_op_ok = client
        .session(options.session_id)
        .is_some_and(|session| session.status == "pin already matched the requested state");
    if no_op_ok {
        let _ = store.transition(
            no_op.mutation_id,
            OutboundMutationState::SentUncertain,
            OutboundMutationState::Acknowledged,
        )?;
    }
    stages.push(serde_json::json!({
        "stage": "pin_noop",
        "ok": no_op_ok,
        "events": no_op_events,
    }));

    let unpin = prepare_omenchat_smoke_pin(&store, &options, client_instance_id, PinAction::Unpin)?;
    let _ =
        send_omenchat_smoke_pin(runtime, client, live_state, transport, &options, &unpin).await?;
    let unpin_events = wait_for_omenchat_condition(
        runtime,
        runtime_events,
        client,
        live_state,
        transport,
        OmenChatWaitOptions {
            link_id: options.link_id,
            session_id: options.session_id,
            wait: options.wait,
        },
        |client| {
            client.session(options.session_id).is_some_and(|session| {
                session.status == "pin mutation accepted by server; awaiting room event"
            })
        },
    )
    .await;
    let unpin_ok = client.session(options.session_id).is_some_and(|session| {
        session.status == "pin mutation accepted by server; awaiting room event"
    });
    if unpin_ok {
        let _ = store.transition(
            unpin.mutation_id,
            OutboundMutationState::SentUncertain,
            OutboundMutationState::Acknowledged,
        )?;
    }
    stages.push(serde_json::json!({
        "stage": "pin_unpin",
        "ok": unpin_ok,
        "events": unpin_events,
    }));

    let unpin_sync_events = omenbrowser_rs::chat::live::handle_live_request(
        client,
        live_state,
        transport,
        ChatClientRequest::SyncRecent {
            session_id: options.session_id,
        },
    );
    send_omenchat_smoke_outgoing(runtime, options.link_id, transport).await?;
    let unpin_snapshot_events = wait_for_omenchat_condition(
        runtime,
        runtime_events,
        client,
        live_state,
        transport,
        OmenChatWaitOptions {
            link_id: options.link_id,
            session_id: options.session_id,
            wait: options.wait,
        },
        |client| {
            client
                .pin_for_target(options.session_id, options.room_id, options.target_event_id)
                .is_none()
                && client.pin_target_authoritative(
                    options.session_id,
                    options.room_id,
                    options.target_event_id,
                )
        },
    )
    .await;
    let unpin_snapshot_ok = client
        .pin_for_target(options.session_id, options.room_id, options.target_event_id)
        .is_none()
        && client.pin_target_authoritative(
            options.session_id,
            options.room_id,
            options.target_event_id,
        );
    stages.push(serde_json::json!({
        "stage": "pin_unpin_snapshot",
        "ok": unpin_snapshot_ok,
        "request_events": unpin_sync_events.iter().map(format_chat_event).collect::<Vec<_>>(),
        "events": unpin_snapshot_events,
    }));

    let recovered = store.recover_nonterminal()?;
    let persistence_ok = recovered.is_empty();
    stages.push(serde_json::json!({
        "stage": "pin_intent_persistence",
        "ok": persistence_ok,
        "nonterminal_count": recovered.len(),
    }));
    Ok((
        replay_acknowledged
            && authoritative_snapshot
            && no_op_ok
            && unpin_ok
            && unpin_snapshot_ok
            && persistence_ok,
        stages,
    ))
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
struct OmenChatRevisionSmokeOptions<'a> {
    link_id: [u8; 16],
    session_id: omenbrowser_rs::chat::ChatSessionId,
    room_id: u32,
    target_event_id: u64,
    server_destination: &'a str,
    identity_storage_root: &'a std::path::Path,
    authenticated_identity_hash: [u8; 16],
    wait: Duration,
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn prepare_omenchat_smoke_revision(
    store: &omenbrowser_rs::chat::mutation_intents::MutationIntentStore,
    options: &OmenChatRevisionSmokeOptions<'_>,
    client_instance_id: omenbrowser_rs::chat::protocol::ClientInstanceId,
    action: omenbrowser_rs::chat::protocol::MessageRevisionAction,
    replacement: Option<String>,
) -> anyhow::Result<omenbrowser_rs::chat::mutation_intents::OutboundMutationIntent> {
    use omenbrowser_rs::chat::mutation_intents::{
        IntentTransition, OutboundMutationState, PrepareOutboundMutation,
    };
    use omenbrowser_rs::chat::protocol::{ChatOp, MessageRevisionRequest};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default();
    let body = MessageRevisionRequest {
        target_event_id: options.target_event_id,
        action,
        replacement,
    }
    .into_frame_body()
    .context("encode OMENchat smoke message revision")?;
    let prepared = store.persist_prepared(PrepareOutboundMutation {
        server_destination: options.server_destination,
        authenticated_identity_hash: &options.authenticated_identity_hash,
        client_instance_id,
        op: ChatOp::RoomMessageRevision,
        room_id: Some(options.room_id),
        body,
        created_at: now,
        expires_at: now.saturating_add(60 * 60),
        correlation_id: Some("release-message-revision-smoke"),
    })?;
    match store.transition(
        prepared.mutation_id,
        OutboundMutationState::Prepared,
        OutboundMutationState::SentUncertain,
    )? {
        IntentTransition::Updated(intent) => Ok(intent),
        other => anyhow::bail!("OMENchat smoke message revision transition failed: {other:?}"),
    }
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
async fn send_omenchat_smoke_revision(
    runtime: &dyn omenbrowser_rs::runtime::NetworkRuntime,
    client: &mut omenbrowser_rs::chat::ChatClient,
    live_state: &mut omenbrowser_rs::chat::live::LiveChatClientState,
    transport: &mut OmenChatSmokeTransport,
    options: &OmenChatRevisionSmokeOptions<'_>,
    intent: &omenbrowser_rs::chat::mutation_intents::OutboundMutationIntent,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let events = omenbrowser_rs::chat::live::send_uncertain_durable_message_revision(
        client,
        live_state,
        transport,
        options.session_id,
        intent,
    );
    if events
        .iter()
        .any(|event| matches!(event, omenbrowser_rs::chat::ChatClientEvent::Error { .. }))
    {
        anyhow::bail!("OMENchat smoke message revision was rejected before transmission");
    }
    send_omenchat_smoke_outgoing(runtime, options.link_id, transport).await?;
    Ok(events.iter().map(format_chat_event).collect())
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
async fn discard_omenchat_revision_ack(
    runtime_events: &mut tokio::sync::broadcast::Receiver<RuntimeBusEvent>,
    link_id: [u8; 16],
    wait: Duration,
) -> anyhow::Result<serde_json::Value> {
    let deadline = tokio::time::Instant::now() + wait;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, runtime_events.recv()).await {
            Ok(Ok(RuntimeBusEvent::OmenChatLinkData(data))) if data.link_id == link_id => {
                let frame = omenbrowser_rs::chat::codec::decode_frame(&data.frame_bytes)
                    .context("decode deliberately discarded OMENchat revision response")?;
                if frame.op == omenbrowser_rs::chat::protocol::ChatOp::MessageRevisionAck {
                    return Ok(serde_json::json!({
                        "stage": "revision_lost_ack",
                        "ok": true,
                        "bytes": data.frame_bytes.len(),
                        "sequence": frame.seq,
                    }));
                }
            }
            Ok(Ok(_)) | Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => {}
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) | Err(_) => break,
        }
    }
    anyhow::bail!("OMENchat smoke did not observe the revision acknowledgement selected for loss")
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
async fn run_omenchat_revision_smoke(
    runtime: &dyn omenbrowser_rs::runtime::NetworkRuntime,
    runtime_events: &mut tokio::sync::broadcast::Receiver<RuntimeBusEvent>,
    client: &mut omenbrowser_rs::chat::ChatClient,
    live_state: &mut omenbrowser_rs::chat::live::LiveChatClientState,
    transport: &mut OmenChatSmokeTransport,
    options: OmenChatRevisionSmokeOptions<'_>,
) -> anyhow::Result<(bool, Vec<serde_json::Value>)> {
    use omenbrowser_rs::chat::mutation_intents::{MutationIntentStore, OutboundMutationState};
    use omenbrowser_rs::chat::protocol::MessageRevisionAction;
    use omenbrowser_rs::chat::ChatClientRequest;

    let mut stages = Vec::new();
    let negotiated = live_state.durable_mutations_negotiated(options.session_id)
        && live_state.message_revisions_negotiated(options.session_id);
    stages.push(serde_json::json!({
        "stage": "revision_capability",
        "ok": negotiated,
    }));
    let Some(client_instance_id) = live_state.client_instance_id() else {
        return Ok((false, stages));
    };
    if !negotiated {
        return Ok((false, stages));
    }
    let store = MutationIntentStore::open_for_identity_storage_root(options.identity_storage_root)
        .context("open isolated OMENchat revision smoke mutation store")?;

    let corrected_body = "OMENchat smoke corrected".to_owned();
    let correction = prepare_omenchat_smoke_revision(
        &store,
        &options,
        client_instance_id,
        MessageRevisionAction::Correct,
        Some(corrected_body.clone()),
    )?;
    let sent = send_omenchat_smoke_revision(
        runtime,
        client,
        live_state,
        transport,
        &options,
        &correction,
    )
    .await?;
    stages.push(serde_json::json!({
        "stage": "revision_correction_send",
        "ok": true,
        "events": sent,
    }));
    stages
        .push(discard_omenchat_revision_ack(runtime_events, options.link_id, options.wait).await?);

    let replayed = send_omenchat_smoke_revision(
        runtime,
        client,
        live_state,
        transport,
        &options,
        &correction,
    )
    .await?;
    let replay_events = wait_for_omenchat_condition(
        runtime,
        runtime_events,
        client,
        live_state,
        transport,
        OmenChatWaitOptions {
            link_id: options.link_id,
            session_id: options.session_id,
            wait: options.wait,
        },
        |client| {
            client.session(options.session_id).is_some_and(|session| {
                session.status == "message revision accepted by server; awaiting room event"
            })
        },
    )
    .await;
    let correction_acknowledged = client.session(options.session_id).is_some_and(|session| {
        session.status == "message revision accepted by server; awaiting room event"
    });
    stages.push(serde_json::json!({
        "stage": "revision_exact_replay",
        "ok": correction_acknowledged,
        "send_events": replayed,
        "events": replay_events,
    }));
    if correction_acknowledged {
        let _ = store.transition(
            correction.mutation_id,
            OutboundMutationState::SentUncertain,
            OutboundMutationState::Acknowledged,
        )?;
    }

    let sync_events = omenbrowser_rs::chat::live::handle_live_request(
        client,
        live_state,
        transport,
        ChatClientRequest::SyncRecent {
            session_id: options.session_id,
        },
    );
    send_omenchat_smoke_outgoing(runtime, options.link_id, transport).await?;
    let snapshot_events = wait_for_omenchat_condition(
        runtime,
        runtime_events,
        client,
        live_state,
        transport,
        OmenChatWaitOptions {
            link_id: options.link_id,
            session_id: options.session_id,
            wait: options.wait,
        },
        |client| {
            client
                .message_revision_for_target(
                    options.session_id,
                    options.room_id,
                    options.target_event_id,
                )
                .is_some_and(|revision| {
                    revision.action == MessageRevisionAction::Correct
                        && revision.replacement_body.as_deref() == Some(corrected_body.as_str())
                })
                && client.message_revision_snapshot_complete(
                    options.session_id,
                    options.room_id,
                    options.target_event_id,
                )
        },
    )
    .await;
    let correction_resource_snapshot = snapshot_events.iter().any(|event| {
        event.get("event").and_then(serde_json::Value::as_str) == Some("resource_data")
            && omenchat_smoke_events_contain_decoded_event(
                std::slice::from_ref(event),
                "message_revision_snapshot_applied",
            )
    }) && client
        .message_revision_for_target(options.session_id, options.room_id, options.target_event_id)
        .is_some_and(|revision| {
            revision.action == MessageRevisionAction::Correct
                && revision.replacement_body.as_deref() == Some(corrected_body.as_str())
        });
    stages.push(serde_json::json!({
        "stage": "revision_correction_resource_snapshot",
        "ok": correction_resource_snapshot,
        "request_events": sync_events.iter().map(format_chat_event).collect::<Vec<_>>(),
        "events": snapshot_events,
    }));

    let tombstone = prepare_omenchat_smoke_revision(
        &store,
        &options,
        client_instance_id,
        MessageRevisionAction::Tombstone,
        None,
    )?;
    if let Some(session) = client.session_mut(options.session_id) {
        session.status = "awaiting message tombstone acknowledgement".into();
    }
    let _ =
        send_omenchat_smoke_revision(runtime, client, live_state, transport, &options, &tombstone)
            .await?;
    let tombstone_events = wait_for_omenchat_condition(
        runtime,
        runtime_events,
        client,
        live_state,
        transport,
        OmenChatWaitOptions {
            link_id: options.link_id,
            session_id: options.session_id,
            wait: options.wait,
        },
        |client| {
            client.session(options.session_id).is_some_and(|session| {
                session.status == "message revision accepted by server; awaiting room event"
            })
        },
    )
    .await;
    let tombstone_acknowledged = client.session(options.session_id).is_some_and(|session| {
        session.status == "message revision accepted by server; awaiting room event"
    });
    if tombstone_acknowledged {
        let _ = store.transition(
            tombstone.mutation_id,
            OutboundMutationState::SentUncertain,
            OutboundMutationState::Acknowledged,
        )?;
    }
    stages.push(serde_json::json!({
        "stage": "revision_tombstone",
        "ok": tombstone_acknowledged,
        "events": tombstone_events,
    }));

    let tombstone_sync_events = omenbrowser_rs::chat::live::handle_live_request(
        client,
        live_state,
        transport,
        ChatClientRequest::SyncRecent {
            session_id: options.session_id,
        },
    );
    send_omenchat_smoke_outgoing(runtime, options.link_id, transport).await?;
    let tombstone_snapshot_events = wait_for_omenchat_condition(
        runtime,
        runtime_events,
        client,
        live_state,
        transport,
        OmenChatWaitOptions {
            link_id: options.link_id,
            session_id: options.session_id,
            wait: options.wait,
        },
        |client| {
            client
                .message_revision_for_target(
                    options.session_id,
                    options.room_id,
                    options.target_event_id,
                )
                .is_some_and(|revision| revision.action == MessageRevisionAction::Tombstone)
                && client.message_revision_snapshot_complete(
                    options.session_id,
                    options.room_id,
                    options.target_event_id,
                )
        },
    )
    .await;
    let tombstone_resource_snapshot = tombstone_snapshot_events.iter().any(|event| {
        event.get("event").and_then(serde_json::Value::as_str) == Some("resource_data")
            && omenchat_smoke_events_contain_decoded_event(
                std::slice::from_ref(event),
                "message_revision_snapshot_applied",
            )
    }) && client
        .message_revision_for_target(options.session_id, options.room_id, options.target_event_id)
        .is_some_and(|revision| revision.action == MessageRevisionAction::Tombstone);
    stages.push(serde_json::json!({
        "stage": "revision_tombstone_resource_snapshot",
        "ok": tombstone_resource_snapshot,
        "request_events": tombstone_sync_events.iter().map(format_chat_event).collect::<Vec<_>>(),
        "events": tombstone_snapshot_events,
    }));

    let recovered = store.recover_nonterminal()?;
    let persistence_ok = recovered.is_empty();
    stages.push(serde_json::json!({
        "stage": "revision_intent_persistence",
        "ok": persistence_ok,
        "nonterminal_count": recovered.len(),
    }));
    Ok((
        correction_acknowledged
            && correction_resource_snapshot
            && tombstone_acknowledged
            && tombstone_resource_snapshot
            && persistence_ok,
        stages,
    ))
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn create_omenchat_reconnect_ready_file(path: &std::path::Path) -> anyhow::Result<()> {
    use std::io::Write;

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| anyhow::anyhow!("OMENchat reconnect marker needs an existing parent"))?;
    if !parent.is_dir() {
        anyhow::bail!("OMENchat reconnect marker parent is not a directory");
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .context("failed to create OMENchat reconnect ready marker")?;
    file.write_all(b"ready\n")
        .context("failed to write OMENchat reconnect ready marker")?;
    file.sync_all()
        .context("failed to sync OMENchat reconnect ready marker")
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
struct OmenchatSmokeUploadFetch<'a> {
    runtime: &'a dyn omenbrowser_rs::runtime::NetworkRuntime,
    runtime_events: &'a mut tokio::sync::broadcast::Receiver<RuntimeBusEvent>,
    link_id: [u8; 16],
    client: &'a mut omenbrowser_rs::chat::ChatClient,
    live_state: &'a mut omenbrowser_rs::chat::live::LiveChatClientState,
    transport: &'a mut OmenChatSmokeTransport,
    session_id: omenbrowser_rs::chat::ChatSessionId,
    room: &'a str,
    resource_id: String,
    filename: &'a str,
    bytes: Option<u64>,
    wait: Duration,
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
async fn run_omenchat_smoke_upload_fetch(
    input: OmenchatSmokeUploadFetch<'_>,
) -> anyhow::Result<(bool, Vec<serde_json::Value>)> {
    let request_upload_events = omenbrowser_rs::chat::live::handle_live_request(
        input.client,
        input.live_state,
        input.transport,
        omenbrowser_rs::chat::ChatClientRequest::RequestUpload {
            session_id: input.session_id,
            room: input.room.to_owned(),
            resource_id: input.resource_id.clone(),
        },
    );
    let mut stages = vec![serde_json::json!({
        "stage": "upload_fetch_frame",
        "ok": !request_upload_events.iter().any(|event| matches!(event, omenbrowser_rs::chat::ChatClientEvent::Error { .. })),
        "resource_id": input.resource_id,
        "filename": input.filename,
        "bytes": input.bytes,
        "events": request_upload_events.iter().map(format_chat_event).collect::<Vec<_>>(),
    })];
    send_omenchat_smoke_outgoing(input.runtime, input.link_id, input.transport).await?;

    let upload_fetch_events = wait_for_omenchat_condition(
        input.runtime,
        input.runtime_events,
        input.client,
        input.live_state,
        input.transport,
        OmenChatWaitOptions {
            link_id: input.link_id,
            session_id: input.session_id,
            wait: input.wait,
        },
        |client| {
            omenchat_session_upload_resource_received(client, input.session_id, input.filename)
        },
    )
    .await;
    let upload_resource_available =
        omenchat_session_upload_resource_received(input.client, input.session_id, input.filename)
            && omenchat_smoke_events_contain_decoded_event(
                &upload_fetch_events,
                "upload_resource_available",
            );
    stages.push(serde_json::json!({
        "stage": "upload_fetch_wait",
        "ok": upload_resource_available,
        "filename": input.filename,
        "bytes": input.bytes,
        "events": upload_fetch_events,
    }));
    Ok((upload_resource_available, stages))
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
async fn collect_runtime_trace(
    runtime_events: &mut tokio::sync::broadcast::Receiver<RuntimeBusEvent>,
    wait: Duration,
    destination_filter: Option<&str>,
) -> Vec<serde_json::Value> {
    let deadline = tokio::time::Instant::now() + wait;
    let mut events = Vec::new();
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let received = tokio::time::timeout(remaining, runtime_events.recv()).await;
        let event = match received {
            Ok(Ok(event)) => event,
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(count))) => {
                events.push(serde_json::json!({"event": "lagged", "count": count}));
                continue;
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => break,
            Err(_) => break,
        };
        match event {
            RuntimeBusEvent::PathUpdated(path)
                if destination_filter.is_none_or(|destination| {
                    path.destination_hash.eq_ignore_ascii_case(destination)
                }) =>
            {
                let known = path.known;
                events.push(serde_json::json!({"event": "path", "value": path}));
                if known {
                    break;
                }
            }
            RuntimeBusEvent::Announce(announce)
                if destination_filter.is_none_or(|destination| {
                    announce.destination_hash.eq_ignore_ascii_case(destination)
                }) =>
            {
                events.push(serde_json::json!({"event": "announce", "value": announce}));
            }
            RuntimeBusEvent::Debug(message) => {
                events.push(serde_json::json!({"event": "debug", "message": message}));
            }
            RuntimeBusEvent::Error(message) => {
                events.push(serde_json::json!({"event": "error", "message": message}));
            }
            _ => {}
        }
    }
    events
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn omenchat_session_contains_message(
    client: &omenbrowser_rs::chat::ChatClient,
    session_id: omenbrowser_rs::chat::ChatSessionId,
    message: &str,
) -> bool {
    client.session(session_id).is_some_and(|session| {
        session.events.iter().any(|event| {
            if event.event_id > u64::MAX.saturating_sub(1_000_000) {
                return false;
            }
            matches!(
                &event.kind,
                omenbrowser_rs::chat::ChatEventKind::Message { body }
                    | omenbrowser_rs::chat::ChatEventKind::Action { body }
                    | omenbrowser_rs::chat::ChatEventKind::Notice { body }
                    | omenbrowser_rs::chat::ChatEventKind::System { body }
                    if body == message
            )
        })
    })
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn omenchat_session_message_event_id(
    client: &omenbrowser_rs::chat::ChatClient,
    session_id: omenbrowser_rs::chat::ChatSessionId,
    message: &str,
) -> Option<u64> {
    client.session(session_id).and_then(|session| {
        session.events.iter().rev().find_map(|event| {
            if event.event_id > u64::MAX.saturating_sub(1_000_000) {
                return None;
            }
            matches!(
                &event.kind,
                omenbrowser_rs::chat::ChatEventKind::Message { body }
                    | omenbrowser_rs::chat::ChatEventKind::Action { body }
                    | omenbrowser_rs::chat::ChatEventKind::Notice { body }
                    | omenbrowser_rs::chat::ChatEventKind::System { body }
                    if body == message
            )
            .then_some(event.event_id)
        })
    })
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn omenchat_session_upload_resource_id(
    client: &omenbrowser_rs::chat::ChatClient,
    session_id: omenbrowser_rs::chat::ChatSessionId,
    filename: &str,
    bytes: Option<u64>,
) -> Option<String> {
    client.session(session_id).and_then(|session| {
        session
            .events
            .iter()
            .rev()
            .find_map(|event| match &event.kind {
                omenbrowser_rs::chat::ChatEventKind::Upload {
                    resource_id,
                    filename: event_filename,
                    bytes: event_bytes,
                } if event_filename == filename
                    && bytes.is_none_or(|bytes| *event_bytes == bytes) =>
                {
                    Some(resource_id.clone())
                }
                _ => None,
            })
    })
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn omenchat_session_upload_resource_received(
    client: &omenbrowser_rs::chat::ChatClient,
    session_id: omenbrowser_rs::chat::ChatSessionId,
    filename: &str,
) -> bool {
    client.session(session_id).is_some_and(|session| {
        session.status.starts_with("upload resource received:") && session.status.contains(filename)
    })
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn omenchat_smoke_events_contain_decoded_event(
    events: &[serde_json::Value],
    event_name: &str,
) -> bool {
    events.iter().any(|entry| {
        entry
            .get("decoded")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|decoded| {
                decoded.iter().any(|event| {
                    event.get("event").and_then(serde_json::Value::as_str) == Some(event_name)
                })
            })
    })
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn omenchat_smoke_events_contain_announcement_policy_rejection(
    events: &[serde_json::Value],
) -> bool {
    events.iter().any(|entry| {
        entry
            .get("decoded")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|decoded| {
                decoded.iter().any(|event| {
                    event.get("event").and_then(serde_json::Value::as_str) == Some("error")
                        && event
                            .get("message")
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(is_announcement_policy_rejection_message)
                })
            })
    })
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn is_announcement_policy_rejection_event(event: &omenbrowser_rs::chat::ChatClientEvent) -> bool {
    matches!(
        event,
        omenbrowser_rs::chat::ChatClientEvent::Error { message, .. }
            if is_announcement_policy_rejection_message(message)
    )
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn is_announcement_policy_rejection_message(message: &str) -> bool {
    message.starts_with("room is read-only for members:")
        && message.contains("restricted to moderators and administrators")
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn format_chat_event(event: &omenbrowser_rs::chat::ChatClientEvent) -> serde_json::Value {
    match event {
        omenbrowser_rs::chat::ChatClientEvent::ServerOpened { session_id, server } => {
            serde_json::json!({
                "event": "server_opened",
                "session_id": session_id,
                "server": {
                    "server_id": server.server_id.clone(),
                    "destination": server.destination.clone(),
                    "display_name": server.display_name.clone(),
                }
            })
        }
        omenbrowser_rs::chat::ChatClientEvent::RoomJoined {
            session_id,
            room,
            users,
            latest_events,
        } => serde_json::json!({
            "event": "room_joined",
            "session_id": session_id,
            "room": {
                "server_id": room.server_id.clone(),
                "room_id": room.room_id,
                "name": room.name.clone(),
                "unread": room.unread,
                "joined": room.joined,
            },
            "users": users.len(),
            "latest_events": latest_events.len(),
        }),
        omenbrowser_rs::chat::ChatClientEvent::EventAppended { session_id, event } => {
            serde_json::json!({
                "event": "event_appended",
                "session_id": session_id,
                "chat_event": format_chat_timeline_event(event),
            })
        }
        omenbrowser_rs::chat::ChatClientEvent::DurableMutationAcknowledged {
            session_id,
            mutation_id,
        } => serde_json::json!({
            "event": "durable_mutation_acknowledged",
            "session_id": session_id,
            "mutation_id": mutation_id
                .as_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        }),
        omenbrowser_rs::chat::ChatClientEvent::DurableMutationTerminal {
            session_id,
            mutation_id,
            state,
        } => serde_json::json!({
            "event": "durable_mutation_terminal",
            "session_id": session_id,
            "mutation_id": mutation_id
                .as_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            "state": match state {
                omenbrowser_rs::chat::DurableMutationTerminalState::Conflict => "conflict",
                omenbrowser_rs::chat::DurableMutationTerminalState::Expired => "expired",
            },
        }),
        omenbrowser_rs::chat::ChatClientEvent::HistoryPrepended { session_id, events } => {
            serde_json::json!({"event": "history_prepended", "session_id": session_id, "events": events.len()})
        }
        omenbrowser_rs::chat::ChatClientEvent::HistorySynced {
            session_id,
            room_id,
        } => {
            serde_json::json!({"event": "history_synced", "session_id": session_id, "room_id": room_id})
        }
        omenbrowser_rs::chat::ChatClientEvent::HistorySyncNeeded {
            session_id,
            room_id,
        } => {
            serde_json::json!({"event": "history_sync_needed", "session_id": session_id, "room_id": room_id})
        }
        omenbrowser_rs::chat::ChatClientEvent::ReactionDeltaApplied {
            session_id,
            room_id,
            event,
        } => serde_json::json!({
            "event": "reaction_delta_applied",
            "session_id": session_id,
            "room_id": room_id,
            "target_event_id": event.target_event_id,
            "actor_user_id": event.actor_user_id,
            "reaction": event.token.as_str(),
            "action": event.action as u8,
        }),
        omenbrowser_rs::chat::ChatClientEvent::ReactionSnapshotApplied {
            session_id,
            room_id,
            snapshot,
        } => serde_json::json!({
            "event": "reaction_snapshot_applied",
            "session_id": session_id,
            "room_id": room_id,
            "targets": snapshot.target_event_ids.len(),
            "entries": snapshot.entries.len(),
        }),
        omenbrowser_rs::chat::ChatClientEvent::MessageRevisionDeltaApplied {
            session_id,
            room_id,
            event,
        } => serde_json::json!({
            "event": "message_revision_delta_applied",
            "session_id": session_id,
            "room_id": room_id,
            "target_event_id": event.target_event_id,
            "revision_event_id": event.revision_event_id,
            "action": event.action as u8,
            "revision_number": event.revision_number,
        }),
        omenbrowser_rs::chat::ChatClientEvent::MessageRevisionSnapshotApplied {
            session_id,
            room_id,
            snapshot,
        } => serde_json::json!({
            "event": "message_revision_snapshot_applied",
            "session_id": session_id,
            "room_id": room_id,
            "targets": snapshot.target_event_ids.len(),
            "entries": snapshot.entries.len(),
        }),
        omenbrowser_rs::chat::ChatClientEvent::PinDeltaApplied {
            session_id,
            room_id,
            event,
        } => serde_json::json!({
            "event": "pin_delta_applied",
            "session_id": session_id,
            "room_id": room_id,
            "target_event_id": event.target_event_id,
            "pin_event_id": event.pin_event_id,
            "action": event.action as u8,
        }),
        omenbrowser_rs::chat::ChatClientEvent::PinSnapshotApplied {
            session_id,
            room_id,
            snapshot,
        } => serde_json::json!({
            "event": "pin_snapshot_applied",
            "session_id": session_id,
            "room_id": room_id,
            "targets": snapshot.target_event_ids.len(),
            "entries": snapshot.entries.len(),
        }),
        omenbrowser_rs::chat::ChatClientEvent::RoomsUpdated { session_id, rooms } => {
            serde_json::json!({"event": "rooms_updated", "session_id": session_id, "rooms": rooms.len()})
        }
        omenbrowser_rs::chat::ChatClientEvent::ServerMotd { session_id, motd } => {
            serde_json::json!({"event": "server_motd", "session_id": session_id, "motd": motd})
        }
        omenbrowser_rs::chat::ChatClientEvent::ServerPolicy {
            session_id,
            upload_quota_bytes,
            upload_max_file_bytes,
            ping_interval_seconds,
        } => {
            serde_json::json!({
                "event": "server_policy",
                "session_id": session_id,
                "upload_quota_bytes": upload_quota_bytes,
                "upload_max_file_bytes": upload_max_file_bytes,
                "ping_interval_seconds": ping_interval_seconds,
            })
        }
        omenbrowser_rs::chat::ChatClientEvent::UserUpdated { session_id, user } => {
            serde_json::json!({
                "event": "user_updated",
                "session_id": session_id,
                "user": {
                    "server_id": user.server_id.clone(),
                    "user_id": user.user_id,
                    "display_name": user.display_name.clone(),
                    "role_bits": user.role_bits,
                    "status_bits": user.status_bits,
                    "lxmf_available": user.lxmf_available,
                }
            })
        }
        omenbrowser_rs::chat::ChatClientEvent::LocalUserBound {
            session_id,
            user_id,
        } => serde_json::json!({
            "event": "local_user_bound",
            "session_id": session_id,
            "user_id": user_id,
        }),
        omenbrowser_rs::chat::ChatClientEvent::UploadAccepted {
            session_id,
            resource_id,
            filename,
            bytes,
        } => {
            serde_json::json!({
                "event": "upload_accepted",
                "session_id": session_id,
                "resource_id": resource_id,
                "filename": filename,
                "bytes": bytes,
            })
        }
        omenbrowser_rs::chat::ChatClientEvent::UploadRejected { session_id, reason } => {
            serde_json::json!({
                "event": "upload_rejected",
                "session_id": session_id,
                "reason": reason,
            })
        }
        omenbrowser_rs::chat::ChatClientEvent::UploadCompleted {
            session_id,
            resource_id,
            filename,
            bytes,
        } => {
            serde_json::json!({
                "event": "upload_completed",
                "session_id": session_id,
                "resource_id": resource_id,
                "filename": filename,
                "bytes": bytes,
            })
        }
        omenbrowser_rs::chat::ChatClientEvent::UploadResourceAvailable {
            session_id,
            resource_id,
            filename,
            content_type,
            bytes,
        } => {
            serde_json::json!({
                "event": "upload_resource_available",
                "session_id": session_id,
                "resource_id": resource_id,
                "filename": filename,
                "content_type": content_type,
                "bytes": bytes.len(),
            })
        }
        omenbrowser_rs::chat::ChatClientEvent::UploadResourceProgress {
            session_id,
            resource_id,
            filename,
            received,
            total,
        } => {
            serde_json::json!({
                "event": "upload_resource_progress",
                "session_id": session_id,
                "resource_id": resource_id,
                "filename": filename,
                "received": received,
                "total": total,
            })
        }
        omenbrowser_rs::chat::ChatClientEvent::ModerationAuditPageApplied {
            session_id,
            room_id,
            page,
        } => serde_json::json!({
            "event": "moderation_audit_page",
            "session_id": session_id,
            "room_id": room_id,
            "records": page.records.len(),
            "oldest_audit_id": page.records.last().map(|record| record.audit_id),
        }),
        omenbrowser_rs::chat::ChatClientEvent::ModerationAuditEnd {
            session_id,
            room_id,
        } => serde_json::json!({
            "event": "moderation_audit_end",
            "session_id": session_id,
            "room_id": room_id,
        }),
        omenbrowser_rs::chat::ChatClientEvent::Error {
            session_id,
            message,
        } => serde_json::json!({"event": "error", "session_id": session_id, "message": message}),
    }
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn format_chat_timeline_event(event: &omenbrowser_rs::chat::ChatEvent) -> serde_json::Value {
    let (kind, body) = match &event.kind {
        omenbrowser_rs::chat::ChatEventKind::Message { body }
        | omenbrowser_rs::chat::ChatEventKind::RichMessage { body, .. } => {
            ("message", body.as_str())
        }
        omenbrowser_rs::chat::ChatEventKind::Action { body } => ("action", body.as_str()),
        omenbrowser_rs::chat::ChatEventKind::Notice { body } => ("notice", body.as_str()),
        omenbrowser_rs::chat::ChatEventKind::System { body } => ("system", body.as_str()),
        omenbrowser_rs::chat::ChatEventKind::Upload { filename, .. } => {
            ("upload", filename.as_str())
        }
    };
    let mut value = serde_json::json!({
        "server_id": event.server_id.clone(),
        "room_id": event.room_id,
        "event_id": event.event_id,
        "actor_user_id": event.actor_user_id,
        "at_unix": event.at_unix,
        "kind": kind,
        "body": body,
    });
    if let omenbrowser_rs::chat::ChatEventKind::Upload {
        resource_id,
        filename,
        bytes,
    } = &event.kind
    {
        value["resource_id"] = serde_json::json!(resource_id);
        value["filename"] = serde_json::json!(filename);
        value["bytes"] = serde_json::json!(bytes);
    }
    if let omenbrowser_rs::chat::ChatEventKind::RichMessage { metadata, .. } = &event.kind {
        value["reply_to_event_id"] = serde_json::json!(metadata.reply_to_event_id);
        value["mentioned_user_ids"] = serde_json::json!(metadata.mentioned_user_ids);
    }
    value
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
#[derive(Clone, Copy)]
struct OmenChatSmokeReportContext<'a> {
    destination: &'a str,
    room: &'a str,
    message: &'a str,
    announcement_rejection_smoke: bool,
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn omenchat_smoke_report(
    ok: bool,
    stage: &str,
    context: OmenChatSmokeReportContext<'_>,
    stages: Vec<serde_json::Value>,
    session: Option<serde_json::Value>,
) -> serde_json::Value {
    let OmenChatSmokeReportContext {
        destination,
        room,
        message,
        announcement_rejection_smoke,
    } = context;
    serde_json::json!({
        "report": "omenchat_smoke",
        "classification": {
            "outcome": if ok { "pass" } else { "fail" },
            "stage": stage,
            "reason": if ok && announcement_rejection_smoke {
                "OMENchat Link opened, room joined, and the server rejected member publication without committing the message"
            } else if ok {
                "OMENchat Link opened, room joined, and message echo was observed"
            } else {
                "OMENchat smoke did not complete all required stages"
            },
            "next_step": if ok {
                "test from the desktop OMENchat pane"
            } else {
                "inspect stages; common blockers are missing destination key/path, no server announce, or Link response timeout"
            },
        },
        "destination": destination,
        "room": room,
        "message": message,
        "announcement_rejection_smoke": announcement_rejection_smoke,
        "stages": stages,
        "session": session,
    })
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn write_omenchat_smoke_report(
    report: serde_json::Value,
    output: Option<PathBuf>,
    stdout: bool,
    default_output: bool,
    diagnostics_dir: &std::path::Path,
) -> anyhow::Result<()> {
    let content =
        serde_json::to_string_pretty(&report).context("failed to render OMENchat smoke JSON")?;
    let outcome = report
        .pointer("/classification/outcome")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let stage = report
        .pointer("/classification/stage")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    if stdout {
        eprintln!("OMENchat smoke: {outcome} at {stage}");
        println!("{content}");
    }
    if let Some(path) = output
        .or_else(|| default_output.then(|| default_omenchat_smoke_report_path(diagnostics_dir)))
    {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create output directory {}", parent.display())
            })?;
        }
        std::fs::write(&path, content.as_bytes())
            .with_context(|| format!("failed to write OMENchat smoke report {}", path.display()))?;
        if stdout {
            eprintln!("{}", path.display());
        } else {
            println!("{}", path.display());
        }
    }
    Ok(())
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn default_omenchat_smoke_report_path(diagnostics_dir: &std::path::Path) -> PathBuf {
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    diagnostics_dir.join(format!("omenchat-smoke-{epoch}.json"))
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(all(
    test,
    any(feature = "chat-client-rns", feature = "chat-client-rns-clean")
))]
mod tests {
    use super::{
        create_omenchat_reconnect_ready_file,
        omenchat_smoke_events_contain_announcement_policy_rejection,
    };

    #[test]
    fn reconnect_ready_marker_is_create_new_and_isolated() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-reconnect-marker-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir(&root).expect("create isolated marker root");
        let marker = root.join("ready");
        create_omenchat_reconnect_ready_file(&marker).expect("create ready marker");
        assert_eq!(std::fs::read(&marker).expect("read marker"), b"ready\n");
        assert!(create_omenchat_reconnect_ready_file(&marker).is_err());
        std::fs::remove_dir_all(root).expect("remove isolated marker root");
    }

    #[test]
    fn announcement_rejection_evidence_requires_the_typed_policy_error() {
        let rejected = vec![serde_json::json!({
            "event": "link_data",
            "decoded": [{
                "event": "error",
                "message": "room is read-only for members: publishing messages is restricted to moderators and administrators in this announcement room"
            }]
        })];
        assert!(omenchat_smoke_events_contain_announcement_policy_rejection(
            &rejected
        ));

        let unrelated = vec![serde_json::json!({
            "event": "link_data",
            "decoded": [{
                "event": "error",
                "message": "permission denied: user is muted"
            }]
        })];
        assert!(!omenchat_smoke_events_contain_announcement_policy_rejection(&unrelated));
    }
}
