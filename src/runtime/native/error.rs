use crate::error::AppError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeRuntimeError {
    IdentityMissing,
    IdentityInvalid,
    InvalidAddress(String),
    PageFetchFailed {
        destination: String,
        stage: NativePageFetchFailureStage,
        detail: String,
    },
    UnsupportedInterface {
        profile: String,
        kind: String,
        reason: String,
    },
    InvalidInterface {
        profile: String,
        kind: String,
        reason: String,
    },
    PathUnavailable(String),
    Timeout(String),
    InvalidResponse(String),
    Cancelled,
    Unsupported(&'static str),
    Native(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativePageFetchFailureStage {
    Runtime,
    DestinationIdentity,
    PathDiscovery,
    LinkSetup,
    RequestSend,
    ResponseWait,
    ResponseDecode,
}

impl NativePageFetchFailureStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Runtime => "runtime setup",
            Self::DestinationIdentity => "destination identity",
            Self::PathDiscovery => "path discovery",
            Self::LinkSetup => "link setup",
            Self::RequestSend => "request send",
            Self::ResponseWait => "response wait",
            Self::ResponseDecode => "response decode",
        }
    }
}

impl From<NativeRuntimeError> for AppError {
    fn from(value: NativeRuntimeError) -> Self {
        match value {
            NativeRuntimeError::IdentityMissing => {
                AppError::Runtime("native Reticulum identity is missing".into())
            }
            NativeRuntimeError::IdentityInvalid => {
                AppError::Runtime("native Reticulum identity is invalid".into())
            }
            NativeRuntimeError::InvalidAddress(address) => {
                AppError::Browser(format!("invalid native Reticulum address: {address}"))
            }
            NativeRuntimeError::PageFetchFailed {
                destination,
                stage,
                detail,
            } => AppError::Runtime(format!(
                "native Reticulum page fetch failed for {destination} during {}: {detail}",
                stage.as_str()
            )),
            NativeRuntimeError::UnsupportedInterface {
                profile,
                kind,
                reason,
            } => AppError::Unsupported(format!(
                "native Reticulum interface '{profile}' ({kind}) is unsupported: {reason}"
            )),
            NativeRuntimeError::InvalidInterface {
                profile,
                kind,
                reason,
            } => AppError::Runtime(format!(
                "native Reticulum interface '{profile}' ({kind}) is invalid: {reason}"
            )),
            NativeRuntimeError::PathUnavailable(destination) => {
                AppError::Runtime(format!("Reticulum path unavailable for {destination}"))
            }
            NativeRuntimeError::Timeout(operation) => {
                AppError::Runtime(format!("native Reticulum timeout during {operation}"))
            }
            NativeRuntimeError::InvalidResponse(operation) => AppError::Runtime(format!(
                "native Reticulum invalid response during {operation}"
            )),
            NativeRuntimeError::Cancelled => {
                AppError::Runtime("native Reticulum operation cancelled".into())
            }
            NativeRuntimeError::Unsupported(message) => AppError::Unsupported(message.into()),
            NativeRuntimeError::Native(message) => AppError::Runtime(message),
        }
    }
}
