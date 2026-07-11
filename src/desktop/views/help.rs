use iced::widget::{column, text};
use iced::{Element, Length};

use super::super::{
    app_scrollable, section_card, ui_size, wrapped_panel_text, wrapped_text_owned, DesktopApp,
    Message, LXMF_HELP_LINES, OMENCHATD_OPERATOR_HELP_LINES, OMENCHAT_HISTORY_HELP_LINES,
    OMENCHAT_MEDIA_HELP_LINES, OMENCHAT_RELEASE_TEST_HELP_LINES,
};

pub(in crate::desktop) fn help_view(_desktop: &DesktopApp) -> Element<'_, Message> {
    let browser_help = column![
        wrapped_panel_text("Open NomadNet pages with destination:/path.mu or paste a full destination hash into the address field. Use Request Path when the route/key is unknown, then retry after the path status returns pass."),
        wrapped_panel_text("Back, Forward, Reload, Stop, Identify, Capture, and Diag act on the selected browser pane only. Diag opens diagnostics for that pane's current destination."),
        wrapped_panel_text("Ctrl + Plus zooms in, Ctrl + Minus zooms out, and Ctrl + mouse wheel also zooms only the active Micron viewport. The Top button returns that viewport to the first rendered row."),
        wrapped_panel_text("NomadNet non-.mu file links download through the configured downloads path. HTTP/HTTPS links open through the external browser prompt; use Copy URL for Tor Browser."),
    ]
    .spacing(6);

    let micron_help = column![
        wrapped_panel_text("Micron is rendered as a styled cell grid, so half-block art, true-color headers, links, forms, and focus order are preserved inside the viewport."),
        wrapped_panel_text("Tab and Shift+Tab move focus through links and form fields in the active viewport. Enter activates the focused link or submits the focused form action."),
        wrapped_panel_text("MicronPlus pages can expose live regions and UI controls. Live refreshes are quiet on success and report failures in the browser status/error surface."),
    ]
    .spacing(6);

    let omenchat_commands = column![
        wrapped_text_owned("/me <action> - send an action message", 14),
        wrapped_text_owned("/join <room> - switch to a room", 14),
        wrapped_text_owned("/part [room] - leave a room", 14),
        wrapped_text_owned("/rooms - list rooms advertised by the server", 14),
        wrapped_text_owned("/who - list visible users in the active room", 14),
        wrapped_text_owned("/upload <path> - offer a local file upload to the active room; the attach button opens a native file picker and sends the selected file", 14),
        wrapped_text_owned("/notice <text> - send a room notice; moderator/admin only", 14),
        wrapped_text_owned("/topic <text> - change the active room topic; moderator/admin only", 14),
        wrapped_text_owned("/create-room <room> [topic] - create a room; admin only; /create and /mkroom also work", 14),
        wrapped_text_owned("/kick <user>, /ban <user>, /unban <user> - moderation actions", 14),
        wrapped_text_owned("/mute <user>, /unmute <user> - moderation actions", 14),
        wrapped_text_owned("/role <user> <standard|trusted|mod|admin> - change a user role; admin only", 14),
    ]
    .spacing(4);

    let omenchat_help = column![
        wrapped_panel_text("Open OMENchat with omenchat://<destination hash> from the Browser workspace, a NomadNet link, or the OMENchat quick open field."),
        wrapped_panel_text("Path requests route to the selected OMENchat server. Reconnect restarts the live link and cancels stale reconnect attempts."),
        wrapped_panel_text("Load Older asks the server/client cache for earlier room history. Room history is cached locally per identity and server."),
        wrapped_panel_text("Enter sends the composer draft. The input clears after a successful send."),
        section_card("OMENchat Slash Commands", omenchat_commands),
    ]
    .spacing(8);

    let omenchat_release_help = OMENCHAT_RELEASE_TEST_HELP_LINES
        .iter()
        .fold(column![].spacing(6), |column, line| {
            column.push(wrapped_text_owned(*line, 14))
        });

    let omenchat_history_help = OMENCHAT_HISTORY_HELP_LINES
        .iter()
        .fold(column![].spacing(6), |column, line| {
            column.push(wrapped_text_owned(*line, 14))
        });

    let omenchat_media_help = OMENCHAT_MEDIA_HELP_LINES
        .iter()
        .fold(column![].spacing(6), |column, line| {
            column.push(wrapped_text_owned(*line, 14))
        });

    let omenchatd_help = OMENCHATD_OPERATOR_HELP_LINES
        .iter()
        .fold(column![].spacing(6), |column, line| {
            column.push(wrapped_text_owned(*line, 14))
        });

    let lxmf_help = LXMF_HELP_LINES
        .iter()
        .fold(column![].spacing(6), |column, line| {
            column.push(wrapped_text_owned(*line, 14))
        });

    let admin_help = column![
        wrapped_text_owned("Directory remembers selected nodes, peers, and propagation nodes. Trust controls affect defaults and safe interaction choices.", 14),
        wrapped_text_owned("Identities create separate identity material and per-identity storage roots. Delete Active is the only destructive identity action and requires confirmation.", 14),
        wrapped_text_owned("Interfaces edits the active identity's Reticulum config. Diagnostics, Logs, and Monitoring are the places to inspect runtime behavior and traffic.", 14),
        wrapped_text_owned("omenchatd keeps its own server root under ~/.omenchatd by default and should not touch ~/.reticulum, ~/.nomadnetwork, or OMENbrowser_rs identity storage.", 14),
    ]
    .spacing(6);

    app_scrollable(
        column![
            text("Help").size(ui_size(28)),
            section_card("Browser", browser_help),
            section_card("Micron And MicronPlus", micron_help),
            section_card("OMENchat Plugin Client", omenchat_help),
            section_card("OMENchat History Sync", omenchat_history_help),
            section_card("OMENchat Media Privacy And Uploads", omenchat_media_help),
            section_card("OMENchat Release Testing", omenchat_release_help),
            section_card("omenchatd Operator Notes", omenchatd_help),
            section_card("LXMF Messages", lxmf_help),
            section_card("Directory, Identities, And Admin", admin_help),
        ]
        .spacing(12)
        .width(Length::Fill),
    )
    .height(Length::Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_help_documents_release_isolation_and_server_storage() {
        let release_help = OMENCHAT_RELEASE_TEST_HELP_LINES.join("\n");
        assert!(release_help.contains("isolated app root"));
        assert!(release_help.contains("--desktop --app-root /tmp/omenbrowser-rs-test"));
        assert!(release_help.contains("--desktop --app-root /tmp/omenbrowser-rs-test-2"));
        assert!(release_help.contains("instance_name suffix"));
        assert!(release_help.contains("omenchat://<destination hash>"));

        let server_help = OMENCHATD_OPERATOR_HELP_LINES.join("\n");
        assert!(server_help.contains("~/.omenchatd"));
        assert!(server_help.contains("reticulum/storage/pages/index.mu"));
        assert!(server_help.contains("omenchat.node"));
        assert!(server_help.contains("nomadnetwork.node"));
        assert!(server_help.contains("--features live-reticulum -- run"));
        assert!(server_help.contains("--features live-reticulum -- tui"));

        let history_help = OMENCHAT_HISTORY_HELP_LINES.join("\n");
        assert!(history_help.contains("bounded recent room history"));
        assert!(history_help.contains("Load Older"));
        assert!(history_help.contains("server event id"));
        assert!(history_help.contains("HistoryRecent/HistoryBefore"));

        let media_help = OMENCHAT_MEDIA_HELP_LINES.join("\n");
        assert!(media_help.contains("animated GIFs"));
        assert!(media_help.contains("127.0.0.1:9050"));
        assert!(media_help.contains("127.0.0.1:9150"));
        assert!(media_help.contains("512 KiB max per file"));
        assert!(media_help.contains("native file picker"));
    }

    #[test]
    fn lxmf_help_documents_native_ticket_and_receipt_limits() {
        let help = LXMF_HELP_LINES.join("\n");

        assert!(help.contains("direct or propagated"));
        assert!(help.contains("ticketed sends include LXMF reply tickets"));
        assert!(help.contains("peer-advertised direct stamp costs are honored"));
        assert!(help.contains("not the same as a guaranteed peer-side LXMF receipt"));
    }
}
