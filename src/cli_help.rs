//! Stable command-line help rendered by the compatibility binary.
//!
//! Keep command spellings synchronized with the parser and release docs. The
//! trailing newline is part of the binary's established output contract.

pub const HELP_TEXT: &str = concat!(
    "OMENbrowser_rs\n\nUSAGE:\n  omenbrowser_rs\n  omenbrowser_rs --version\n  omenbrowser_rs --desktop [--app-root <dir>]\n  omenbrowser_rs --tui [--app-root <dir>]\n  omenbrowser_rs --generate-native-identity <label> [--app-root <dir>] [--reticulum-config <dir>] [--output <file>] [--stdout]\n  omenbrowser_rs --native-startup [--app-root <dir>] [--backend reticulum] [--identity <file>] [--reticulum-config <dir>] [--tcp-client host:port] [--output <file>] [--stdout] [--suggest-shell] [--bundle-report <dir>]\n  omenbrowser_rs --native-live-sequence <destination:path> [--known-destinations <file>] [--path-wait <secs>] [--send-lxmf-smoke <peer_hash>] [--lxmf-smoke-method direct|propagated] [--propagation-node <hash>] [--lxmf-include-ticket] [--lxmf-interop|--lxmf-wait <secs>] [--preflight-wait <ms>] [--app-root <dir>] [--identity <file>] [--reticulum-config <dir>] [--tcp-client host:port] [--output <file>] [--stdout] [--suggest-shell] [--bundle-report <dir>]\n  omenbrowser_rs --native-validate <destination:path> [--known-destinations <file>] [--path-wait <secs>] [--send-lxmf-smoke <peer_hash>] [--lxmf-smoke-method direct|propagated] [--propagation-node <hash>] [--lxmf-include-ticket] [--lxmf-interop|--lxmf-wait <secs>] [--app-root <dir>] [--identity <file>] [--reticulum-config <dir>] [--tcp-client host:port] [--output <file>] [--stdout] [--suggest-shell] [--bundle-report <dir>]\n  omenbrowser_rs --native-preflight <destination:path> [--preflight-wait <ms>] [--send-lxmf-smoke <peer_hash>] [--app-root <dir>] [--backend reticulum] [--identity <file>] [--reticulum-config <dir>] [--tcp-client host:port] [--known-destinations <file>] [--output <file>] [--stdout] [--suggest-shell] [--bundle-report <dir>]\n  omenbrowser_rs --native-smoke <destination:path> [--known-destinations <file>] [--generate-known-destinations-fixture <file>] [--warm-path] [--path-wait <secs>] [--live] [--fetch-page] [--send-lxmf-smoke <peer_hash>] [--lxmf-smoke-method direct|propagated] [--propagation-node <hash>] [--lxmf-include-ticket] [--lxmf-interop|--lxmf-wait <secs>] [--app-root <dir>] [--backend reticulum] [--identity <file>] [--reticulum-config <dir>] [--tcp-client host:port] [--output <file>] [--stdout] [--suggest-shell] [--bundle-report <dir>]\n  omenbrowser_rs --omenchat-smoke <destination_hash> [--omenchat-room lobby] [--omenchat-message text] [--omenchat-reaction-smoke] [--omenchat-upload-file <file>] [--omenchat-fetch-upload <filename>] [--omenchat-fetch-upload-bytes <n>] [--path-wait <secs>] [--known-destinations <file>] [--app-root <dir>] [--backend reticulum] [--identity <file>] [--reticulum-config <dir>] [--tcp-client host:port] [--network-name name] [--passphrase secret] [--output <file>] [--stdout]\n  omenbrowser_rs --lxmf-interop [--send-lxmf-smoke <peer_hash>] [--lxmf-wait <secs>] [--backend reticulum] [--identity <file>] [--tcp-client host:port] [--stdout] [--suggest-shell] [--bundle-report <dir>]\n\nOPTIONS:\n  --desktop, --iced            Open the iced desktop UI; this is the default when desktop-ui is compiled\n  --tui, --terminal            Open the legacy ratatui terminal UI when the tui feature is compiled\n  --version, -V                Print version and compiled feature summary\n  --generate-native-identity   Create and activate managed native Reticulum identity material; requires native-reticulum/native-network features\n  --native-startup             Start the configured runtime, collect status/interface data, then stop cleanly\n  --native-live-sequence       Run startup, preflight, live NomadNet validation, and optional LXMF interop into one JSON report\n  --native-validate            Run the live native NomadNet validation path: reticulum backend, path warmup, live probe, and fetch_page\n  --native-preflight, --preflight\n                               Validate native-network CLI inputs without starting live fetch or LXMF delivery\n  --preflight-wait <ms>        Runtime event wait for preflight transport startup; default is 250 ms\n  --native-smoke, --smoke-test  Run a non-TUI native-network smoke report for a NomadNet address\n  --omenchat-smoke <hash>      Open an OMENchat Link, join a room, send one message, optionally test reactions or upload/fetch a file, and report JSON evidence\n  --omenchat-reaction-smoke    Exercise durable reaction replay, snapshots, no-op, and removal against the smoke message\n  --known-destinations <file>  Preload a Python/RNS-compatible known_destinations cache for this command\n  --generate-known-destinations-fixture <file>\n                               Write a dev/test known_destinations fixture for the smoke destination and preload it\n  --warm-path, --request-path   Request/warm the destination path before probing; default wait is 5 seconds\n  --path-wait <secs>           Set warm-path event wait seconds and enable path warmup\n  --live                       Include the explicit live page probe step\n  --fetch-page, --live-fetch   Also call the normal runtime fetch_page path and include response metadata\n  --send-lxmf-smoke <peer_hash>\n                               Explicitly send a labeled native LXMF smoke-test message when readiness passes\n  --lxmf-interop               Announce local lxmf.delivery and wait up to 10s for LXMF/proof events; can be used without --native-smoke\n  --lxmf-wait <secs>           Announce local lxmf.delivery and wait this many seconds for LXMF/proof events\n  --app-root <dir>             Temporarily use this app data root for frontend and smoke command files\n  --backend <name>             Temporarily use auto, mock, or reticulum for this command\n  --identity <file>            Temporarily attach this identity path for this command\n  --reticulum-config <dir>     Temporarily use this Reticulum config directory\n  --tcp-client <host:port>     Temporarily use a TCP client interface endpoint\n  --network-name <name>        Set IFAC network name for the temporary TCP client\n  --passphrase <secret>        Set IFAC passphrase for the temporary TCP client\n  --output, -o <file>          Write report JSON to this path\n  --stdout                     Print report JSON to stdout\n  --suggest-shell              Include shell-escaped suggested command lines in stderr summaries and bundle summary.txt\n  --bundle-report <dir>        Write report.json, summary.txt, command.json, environment.json, and logs.json under a timestamped directory\n  --help, -h                   Show this help\n\nWithout --output, --stdout, or --bundle-report, reports are written under the diagnostics directory. CLI overrides are command-local and do not rewrite saved settings, except --generate-native-identity activates the new managed identity.\n",
    "\nOMENCHAT REVISION SMOKE:\n  --omenchat-revision-smoke               Exercise durable correction replay, tombstone, and authoritative Resource recovery against the smoke message\n",
    "\nOMENCHAT PIN SMOKE:\n  --omenchat-pin-smoke                    Exercise moderator-only durable pin replay, snapshot recovery, no-op, and unpin against the smoke message\n",
    "\nLXMF INVITATION SMOKE:\n  --lxmf-invitation-smoke <server_hash>   Opt-in tokenless invitation evidence; add --send-lxmf-smoke <peer_hash> to send, or omit it for receive-only\n  --lxmf-invitation-capability-probe <peer_hash>\n                               Probe only the managed-native invitation capability; sends no invitation and never retries\n  --lxmf-invitation-capability-cancel-after-ms <ms>\n                               Cancel and drain the probe after at most 15000 ms; 0 is the deterministic pre-cancel gate\n",
    "\nLXMF TOPIC CAPABILITY PROBE:\n  --lxmf-topic-capability-probe\n                               Negotiate external local SDK/RPC topic capabilities once; never subscribes, publishes, retries, or shuts down the daemon\n",
    "\nOMENCHAT RECONNECT SMOKE:\n  --omenchat-reconnect-ready-file <path>  Create an isolated ready marker, then keep this smoke process alive across one server restart\n  --omenchat-reconnect-wait <secs>        Bound link-close and reconnect work; default is 60 seconds\n",
    "\nSAFE IFAC PASSPHRASE INPUT:\n  --passphrase-file <path>     Read from an owner-only regular file\n  --passphrase-stdin           Read from standard input\n  --passphrase-prompt          Read from the terminal with echo disabled\n  --passphrase <secret>        Deprecated: argv may be visible to other processes\n"
);

/// Return the complete help document, including its single trailing newline.
pub const fn help_text() -> &'static str {
    HELP_TEXT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_output_preserves_document_shape() {
        assert!(HELP_TEXT.starts_with("OMENbrowser_rs\n\nUSAGE:\n"));
        assert!(HELP_TEXT.contains("\n\nOPTIONS:\n"));
        assert!(HELP_TEXT.contains("\n\nSAFE IFAC PASSPHRASE INPUT:\n"));
        assert!(HELP_TEXT.ends_with("visible to other processes\n"));
        assert!(!HELP_TEXT.ends_with("\n\n"));
        assert_eq!(HELP_TEXT.lines().count(), 82);
    }

    #[test]
    fn help_documents_compatibility_commands_and_safe_secret_inputs() {
        for command in [
            "--desktop",
            "--tui",
            "--generate-native-identity",
            "--native-startup",
            "--native-live-sequence",
            "--native-validate",
            "--native-preflight",
            "--native-smoke",
            "--omenchat-smoke",
            "--omenchat-reaction-smoke",
            "--omenchat-revision-smoke",
            "--omenchat-pin-smoke",
            "--omenchat-reconnect-ready-file",
            "--lxmf-interop",
            "--lxmf-invitation-smoke",
            "--lxmf-invitation-capability-probe",
            "--lxmf-invitation-capability-cancel-after-ms",
            "--lxmf-topic-capability-probe",
        ] {
            assert!(HELP_TEXT.contains(command), "missing {command}");
        }
        for source in [
            "--passphrase-file",
            "--passphrase-stdin",
            "--passphrase-prompt",
        ] {
            assert!(HELP_TEXT.contains(source), "missing {source}");
        }
        assert!(HELP_TEXT.contains("Deprecated: argv may be visible"));
    }
}
