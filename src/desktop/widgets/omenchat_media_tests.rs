use super::super::super::{OMENCHAT_GIF_ANIMATION_SCAN_BYTES, OMENCHAT_INLINE_MEDIA_HEADER_BYTES};
use super::*;

const FIXTURE_RETICULUM_HASH: &str = "00112233445566778899aabbccddeeff";

#[test]
fn omenchat_media_hints_offer_reticulum_load_without_clearweb_fetch() {
    let settings = crate::storage::settings::ClearwebPrivacySettings::default();
    let body = format!("pic {FIXTURE_RETICULUM_HASH}:/files/cat.png");
    let hints = omenchat_media_hints(&body, &settings, None, false, &HashMap::new());

    assert_eq!(hints.len(), 1);
    assert!(hints[0].label.contains("Reticulum/NomadNet"));
    assert!(hints[0].load_url.is_some());
    assert!(hints[0].open_url.is_none());
}

#[test]
fn omenchat_media_hints_offer_socks_load_when_remote_media_allowed() {
    let mut settings = crate::storage::settings::ClearwebPrivacySettings::default();
    settings.remote_media_enabled = true;
    let hints = omenchat_media_hints(
        "pic https://example.org/cat.png",
        &settings,
        Some(&("127.0.0.1".to_string(), 9150)),
        true,
        &HashMap::new(),
    );

    assert_eq!(hints.len(), 1);
    assert!(hints[0].label.contains("SOCKS5"));
    assert!(hints[0].open_url.is_none());
    assert!(hints[0].load_url.is_some());
}

#[test]
fn omenchat_media_hints_require_trust_for_clearweb_auto_load() {
    let mut settings = crate::storage::settings::ClearwebPrivacySettings::default();
    settings.remote_media_enabled = true;
    let hints = omenchat_media_hints(
        "pic https://example.org/cat.png",
        &settings,
        Some(&("127.0.0.1".to_string(), 9150)),
        false,
        &HashMap::new(),
    );

    assert_eq!(hints.len(), 1);
    assert!(hints[0].label.contains("untrusted OMENchat server"));
    assert!(hints[0].open_url.is_none());
    assert!(hints[0].load_url.is_some());
}

#[test]
fn omenchat_media_hints_keep_clearweb_images_explicit_without_remote_media() {
    let settings = crate::storage::settings::ClearwebPrivacySettings::default();
    let hints = omenchat_media_hints(
        "pic https://example.org/cat.png",
        &settings,
        Some(&("127.0.0.1".to_string(), 9150)),
        true,
        &HashMap::new(),
    );

    assert_eq!(hints.len(), 1);
    assert!(hints[0].label.contains("disabled"));
    assert!(hints[0].open_url.is_some());
    assert!(hints[0].load_url.is_none());
}

#[test]
fn omenchat_media_hints_report_cached_media() {
    let settings = crate::storage::settings::ClearwebPrivacySettings::default();
    let url = format!("{FIXTURE_RETICULUM_HASH}:/files/cat.png");
    let mut cache = HashMap::new();
    cache.insert(
        url.clone(),
        OmenChatMediaLoadState::Cached {
            path: "/tmp/cat.png".into(),
            content_type: "image/png".into(),
            animated: false,
        },
    );

    let hints = omenchat_media_hints(&url, &settings, None, false, &cache);
    assert_eq!(hints.len(), 1);
    assert!(hints[0].label.is_empty());
    assert!(hints[0].open_url.is_none());
    assert!(hints[0].open_path.is_none());
    assert!(hints[0].load_url.is_none());
    assert_eq!(hints[0].image_path.as_deref(), Some("/tmp/cat.png"));
    assert_eq!(
        hints[0].caption.as_deref(),
        Some("Reticulum/NomadNet image")
    );
}

#[test]
fn omenchat_media_hints_offer_cached_animated_gif_open_button() {
    let settings = crate::storage::settings::ClearwebPrivacySettings::default();
    let url = "https://example.test/loop.gif".to_string();
    let mut cache = HashMap::new();
    cache.insert(
        url.clone(),
        OmenChatMediaLoadState::Cached {
            path: "/tmp/loop.gif".into(),
            content_type: "image/gif".into(),
            animated: true,
        },
    );

    let hints = omenchat_media_hints(&url, &settings, None, false, &cache);
    assert_eq!(hints.len(), 1);
    assert!(hints[0].label.is_empty());
    assert!(hints[0].open_url.is_none());
    assert_eq!(hints[0].open_path.as_deref(), Some("/tmp/loop.gif"));
    assert!(hints[0].load_url.is_none());
    assert_eq!(hints[0].image_path.as_deref(), Some("/tmp/loop.gif"));
}

#[test]
fn omenchat_media_hints_caption_cached_clearweb_source() {
    let settings = crate::storage::settings::ClearwebPrivacySettings::default();
    let url = "https://cdn.example.test/images/cat.png".to_string();
    let mut cache = HashMap::new();
    cache.insert(
        url.clone(),
        OmenChatMediaLoadState::Cached {
            path: "/tmp/cat.png".into(),
            content_type: "image/png".into(),
            animated: false,
        },
    );

    let trusted = omenchat_media_hints(&url, &settings, None, true, &cache);
    let untrusted = omenchat_media_hints(&url, &settings, None, false, &cache);

    assert_eq!(
        trusted[0].caption.as_deref(),
        Some("trusted clearweb image from cdn.example.test")
    );
    assert_eq!(
        untrusted[0].caption.as_deref(),
        Some("manual clearweb image from cdn.example.test")
    );
}

#[test]
fn gif_image_descriptor_count_detects_multiple_frames() {
    let single_frame = [
        b"GIF89a".as_slice(),
        &[1, 0, 1, 0, 0, 0, 0],
        &[0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0],
        &[2, 1, 0, 0],
        &[0x3B],
    ]
    .concat();
    let animated = [
        b"GIF89a".as_slice(),
        &[1, 0, 1, 0, 0, 0, 0],
        &[0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0],
        &[2, 1, 0, 0],
        &[0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0],
        &[2, 1, 0, 0],
        &[0x3B],
    ]
    .concat();

    assert_eq!(gif_image_descriptor_count(&single_frame, 2), 1);
    assert_eq!(gif_image_descriptor_count(&animated, 2), 2);
}

#[test]
fn omenchat_media_dimensions_parse_common_headers() {
    let png = [
        b"\x89PNG\r\n\x1a\n".as_slice(),
        &[0, 0, 0, 13],
        b"IHDR".as_slice(),
        &640u32.to_be_bytes(),
        &480u32.to_be_bytes(),
        &[8, 6, 0, 0, 0],
    ]
    .concat();
    let gif = [b"GIF89a".as_slice(), &[44, 1, 200, 0, 0, 0, 0]].concat();
    let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 4, 0, 0, 0xFF, 0xC0, 0, 17, 8];
    jpeg.extend_from_slice(&720u16.to_be_bytes());
    jpeg.extend_from_slice(&1280u16.to_be_bytes());
    jpeg.extend_from_slice(&[3, 1, 0x11, 0, 2, 0x11, 0, 3, 0x11, 0]);

    assert_eq!(image_dimensions_from_bytes(&png), Some((640, 480)));
    assert_eq!(image_dimensions_from_bytes(&gif), Some((300, 200)));
    assert_eq!(image_dimensions_from_bytes(&jpeg), Some((1280, 720)));
}

#[test]
fn omenchat_media_dimensions_scale_large_images_without_upscaling_small_ones() {
    assert_eq!(
        scale_media_dimensions(1040, 720, 520.0, 360.0),
        (520.0, 360.0)
    );
    assert_eq!(
        scale_media_dimensions(200, 100, 520.0, 360.0),
        (200.0, 100.0)
    );
}

#[test]
fn omenchat_upload_state_label_keeps_attachment_status_compact() {
    assert_eq!(
        omenchat_upload_state_label(&OmenChatMediaLoadState::Loading {
            message: "requested upload from server".into(),
            received: None,
            total: None,
        }),
        "waiting for server"
    );
    assert_eq!(
        omenchat_upload_state_label(&OmenChatMediaLoadState::Loading {
            message: "receiving file".into(),
            received: Some(1536),
            total: Some(4096),
        }),
        "loading: 1.5 KiB / 4.0 KiB"
    );
    let label = omenchat_upload_state_label(&OmenChatMediaLoadState::Failed {
        message: "resource transfer failed because the server closed before the resource completed and the retry window expired".into(),
    });
    assert!(label.starts_with("failed: resource transfer failed"));
    assert!(label.ends_with("..."));
    assert!(label.len() < 90);
}

#[test]
fn omenchat_inline_media_size_reads_bounded_header_only() {
    let root = std::env::temp_dir().join(format!(
        "omenchat-media-header-{}",
        crate::app::current_epoch_ms()
    ));
    std::fs::create_dir_all(&root).expect("temp dir");
    let path = root.join("wide.png");
    let mut png = [
        b"\x89PNG\r\n\x1a\n".as_slice(),
        &[0, 0, 0, 13],
        b"IHDR".as_slice(),
        &1200u32.to_be_bytes(),
        &600u32.to_be_bytes(),
        &[8, 6, 0, 0, 0],
    ]
    .concat();
    png.extend(std::iter::repeat_n(
        0xAA,
        OMENCHAT_INLINE_MEDIA_HEADER_BYTES + 32,
    ));
    std::fs::write(&path, png).expect("write png");

    let bytes = read_media_header_bytes(&path, 32).expect("read header");
    assert_eq!(bytes.len(), 32);
    assert_eq!(image_dimensions_from_bytes(&bytes), Some((1200, 600)));
    assert_eq!(omenchat_inline_media_size(&path), Some((520.0, 260.0)));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn cached_media_is_animated_gif_uses_bounded_scan() {
    let root = std::env::temp_dir().join(format!(
        "omenchat-gif-scan-{}",
        crate::app::current_epoch_ms()
    ));
    std::fs::create_dir_all(&root).expect("temp dir");
    let path = root.join("animated.gif");
    let mut gif = [
        b"GIF89a".as_slice(),
        &[1, 0, 1, 0, 0, 0, 0],
        &[0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0],
        &[2, 1, 0, 0],
        &[0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0],
        &[2, 1, 0, 0],
        &[0x3B],
    ]
    .concat();
    gif.extend(std::iter::repeat_n(
        0xCC,
        OMENCHAT_GIF_ANIMATION_SCAN_BYTES + 1024,
    ));
    std::fs::write(&path, gif).expect("write gif");

    let bounded = read_media_header_bytes(&path, 32).expect("read bounded scan");
    assert_eq!(bounded.len(), 32);
    assert!(cached_media_is_animated_gif(
        &path,
        "application/octet-stream"
    ));
    let _ = std::fs::remove_dir_all(root);
}
