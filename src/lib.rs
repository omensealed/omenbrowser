#![allow(dead_code)]

#[cfg(feature = "native-rns-net")]
compile_error!(
    "native-rns-net was removed from OMENbrowser_rs; use chat-client-rns-clean/native-network with reticulum-rs 0.6"
);

pub mod app;
pub mod browser;
#[cfg(feature = "chat-client")]
pub mod chat;
pub mod config;
#[cfg(feature = "desktop-ui")]
pub mod desktop;
pub mod diagnostics;
pub mod directory;
pub mod error;
pub mod identity;
pub mod input;
pub mod interfaces;
pub mod media;
pub mod messaging;
pub mod micron;
pub mod plugins;
pub mod runtime;
pub mod storage;
#[cfg(feature = "tui")]
pub mod ui;
pub mod workspace;
