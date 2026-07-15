use iced::widget::Button;

use crate::interfaces::{GatewayPreset, InterfaceKind};

use super::{omen_button_owned, subtle_button_owned, warning_button_owned};
use crate::desktop::{DesktopApp, InterfaceMessage, Message};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GatewayPresetButtonState {
    Missing,
    Disabled,
    Enabled,
}

pub(in crate::desktop) fn gateway_preset_buttons(
    desktop: &DesktopApp,
) -> Vec<Button<'static, Message>> {
    desktop
        .app
        .interface_service
        .gateway_presets()
        .unwrap_or_default()
        .into_iter()
        .map(|preset| {
            let state = gateway_preset_button_state(desktop, &preset);
            let message = Message::Interface(InterfaceMessage::CreateGatewayPreset(preset.id));
            match state {
                GatewayPresetButtonState::Missing => {
                    subtle_button_owned(format!("Add {}", preset.name), message)
                }
                GatewayPresetButtonState::Disabled => {
                    warning_button_owned(format!("Enable {}", preset.name), message)
                }
                GatewayPresetButtonState::Enabled => {
                    omen_button_owned(format!("{} Enabled", preset.name), message)
                }
            }
        })
        .collect()
}

pub(in crate::desktop) fn gateway_preset_status_line(desktop: &DesktopApp) -> String {
    let presets = desktop
        .app
        .interface_service
        .gateway_presets()
        .unwrap_or_default();
    if presets.is_empty() {
        return "gateway presets: none configured".into();
    }
    let parts = presets
        .iter()
        .map(|preset| {
            let state = match gateway_preset_button_state(desktop, preset) {
                GatewayPresetButtonState::Missing => "missing",
                GatewayPresetButtonState::Disabled => "disabled",
                GatewayPresetButtonState::Enabled => "enabled",
            };
            format!("{}={state}", preset.name)
        })
        .collect::<Vec<_>>();
    let all_enabled = presets.iter().all(|preset| {
        matches!(
            gateway_preset_button_state(desktop, preset),
            GatewayPresetButtonState::Enabled
        )
    });
    let next = if all_enabled {
        "restart if you just changed interfaces"
    } else {
        "add or enable WNS/RMAP, then restart"
    };
    format!("gateway presets: {} | next: {next}", parts.join(" | "))
}

fn gateway_preset_button_state(
    desktop: &DesktopApp,
    preset: &GatewayPreset,
) -> GatewayPresetButtonState {
    let mut has_disabled = false;
    for profile in &desktop.app.interfaces_state.profiles {
        if profile.kind == InterfaceKind::TcpClient
            && profile.target_host == preset.host
            && profile.target_port == preset.port
        {
            if profile.enabled {
                return GatewayPresetButtonState::Enabled;
            }
            has_disabled = true;
        }
    }
    if has_disabled {
        GatewayPresetButtonState::Disabled
    } else {
        GatewayPresetButtonState::Missing
    }
}
