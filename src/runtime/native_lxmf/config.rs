#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeLxmfConfig {
    pub display_name: Option<String>,
    pub storage_namespace: String,
}

impl Default for NativeLxmfConfig {
    fn default() -> Self {
        Self {
            display_name: None,
            storage_namespace: "omenbrowser-rs".into(),
        }
    }
}
