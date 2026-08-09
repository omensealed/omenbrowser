use sha2::{Digest, Sha256};

const DOMAIN: &[u8] = b"omenchat-nickname-colour-v1\0";
pub const NICKNAME_CONTRAST_TARGET: f64 = 4.5;
const CONTRAST_SEARCH_STEPS: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplayNicknameColour {
    pub rgb: [u8; 3],
    pub used_foreground_fallback: bool,
}

pub fn automatic_nickname_colour(server_id: &str, user_id: u32) -> [u8; 3] {
    let mut hasher = Sha256::new();
    hasher.update(DOMAIN);
    hasher.update((server_id.len() as u64).to_be_bytes());
    hasher.update(server_id.as_bytes());
    hasher.update(user_id.to_be_bytes());
    let digest = hasher.finalize();
    let hue = u16::from_be_bytes([digest[0], digest[1]]) as f64 / 65_536.0;
    let saturation = 0.58 + (f64::from(digest[2]) / 255.0) * 0.18;
    let lightness = 0.50 + (f64::from(digest[3]) / 255.0) * 0.12;
    hsl_to_srgb(hue, saturation, lightness).unwrap_or([0xff, 0xff, 0xff])
}

pub fn readable_nickname_colour(
    requested: [u8; 3],
    background: [u8; 3],
    theme_foreground: [u8; 3],
) -> DisplayNicknameColour {
    if contrast_ratio(requested, background).is_some_and(|ratio| ratio >= NICKNAME_CONTRAST_TARGET)
    {
        return DisplayNicknameColour {
            rgb: requested,
            used_foreground_fallback: false,
        };
    }
    let black = [0, 0, 0];
    let white = [255, 255, 255];
    let endpoint = if contrast_ratio(black, background).unwrap_or(0.0)
        >= contrast_ratio(white, background).unwrap_or(0.0)
    {
        black
    } else {
        white
    };
    let mut passing = endpoint;
    let mut failing = requested;
    for _ in 0..CONTRAST_SEARCH_STEPS {
        let candidate = [
            midpoint(failing[0], passing[0]),
            midpoint(failing[1], passing[1]),
            midpoint(failing[2], passing[2]),
        ];
        if contrast_ratio(candidate, background)
            .is_some_and(|ratio| ratio >= NICKNAME_CONTRAST_TARGET)
        {
            passing = candidate;
        } else {
            failing = candidate;
        }
    }
    if contrast_ratio(passing, background).is_some_and(|ratio| ratio >= NICKNAME_CONTRAST_TARGET) {
        DisplayNicknameColour {
            rgb: passing,
            used_foreground_fallback: false,
        }
    } else {
        DisplayNicknameColour {
            rgb: theme_foreground,
            used_foreground_fallback: true,
        }
    }
}

pub fn parse_rgb_hex(value: &str) -> Result<[u8; 3], &'static str> {
    let value = value.trim();
    let hex = value
        .strip_prefix('#')
        .ok_or("nickname colour must use #RRGGBB")?;
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("nickname colour must use exactly six hexadecimal digits");
    }
    let packed = u32::from_str_radix(hex, 16).map_err(|_| "nickname colour is invalid")?;
    Ok([
        ((packed >> 16) & 0xff) as u8,
        ((packed >> 8) & 0xff) as u8,
        (packed & 0xff) as u8,
    ])
}

pub fn contrast_ratio(foreground: [u8; 3], background: [u8; 3]) -> Option<f64> {
    let foreground = relative_luminance(foreground)?;
    let background = relative_luminance(background)?;
    let (lighter, darker) = if foreground >= background {
        (foreground, background)
    } else {
        (background, foreground)
    };
    Some((lighter + 0.05) / (darker + 0.05))
}

fn relative_luminance(rgb: [u8; 3]) -> Option<f64> {
    let linear = rgb.map(|channel| {
        let channel = f64::from(channel) / 255.0;
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    });
    let luminance = 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2];
    luminance.is_finite().then_some(luminance)
}

fn hsl_to_srgb(hue: f64, saturation: f64, lightness: f64) -> Option<[u8; 3]> {
    if !hue.is_finite() || !saturation.is_finite() || !lightness.is_finite() {
        return None;
    }
    let hue = hue.rem_euclid(1.0);
    let saturation = saturation.clamp(0.0, 1.0);
    let lightness = lightness.clamp(0.0, 1.0);
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let section = hue * 6.0;
    let x = chroma * (1.0 - (section.rem_euclid(2.0) - 1.0).abs());
    let (red, green, blue) = match section.floor() as u8 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let offset = lightness - chroma / 2.0;
    Some(
        [red, green, blue]
            .map(|channel| ((channel + offset).clamp(0.0, 1.0) * 255.0).round() as u8),
    )
}

fn midpoint(left: u8, right: u8) -> u8 {
    ((u16::from(left) + u16::from(right)) / 2) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_colour_is_stable_and_scoped_to_server_and_user() {
        let first = automatic_nickname_colour("server-a", 1);
        assert_eq!(first, automatic_nickname_colour("server-a", 1));
        assert_ne!(first, automatic_nickname_colour("server-a", 2));
        assert_ne!(first, automatic_nickname_colour("server-b", 1));
    }

    #[test]
    fn contrast_correction_meets_wcag_target_on_dark_and_light_surfaces() {
        for background in [[0, 0, 0], [8, 10, 18], [245, 245, 245], [255, 255, 255]] {
            for requested in [[0, 0, 0], [255, 255, 255], [80, 80, 80], [255, 0, 80]] {
                let result = readable_nickname_colour(requested, background, [230, 230, 230]);
                assert!(
                    result.used_foreground_fallback
                        || contrast_ratio(result.rgb, background).unwrap()
                            >= NICKNAME_CONTRAST_TARGET
                );
            }
        }
    }

    #[test]
    fn hex_parser_is_exact_and_bounded() {
        assert_eq!(parse_rgb_hex("#000000"), Ok([0, 0, 0]));
        assert_eq!(parse_rgb_hex("#FFFFFF"), Ok([255, 255, 255]));
        assert!(parse_rgb_hex("ffffff").is_err());
        assert!(parse_rgb_hex("#ffff").is_err());
        assert!(parse_rgb_hex("#gggggg").is_err());
    }
}
