#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatTheme {
    pub name: String,
}

impl Default for ChatTheme {
    fn default() -> Self {
        Self {
            name: "field-terminal".into(),
        }
    }
}
