#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativePathRequest {
    pub destination_hash: String,
    pub reason: String,
    pub sibling_aspects: bool,
}

impl NativePathRequest {
    pub fn new(destination_hash: &str, reason: &str, sibling_aspects: bool) -> Self {
        Self {
            destination_hash: destination_hash.into(),
            reason: reason.into(),
            sibling_aspects,
        }
    }
}
