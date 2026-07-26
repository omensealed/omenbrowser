#![allow(dead_code)]

#[cfg(feature = "native-rns-net")]
compile_error!(
    "native-rns-net was removed from OMENbrowser_rs; use chat-client-rns-clean/native-network with reticulum-rs 0.9"
);

pub mod app;
pub mod browser;
#[cfg(feature = "chat-client")]
pub mod chat;
pub mod cli_frontend;
pub mod cli_help;
pub mod cli_network;
pub mod cli_overrides;
pub mod cli_redaction;
pub mod cli_report_logs;
pub mod cli_secret;
pub mod cli_values;
pub mod config;
#[cfg(feature = "desktop-ui")]
pub mod desktop;
pub mod diagnostics;
pub mod directory;
pub mod error;
pub mod history_search;
pub mod identity;
pub mod input;
pub mod interfaces;
pub mod media;
pub mod messaging;
pub mod micron;
mod msgpack;
pub mod operations;
pub mod plugins;
pub mod product_identity;
mod protocol_limits;
pub mod runtime;
pub mod storage;
pub mod structured_log_reader;
pub mod structured_log_writer;
#[cfg(feature = "tui")]
pub mod ui;
pub mod workspace;
