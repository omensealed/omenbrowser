mod app;
mod browser;
mod clearweb;
mod clearweb_state;
mod command_palette;
mod constants;
mod conversation;
mod conversation_active;
mod conversation_editor;
mod conversation_pane;
mod conversation_state;
mod diagnostics;
mod directory;
mod external_browser;
mod fonts;
mod icons;
mod identity;
mod input;
mod interfaces;
mod layout;
mod lxmf_links;
mod message;
mod message_compact;
mod message_detail;
mod message_labels;
mod message_retry;
mod message_secondary;
mod message_stamp;
mod message_status;
mod monitoring_state;
#[cfg(feature = "chat-client")]
mod omenchat_commands;
#[cfg(feature = "chat-client")]
mod omenchat_desktop_state;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
mod omenchat_diagnostics;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
mod omenchat_live_drain;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
mod omenchat_live_heartbeat;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
mod omenchat_live_reconnect;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
mod omenchat_live_send;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
mod omenchat_live_session;
#[cfg(feature = "chat-client")]
mod omenchat_live_transport;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
mod omenchat_live_update;
#[cfg(feature = "chat-client")]
mod omenchat_media_tasks;
#[cfg(feature = "chat-client")]
mod omenchat_media_update;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
mod omenchat_mutations;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
mod omenchat_recent_sync;
mod omenchat_runtime;
#[cfg(feature = "chat-client")]
mod omenchat_session;
#[cfg(feature = "chat-client")]
mod omenchat_state;
#[cfg(feature = "chat-client")]
mod omenchat_update;
mod page_widget;
mod page_widget_canvas;
mod plugins;
mod runtime_update;
mod shell_update;
mod startup;
mod state;
mod subscriptions;
mod theme;
mod ui_state;
mod update;
mod views;
mod widgets;
mod workspace_panes;
mod workspace_persistence;
mod workspace_scroll;
mod workspace_scroll_conversation;
#[cfg(feature = "chat-client")]
mod workspace_scroll_omenchat;
mod workspace_state;
mod workspace_tabs;

#[cfg(feature = "chat-client")]
use iced::font::Style as FontStyle;
use iced::widget::scrollable::Viewport;

use crate::app::{current_epoch_ms, BrowserFieldEditor, TabId};
#[cfg(feature = "chat-client")]
use crate::chat::protocol::RoomId;
#[cfg(feature = "chat-client")]
use crate::chat::ChatSessionId;
use crate::workspace::WorkspaceSection;
pub use app::run;
#[cfg(feature = "chat-client")]
pub(in crate::desktop) use clearweb::fetch_clearweb_media_over_socks;
pub(in crate::desktop) use command_palette::{
    bounded_command_palette_query, command_palette_input_id, command_palette_message,
    command_palette_overlay, command_palette_results,
};
pub(in crate::desktop) use constants::*;
pub(in crate::desktop) use conversation_editor::conversation_editor_text;
use external_browser::*;
use fonts::*;
use icons::*;
use layout::*;
use message::*;
use message_compact::lxmf_message_compact_status;
pub(in crate::desktop) use message_retry::{
    desktop_message_is_cancel_candidate, desktop_message_is_retry_candidate,
    desktop_message_propagation_sync_label, desktop_message_retry_labels,
};
use message_stamp::lxmf_message_compact_stamp_status;
use message_status::lxmf_message_status_lines;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
pub(in crate::desktop) use omenchat_recent_sync::omenchat_recent_sync_wants_bottom_restore;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use omenchat_runtime::DesktopOmenChatTransport;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
pub(in crate::desktop) use omenchat_runtime::{
    delayed_omenchat_reconnect_if_disconnected_task, hex_bytes,
    omenchat_close_reason_allows_quick_reconnect, omenchat_close_reason_is_timeout,
    omenchat_live_open_error_status,
};
#[cfg(feature = "chat-client")]
pub(in crate::desktop) use omenchat_runtime::{omenchat_event_counts_by_room, request_session_id};
use page_widget::{color_from_style, nomadnet_page_with_row_renderer, NomadNetPageProps};
pub(in crate::desktop) use state::DesktopApp;
use theme::*;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
pub(in crate::desktop) use views::directory_model::short_destination_hash;
pub(in crate::desktop) use widgets::*;
pub(in crate::desktop) use workspace_scroll::sanitize_scroll_offset;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
pub(in crate::desktop) use workspace_scroll::scroll_offset_is_at_bottom;
pub(in crate::desktop) use workspace_scroll_conversation::conversation_scroll_id;
#[cfg(feature = "chat-client")]
pub(in crate::desktop) use workspace_scroll_omenchat::{
    omenchat_offset_from_bottom_anchored_viewport, omenchat_scroll_id,
};
