pub(in crate::desktop) const DIRECTORY_RENDER_LIMIT: usize = 80;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
pub(in crate::desktop) const OMENCHAT_RECENT_SYNC_MAX_ATTEMPTS: u8 = 3;
pub(in crate::desktop) const DESKTOP_IDLE_TICK_MS: u64 = 1_000;
pub(in crate::desktop) const DESKTOP_LIVE_TICK_MS: u64 = 250;
// Reticulum links can go stale quickly on quiet paths. A lightweight OMENchat
// ping below that window is less noisy than repeated teardown/reconnect/history sync.
pub(in crate::desktop) const OMENCHAT_HEARTBEAT_IDLE_MS: u64 = 4_000;
pub(in crate::desktop) const OMENCHAT_HEARTBEAT_TIMEOUT_MS: u64 = 18_000;
pub(in crate::desktop) const OMENCHAT_MIN_HEARTBEAT_IDLE_MS: u64 = 5_000;
pub(in crate::desktop) const OMENCHAT_MAX_HEARTBEAT_IDLE_MS: u64 = 600_000;
pub(in crate::desktop) const OMENCHAT_PATH_RECONNECT_DELAY_MS: u64 = 2_000;
pub(in crate::desktop) const OMENCHAT_MESSAGE_GROUP_GAP_SECS: i64 = 5 * 60;
#[cfg(feature = "chat-client")]
pub(in crate::desktop) const OMENCHAT_LOCAL_ECHO_RESEND_SECS: i64 = 15;
#[cfg(feature = "chat-client")]
pub(in crate::desktop) const OMENCHAT_INLINE_MEDIA_HEADER_BYTES: usize = 128 * 1024;
#[cfg(feature = "chat-client")]
pub(in crate::desktop) const OMENCHAT_GIF_ANIMATION_SCAN_BYTES: usize = 512 * 1024;
#[cfg(feature = "chat-client")]
pub(in crate::desktop) const OMENCHAT_INLINE_MEDIA_MAX_WIDTH: f32 = 520.0;
#[cfg(feature = "chat-client")]
pub(in crate::desktop) const OMENCHAT_INLINE_MEDIA_MAX_HEIGHT: f32 = 360.0;
#[cfg(feature = "chat-client")]
pub(in crate::desktop) const OMENCHAT_PENDING_DESTINATION_PREFIX: &str = "pending-omenchat-";

pub(in crate::desktop) const CONVERSATION_VISIBLE_MESSAGES: usize = 8;
pub(in crate::desktop) const CONVERSATION_PREVIEW_CHARS: usize = 220;
pub(in crate::desktop) const CONVERSATION_PREVIEW_LINES: usize = 5;
pub(in crate::desktop) const CONVERSATION_MICRON_PREVIEW_WIDTH: usize = 80;
pub(in crate::desktop) const LOG_VISIBLE_ENTRIES: usize = 48;

pub(in crate::desktop) const LXMF_HELP_LINES: &[&str] = &[
    "Messages can be direct or propagated. Direct sends use live paths; propagated sends hand the envelope to the selected propagation node.",
    "Sync Propagation checks the selected propagation node. Path/Diag buttons help inspect peer and path state without burying it in logs.",
    "Native ticketed sends include LXMF reply tickets. Propagation stamps are generated when the selected propagation node advertises a target stamp cost.",
    "Remembered inbound reply tickets are reused for direct ticket stamps. If no valid ticket is available, peer-advertised direct stamp costs are honored before sending.",
    "Transport proof, propagation-node acceptance, and inbound peer activity are useful evidence, but they are not the same as a guaranteed peer-side LXMF receipt.",
    "Unread counts clear when the matching conversation becomes active. Delete removes local conversation history for that peer.",
];

pub(in crate::desktop) const OMENCHAT_RELEASE_TEST_HELP_LINES: &[&str] = &[
    "For a second local test client, start OMENbrowser_rs with an isolated app root so it does not reuse your main identity, settings, message store, or plugin database.",
    "Example first client: ./target/release/omenbrowser_rs --desktop --app-root /tmp/omenbrowser-rs-test",
    "Example second client: ./target/release/omenbrowser_rs --desktop --app-root /tmp/omenbrowser-rs-test-2",
    "Use a separate OMENbrowser_rs config/storage root for each tester identity. Newly generated Reticulum configs include an instance_name suffix to avoid interface instance-name collisions.",
    "Open OMENchat servers with omenchat://<destination hash>. If the path/key is missing, use Path, wait for the path result, then Reconnect.",
    "A minimized OMENchat pane should highlight when new room messages arrive. Restoring the pane marks the active room read.",
];

pub(in crate::desktop) const OMENCHAT_HISTORY_HELP_LINES: &[&str] = &[
    "When an OMENchat pane opens or reconnects, it asks the server for the bounded recent room history and merges missing events into the local cache.",
    "The server-side history limit controls how much recent backlog is offered on join/reconnect. The client should not need Load Older just to catch up with the latest messages.",
    "Load Older requests the next older batch before the oldest locally cached event in the active room. If the server has nothing older, the pane should report that the room is already current.",
    "History batches are inserted by server event id, so recovered messages should appear in chronological order and still obey the normal same-user message stacking rules.",
    "If two clients disagree after reconnect, check Logs for HistoryRecent/HistoryBefore frames and the server TUI Monitoring/Logs for history resource offers or protocol errors.",
];

pub(in crate::desktop) const OMENCHAT_MEDIA_HELP_LINES: &[&str] = &[
    "OMENchat can preview cached images and animated GIFs inline. NomadNet/Reticulum media stays on the Reticulum path and does not use direct clearweb TCP.",
    "Clearweb HTTP/HTTPS image previews are privacy-gated. Remote media is off by default; when enabled, trusted OMENchat servers can auto-load images only through a detected SOCKS/Tor proxy on 127.0.0.1:9050 or 127.0.0.1:9150.",
    "Untrusted clearweb images require an explicit Load action. Non-image clearweb links open through the external browser prompt; use Copy URL for Tor Browser.",
    "Uploads use the native file picker from the attach button or /upload <path>. The server advertises both total upload quota and max file size; current defaults are 50 MiB quota per identity and 512 KiB max per file.",
    "Accepted upload images/GIFs are cached under the active identity's OMENchat media cache and rendered inline for supported image types. Oversized or rejected files should fail before transfer.",
];

pub(in crate::desktop) const OMENCHATD_OPERATOR_HELP_LINES: &[&str] = &[
    "omenchatd is standalone. Its default server root is ~/.omenchatd, including identity material, Reticulum config, SQLite database, logs, and operator-owned NomadNet pages.",
    "The server must not use ~/.reticulum, ~/.nomadnetwork, ~/.lxmd, or OMENbrowser_rs identity storage unless an operator explicitly points it somewhere else.",
    "The public chat destination announces as omenchat.node. The quiet NomadNet portal announces separately as nomadnetwork.node and serves /page/index.mu from reticulum/storage/pages/index.mu.",
    "Edit reticulum/storage/pages/index.mu for MOTD, server rules, room summaries, and the omenchat:// link. omenchatd creates the file only if it is missing and should not overwrite operator edits.",
    "Typical server start: cargo run --manifest-path src/server/Cargo.toml --features live-reticulum -- run",
    "Typical server setup UI: cargo run --manifest-path src/server/Cargo.toml --features live-reticulum -- tui",
    "Use omenchatd tui for setup, interfaces, rooms, moderation, monitoring, logs, audit, and help. Use omenchatd status for copyable identity, destination, portal path, limits, and storage information.",
    "Room creation is admin-only. Topic edits and kick/ban/mute actions are moderator/admin operations. Use the Moderation panel or slash commands from a privileged OMENchat client.",
];
