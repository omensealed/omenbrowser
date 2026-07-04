use crate::interfaces::{InterfaceKind, ReticulumInterfaceProfile};
use crate::runtime::native::NativeRuntimeError;

#[derive(Clone, PartialEq, Eq)]
pub struct NativeInterfacePlan {
    pub profile_id: String,
    pub name: String,
    pub kind: String,
    pub enabled: bool,
    pub supported: bool,
    pub endpoint: Option<NativeTcpEndpoint>,
    pub ifac_network_name: Option<String>,
    pub ifac_passphrase: Option<String>,
    pub ifac_configured: bool,
    pub reason: Option<String>,
}

impl std::fmt::Debug for NativeInterfacePlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeInterfacePlan")
            .field("profile_id", &self.profile_id)
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("enabled", &self.enabled)
            .field("supported", &self.supported)
            .field("endpoint", &self.endpoint)
            .field("ifac_network_name", &self.ifac_network_name)
            .field(
                "ifac_passphrase",
                &self.ifac_passphrase.as_ref().map(|_| "<redacted>"),
            )
            .field("ifac_configured", &self.ifac_configured)
            .field("reason", &self.reason)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeTcpEndpoint {
    pub host: String,
    pub port: u16,
}

pub fn plan_interface(profile: &ReticulumInterfaceProfile) -> NativeInterfacePlan {
    let kind = kind_label(&profile.kind);
    let endpoint = match profile.kind {
        InterfaceKind::TcpClient | InterfaceKind::TcpServer => Some(NativeTcpEndpoint {
            host: profile.target_host.clone(),
            port: profile.target_port,
        }),
        _ => None,
    };
    let supported = matches!(profile.kind, InterfaceKind::Auto | InterfaceKind::TcpClient);

    NativeInterfacePlan {
        profile_id: profile.profile_id.clone(),
        name: profile.name.clone(),
        kind,
        enabled: profile.enabled,
        supported,
        endpoint,
        ifac_network_name: (!profile.network_name.is_empty()).then(|| profile.network_name.clone()),
        ifac_passphrase: (!profile.passphrase.is_empty()).then(|| profile.passphrase.clone()),
        ifac_configured: !profile.network_name.is_empty() || !profile.passphrase.is_empty(),
        reason: (!supported)
            .then(|| "native interface startup is not implemented for this profile kind".into()),
    }
}

pub fn plan_interfaces(profiles: &[ReticulumInterfaceProfile]) -> Vec<NativeInterfacePlan> {
    profiles.iter().map(plan_interface).collect()
}

pub fn validate_startup_plans(plans: &[NativeInterfacePlan]) -> Result<(), NativeRuntimeError> {
    let unsupported = plans.iter().find(|plan| plan.enabled && !plan.supported);

    match unsupported {
        Some(plan) => Err(NativeRuntimeError::UnsupportedInterface {
            profile: plan.name.clone(),
            kind: plan.kind.clone(),
            reason: plan
                .reason
                .clone()
                .unwrap_or_else(|| "native interface unsupported".into()),
        }),
        None => Ok(()),
    }
}

fn kind_label(kind: &InterfaceKind) -> String {
    match kind {
        InterfaceKind::Auto => "auto".into(),
        InterfaceKind::TcpClient => "tcp_client".into(),
        InterfaceKind::TcpServer => "tcp_server".into(),
        InterfaceKind::I2p => "i2p".into(),
        InterfaceKind::RNode => "rnode".into(),
        InterfaceKind::Unknown(kind) => kind.clone(),
    }
}
