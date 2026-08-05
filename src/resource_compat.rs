//! Temporary exact-train Resource safety policy.
//!
//! `reticulum-rs-transport 0.9.7` strips metadata from every segment of a
//! split Resource even though only segment one carries metadata. See upstream
//! issue #553 and proposed fix #556. Remove this guard only after both Cargo
//! roots adopt an official fixed registry train and the interoperability
//! sentinel passes against that unmodified release.

pub(crate) const RETICULUM_0_9_7_MAX_EFFICIENT_RESOURCE_BYTES: usize = 1_048_575;
pub(crate) const RETICULUM_RESOURCE_METADATA_LENGTH_PREFIX_BYTES: usize = 3;
pub(crate) const OMENCHAT_RESOURCE_METADATA_PREFIX_BYTES: usize = b"omenchat-resource:".len();

pub(crate) fn maximum_upload_resource_id_bytes() -> usize {
    format!(
        "upload:{}:{}:{}:{:016x}",
        u32::MAX,
        u32::MAX,
        u32::MAX,
        u64::MAX
    )
    .len()
}

pub(crate) fn exact_train_upload_payload_max() -> usize {
    maximum_payload_for_metadata_len(
        OMENCHAT_RESOURCE_METADATA_PREFIX_BYTES + maximum_upload_resource_id_bytes(),
    )
    .expect("bounded OMENchat upload metadata fits Reticulum Resource boundary")
}

pub(crate) fn metadata_bearing_resource_is_unsplit_safe(
    payload_len: usize,
    metadata_len: usize,
) -> bool {
    RETICULUM_RESOURCE_METADATA_LENGTH_PREFIX_BYTES
        .checked_add(metadata_len)
        .and_then(|overhead| overhead.checked_add(payload_len))
        .is_some_and(|total| total <= RETICULUM_0_9_7_MAX_EFFICIENT_RESOURCE_BYTES)
}

pub(crate) fn maximum_payload_for_metadata_len(metadata_len: usize) -> Option<usize> {
    RETICULUM_0_9_7_MAX_EFFICIENT_RESOURCE_BYTES
        .checked_sub(RETICULUM_RESOURCE_METADATA_LENGTH_PREFIX_BYTES.checked_add(metadata_len)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_boundary_is_safe_and_plus_one_is_not() {
        let metadata_len = 37;
        let maximum = maximum_payload_for_metadata_len(metadata_len).expect("metadata fits");
        assert!(metadata_bearing_resource_is_unsplit_safe(
            maximum,
            metadata_len
        ));
        assert!(!metadata_bearing_resource_is_unsplit_safe(
            maximum + 1,
            metadata_len
        ));
        assert_eq!(
            RETICULUM_RESOURCE_METADATA_LENGTH_PREFIX_BYTES + metadata_len + maximum,
            RETICULUM_0_9_7_MAX_EFFICIENT_RESOURCE_BYTES
        );
    }

    #[test]
    fn overflow_and_oversized_metadata_fail_closed() {
        assert!(!metadata_bearing_resource_is_unsplit_safe(
            usize::MAX,
            usize::MAX
        ));
        assert_eq!(maximum_payload_for_metadata_len(usize::MAX), None);
        assert_eq!(
            maximum_payload_for_metadata_len(RETICULUM_0_9_7_MAX_EFFICIENT_RESOURCE_BYTES),
            None
        );
    }

    #[test]
    fn upload_ceiling_uses_the_actual_bounded_resource_id_shape() {
        assert_eq!(maximum_upload_resource_id_bytes(), 56);
        assert_eq!(
            format!(
                "upload:{}:{}:{}:{:016x}",
                u32::MAX,
                u32::MAX,
                u32::MAX,
                u64::MAX
            )
            .len(),
            maximum_upload_resource_id_bytes()
        );
        let maximum = exact_train_upload_payload_max();
        assert!(metadata_bearing_resource_is_unsplit_safe(
            maximum,
            OMENCHAT_RESOURCE_METADATA_PREFIX_BYTES + maximum_upload_resource_id_bytes()
        ));
        assert!(!metadata_bearing_resource_is_unsplit_safe(
            maximum + 1,
            OMENCHAT_RESOURCE_METADATA_PREFIX_BYTES + maximum_upload_resource_id_bytes()
        ));
    }
}
