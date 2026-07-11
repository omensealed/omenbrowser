use std::io::Read;
use std::path::Path;

use super::super::{OMENCHAT_GIF_ANIMATION_SCAN_BYTES, OMENCHAT_INLINE_MEDIA_HEADER_BYTES};

pub(in crate::desktop) fn read_media_header_bytes(
    path: &Path,
    max_bytes: usize,
) -> std::io::Result<Vec<u8>> {
    let mut file = std::fs::File::open(path)?;
    let mut bytes = vec![0; max_bytes.max(1)];
    let read = file.read(&mut bytes)?;
    bytes.truncate(read);
    Ok(bytes)
}

pub(in crate::desktop) fn scale_media_dimensions(
    width: u32,
    height: u32,
    max_width: f32,
    max_height: f32,
) -> (f32, f32) {
    let width = width.max(1) as f32;
    let height = height.max(1) as f32;
    let scale = (max_width / width).min(max_height / height).min(1.0);
    ((width * scale).max(1.0), (height * scale).max(1.0))
}

pub(in crate::desktop) fn image_dimensions_from_bytes(bytes: &[u8]) -> Option<(u32, u32)> {
    png_dimensions(bytes)
        .or_else(|| gif_dimensions(bytes))
        .or_else(|| jpeg_dimensions(bytes))
}

fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") || &bytes[12..16] != b"IHDR" {
        return None;
    }
    Some((
        u32::from_be_bytes(bytes[16..20].try_into().ok()?),
        u32::from_be_bytes(bytes[20..24].try_into().ok()?),
    ))
}

fn gif_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 10 || (!bytes.starts_with(b"GIF87a") && !bytes.starts_with(b"GIF89a")) {
        return None;
    }
    Some((
        u16::from_le_bytes(bytes[6..8].try_into().ok()?) as u32,
        u16::from_le_bytes(bytes[8..10].try_into().ok()?) as u32,
    ))
}

fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut offset = 2usize;
    while offset + 4 <= bytes.len() {
        while offset < bytes.len() && bytes[offset] != 0xFF {
            offset += 1;
        }
        while offset < bytes.len() && bytes[offset] == 0xFF {
            offset += 1;
        }
        if offset >= bytes.len() {
            return None;
        }
        let marker = bytes[offset];
        offset += 1;
        if matches!(marker, 0xD8 | 0xD9) {
            continue;
        }
        if offset + 2 > bytes.len() {
            return None;
        }
        let segment_len = u16::from_be_bytes(bytes[offset..offset + 2].try_into().ok()?) as usize;
        if segment_len < 2 || offset + segment_len > bytes.len() {
            return None;
        }
        if matches!(
            marker,
            0xC0 | 0xC1
                | 0xC2
                | 0xC3
                | 0xC5
                | 0xC6
                | 0xC7
                | 0xC9
                | 0xCA
                | 0xCB
                | 0xCD
                | 0xCE
                | 0xCF
        ) {
            if segment_len < 7 {
                return None;
            }
            let height = u16::from_be_bytes(bytes[offset + 3..offset + 5].try_into().ok()?) as u32;
            let width = u16::from_be_bytes(bytes[offset + 5..offset + 7].try_into().ok()?) as u32;
            return Some((width, height));
        }
        offset += segment_len;
    }
    None
}

pub(in crate::desktop) fn cached_media_is_animated_gif(path: &Path, content_type: &str) -> bool {
    let content_type_is_gif = content_type.eq_ignore_ascii_case("image/gif");
    let extension_is_gif = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gif"));
    if !content_type_is_gif && !extension_is_gif {
        return false;
    }
    read_media_header_bytes(path, OMENCHAT_GIF_ANIMATION_SCAN_BYTES)
        .map(|bytes| gif_image_descriptor_count(&bytes, 2) > 1)
        .unwrap_or(false)
}

pub(in crate::desktop) fn gif_image_descriptor_count(bytes: &[u8], stop_after: usize) -> usize {
    if stop_after == 0 || bytes.len() < 13 {
        return 0;
    }
    if !bytes.starts_with(b"GIF87a") && !bytes.starts_with(b"GIF89a") {
        return 0;
    }

    let packed = bytes[10];
    let global_color_table_len = if packed & 0b1000_0000 != 0 {
        3usize.saturating_mul(1usize << (((packed & 0b0000_0111) as usize) + 1))
    } else {
        0
    };
    let mut offset = 13usize.saturating_add(global_color_table_len);
    let mut frames = 0usize;

    while offset < bytes.len() {
        match bytes[offset] {
            0x2C => {
                frames = frames.saturating_add(1);
                if frames >= stop_after {
                    return frames;
                }
                if offset + 10 > bytes.len() {
                    return frames;
                }
                let image_packed = bytes[offset + 9];
                offset += 10;
                if image_packed & 0b1000_0000 != 0 {
                    let local_color_table_len = 3usize
                        .saturating_mul(1usize << (((image_packed & 0b0000_0111) as usize) + 1));
                    offset = offset.saturating_add(local_color_table_len);
                }
                if offset >= bytes.len() {
                    return frames;
                }
                offset += 1;
                offset = skip_gif_sub_blocks(bytes, offset);
            }
            0x21 => {
                if offset + 2 > bytes.len() {
                    return frames;
                }
                offset = skip_gif_sub_blocks(bytes, offset + 2);
            }
            0x3B => return frames,
            _ => return frames,
        }
    }

    frames
}

fn skip_gif_sub_blocks(bytes: &[u8], mut offset: usize) -> usize {
    while offset < bytes.len() {
        let block_len = bytes[offset] as usize;
        offset += 1;
        if block_len == 0 {
            break;
        }
        offset = offset.saturating_add(block_len);
        if offset > bytes.len() {
            return bytes.len();
        }
    }
    offset
}

pub(in crate::desktop) fn inline_media_size(
    path: &Path,
    max_width: f32,
    max_height: f32,
) -> Option<(f32, f32)> {
    let bytes = read_media_header_bytes(path, OMENCHAT_INLINE_MEDIA_HEADER_BYTES).ok()?;
    let (width, height) = image_dimensions_from_bytes(&bytes)?;
    Some(scale_media_dimensions(width, height, max_width, max_height))
}
