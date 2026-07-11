#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceSection {
    Browser,
    Messages,
    Directory,
    Identities,
    Interfaces,
    Monitoring,
    NetworkDoctor,
    Settings,
    Diagnostics,
    Logs,
    Plugins,
    Help,
}

impl WorkspaceSection {
    pub const ALL: [Self; 12] = [
        Self::Browser,
        Self::Messages,
        Self::Directory,
        Self::Identities,
        Self::Interfaces,
        Self::Monitoring,
        Self::NetworkDoctor,
        Self::Settings,
        Self::Diagnostics,
        Self::Logs,
        Self::Plugins,
        Self::Help,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Browser => "Workspace",
            Self::Messages => "Messages",
            Self::Directory => "Directory",
            Self::Identities => "Identities",
            Self::Interfaces => "Interfaces",
            Self::Monitoring => "Monitoring",
            Self::NetworkDoctor => "Network Doctor",
            Self::Settings => "Settings",
            Self::Diagnostics => "Diagnostics",
            Self::Logs => "Logs",
            Self::Plugins => "Plugins",
            Self::Help => "Help",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusArea {
    Sidebar,
    Workspace,
    Command,
}

impl FocusArea {
    pub fn next(self) -> Self {
        match self {
            Self::Sidebar => Self::Workspace,
            Self::Workspace => Self::Command,
            Self::Command => Self::Sidebar,
        }
    }
}
