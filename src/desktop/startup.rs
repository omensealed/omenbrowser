use std::collections::{HashMap, HashSet};

use iced::widget::{pane_grid, scrollable, text_editor};

use crate::app::App;
#[cfg(feature = "chat-client")]
use crate::chat::store::SqliteChatStore;
#[cfg(feature = "chat-client")]
use crate::chat::{ChatClient, ChatSessionId};

use super::clearweb::detect_clearweb_socks_proxy;
use super::clearweb_state::ClearwebDesktopState;
use super::conversation_state::ConversationDesktopState;
use super::external_browser::detect_external_browsers;
#[cfg(feature = "chat-client")]
use super::omenchat_runtime::prune_unrestorable_omenchat_servers;
use super::{desktop_pane_order, restored_desktop_pane_state, DesktopPane};

pub(in crate::desktop) struct DesktopWorkspaceStartup {
    pub(in crate::desktop) workspace_panes: pane_grid::State<DesktopPane>,
    pub(in crate::desktop) active_workspace_pane: pane_grid::Pane,
}

#[cfg(feature = "chat-client")]
pub(in crate::desktop) struct OmenChatStartupState {
    pub(in crate::desktop) chat_client: ChatClient,
    pub(in crate::desktop) chat_store: Option<SqliteChatStore>,
    pub(in crate::desktop) session_ids: Vec<ChatSessionId>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) client_instance_id: Option<crate::chat::protocol::ClientInstanceId>,
}

#[cfg(feature = "chat-client")]
pub(in crate::desktop) fn restore_omenchat_startup_state(app: &App) -> OmenChatStartupState {
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    let client_instance_id = {
        let store = crate::chat::client_instance::ClientInstanceIdStore::for_identity_storage_root(
            app.paths.identity_storage_root(),
        );
        match store.load_or_create() {
            Ok(client_instance_id) => Some(client_instance_id),
            Err(error) => {
                tracing::warn!(
                    "OMENchat durable mutation capability remains disabled because the client instance could not be loaded from {}: {error}",
                    store.path().display()
                );
                None
            }
        }
    };
    let chat_store_path = app
        .paths
        .identity_storage_root()
        .join("plugins")
        .join(crate::plugins::BUILTIN_OMENCHAT_PLUGIN_ID)
        .join("chat.sqlite");
    let (chat_client, chat_store) = match SqliteChatStore::open(&chat_store_path) {
        Ok(mut store) => {
            prune_unrestorable_omenchat_servers(&mut store);
            let mut client = ChatClient::new();
            if let Err(error) = client.restore_from_store(&store, 100) {
                tracing::warn!(
                    "failed to restore OMENchat sessions from {}: {error}",
                    chat_store_path.display()
                );
            }
            (client, Some(store))
        }
        Err(error) => {
            tracing::warn!(
                "failed to open OMENchat store at {}: {error}",
                chat_store_path.display()
            );
            (ChatClient::new(), None)
        }
    };
    let session_ids = chat_client
        .sessions()
        .iter()
        .map(|session| session.session_id)
        .collect::<Vec<_>>();
    OmenChatStartupState {
        chat_client,
        chat_store,
        session_ids,
        #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
        client_instance_id,
    }
}

pub(in crate::desktop) fn desktop_workspace_startup_state(
    app: &App,
    omenchat_session_ids: &[u64],
) -> DesktopWorkspaceStartup {
    let mut workspace_panes = restored_desktop_pane_state(app, omenchat_session_ids);
    let mut pane_order = desktop_pane_order(workspace_panes.layout());
    let first_pane = pane_order
        .first()
        .copied()
        .or_else(|| workspace_panes.iter().next().map(|(pane, _)| *pane))
        .expect("desktop workspace has at least one pane");
    if workspace_panes.len() == 1 {
        let has_conversation = workspace_panes
            .iter()
            .any(|(_, pane)| matches!(pane, DesktopPane::Conversation(_)));
        if !has_conversation {
            if let Some((new_pane, _)) = workspace_panes.split(
                pane_grid::Axis::Vertical,
                first_pane,
                DesktopPane::Conversation(app.active_conversation().id),
            ) {
                pane_order.push(new_pane);
            }
        }
    }
    if pane_order.is_empty() {
        pane_order = desktop_pane_order(workspace_panes.layout());
    }
    let active_workspace_pane = app
        .settings
        .ui
        .active_desktop_workspace_pane
        .and_then(|index| pane_order.get(index).copied())
        .unwrap_or(first_pane);

    DesktopWorkspaceStartup {
        workspace_panes,
        active_workspace_pane,
    }
}

pub(in crate::desktop) fn conversation_startup_state(
    app: &App,
    workspace_panes: &pane_grid::State<DesktopPane>,
) -> ConversationDesktopState {
    let body_editors = app
        .workspace
        .conversations
        .iter()
        .map(|conversation| {
            (
                conversation.id,
                text_editor::Content::with_text(&conversation.draft_body),
            )
        })
        .collect::<HashMap<_, _>>();
    let message_counts = app
        .workspace
        .conversations
        .iter()
        .map(|conversation| (conversation.id, 0))
        .collect::<HashMap<_, _>>();
    let scroll_offsets = workspace_panes
        .iter()
        .filter_map(|(_, pane)| match pane {
            DesktopPane::Conversation(conversation_id) => Some((
                *conversation_id,
                scrollable::RelativeOffset { x: 0.0, y: 1.0 },
            )),
            DesktopPane::Browser(_) => None,
            #[cfg(feature = "chat-client")]
            DesktopPane::OmenChat(_) => None,
        })
        .collect::<HashMap<_, _>>();
    let scroll_restore_locks = scroll_offsets.keys().copied().collect::<HashSet<_>>();
    ConversationDesktopState {
        body_editors,
        message_counts,
        scroll_offsets,
        scroll_restore_locks,
    }
}

pub(in crate::desktop) fn clearweb_startup_state(app: &App) -> ClearwebDesktopState {
    let external_browsers = detect_external_browsers(
        app.settings
            .clearweb
            .preferred_external_browser_command
            .as_deref(),
    );
    let clearweb_proxy_endpoint = detect_clearweb_socks_proxy(
        &app.settings.clearweb.socks_proxy_host,
        app.settings.clearweb.socks_proxy_port,
    );
    let clearweb_proxy_reachable = clearweb_proxy_endpoint.is_some();
    ClearwebDesktopState {
        external_link_prompt: None,
        external_browsers,
        clearweb_proxy_reachable,
        clearweb_proxy_endpoint,
    }
}
