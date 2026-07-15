use super::model::{
    bounded_chat_text, chat_text_fits, CHAT_ROOM_NAME_MAX_BYTES, CHAT_SERVER_DESTINATION_MAX_BYTES,
    CHAT_SERVER_DISPLAY_MAX_BYTES,
};
use super::protocol::PROTOCOL_NAME;

pub const OMENCHAT_DESCRIPTOR_MAX_BYTES: usize = 64 * 1024;
pub const OMENCHAT_DESCRIPTOR_MAX_LINES: usize = 128;
pub const OMENCHAT_DESCRIPTOR_LINE_MAX_BYTES: usize = 32 * 1024;
pub const OMENCHAT_DESCRIPTOR_PATH_MAX_BYTES: usize = 4 * 1024;
pub const OMENCHAT_DESCRIPTOR_THEME_MAX_BYTES: usize = 256;
pub const OMENCHAT_DESCRIPTOR_SIGNATURE_MAX_BYTES: usize = 16 * 1024;
pub const OMENCHAT_DESCRIPTOR_MAX_ROOM_HINTS: usize = 64;
pub const OMENCHAT_DESCRIPTOR_MAX_CAPABILITIES: usize = 64;
pub const OMENCHAT_DESCRIPTOR_CAPABILITY_MAX_BYTES: usize = 128;
pub const OMENCHAT_DESCRIPTOR_CAPABILITIES_MAX_BYTES: usize = 8 * 1024;
pub const OMENCHAT_LINK_MAX_FIELDS: usize = 32;
pub const OMENCHAT_LINK_FIELDS_MAX_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OmenChatDescriptor {
    pub server_destination: String,
    pub server_lxmf_destination: Option<String>,
    pub display_name: Option<String>,
    pub descriptor_path: Option<String>,
    pub theme_hint: Option<String>,
    pub rooms_hint: Vec<String>,
    pub capabilities: Vec<String>,
    pub local_display_name: Option<String>,
    pub descriptor_revision: Option<u64>,
    pub signature: Option<String>,
}

impl OmenChatDescriptor {
    pub fn from_omenchat_link(link: &str) -> Option<Self> {
        let destination = normalize_omenchat_link_destination(link)?;
        if destination.is_empty() || !chat_text_fits(destination, CHAT_SERVER_DESTINATION_MAX_BYTES)
        {
            return None;
        }
        Some(Self {
            server_destination: destination.to_owned(),
            ..Self::default()
        })
    }

    pub fn from_block(block: &str) -> Option<Self> {
        if block.len() > OMENCHAT_DESCRIPTOR_MAX_BYTES {
            return None;
        }
        let mut descriptor = Self::default();
        let mut in_block = false;
        for (line_index, raw_line) in block.lines().enumerate() {
            if line_index >= OMENCHAT_DESCRIPTOR_MAX_LINES
                || raw_line.len() > OMENCHAT_DESCRIPTOR_LINE_MAX_BYTES
            {
                return None;
            }
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }
            if line == "[omenchat]" {
                in_block = true;
                continue;
            }
            if !in_block {
                continue;
            }
            if line.starts_with('[') {
                break;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = unquote(value.trim());
            match key {
                "server" => {
                    if !chat_text_fits(value, CHAT_SERVER_DESTINATION_MAX_BYTES) {
                        return None;
                    }
                    descriptor.server_destination = value.to_owned();
                }
                "lxmf" => {
                    descriptor.server_lxmf_destination =
                        exact_optional(value, CHAT_SERVER_DESTINATION_MAX_BYTES)?;
                }
                "name" => {
                    descriptor.display_name =
                        display_optional(value, CHAT_SERVER_DISPLAY_MAX_BYTES);
                }
                "descriptor" => {
                    descriptor.descriptor_path =
                        exact_optional(value, OMENCHAT_DESCRIPTOR_PATH_MAX_BYTES)?;
                }
                "theme" => {
                    descriptor.theme_hint =
                        exact_optional(value, OMENCHAT_DESCRIPTOR_THEME_MAX_BYTES)?;
                }
                "rooms_hint" => {
                    descriptor.rooms_hint = parse_bounded_list(
                        value,
                        OMENCHAT_DESCRIPTOR_MAX_ROOM_HINTS,
                        CHAT_ROOM_NAME_MAX_BYTES,
                        OMENCHAT_DESCRIPTOR_MAX_ROOM_HINTS * CHAT_ROOM_NAME_MAX_BYTES,
                    )?;
                }
                "capabilities" => {
                    descriptor.capabilities = parse_bounded_list(
                        value,
                        OMENCHAT_DESCRIPTOR_MAX_CAPABILITIES,
                        OMENCHAT_DESCRIPTOR_CAPABILITY_MAX_BYTES,
                        OMENCHAT_DESCRIPTOR_CAPABILITIES_MAX_BYTES,
                    )?;
                }
                "descriptor_revision" => descriptor.descriptor_revision = value.parse().ok(),
                "signature" => {
                    descriptor.signature =
                        exact_optional(value, OMENCHAT_DESCRIPTOR_SIGNATURE_MAX_BYTES)?;
                }
                "protocol" if value != PROTOCOL_NAME => return None,
                _ => {}
            }
        }
        if descriptor.server_destination.is_empty() {
            None
        } else {
            Some(descriptor)
        }
    }

    pub fn apply_link_fields(&mut self, fields: &[String]) -> bool {
        if fields.len() > OMENCHAT_LINK_MAX_FIELDS
            || fields
                .iter()
                .try_fold(0usize, |total, field| total.checked_add(field.len()))
                .is_none_or(|total| total > OMENCHAT_LINK_FIELDS_MAX_BYTES)
        {
            return false;
        }

        let mut display_name = None;
        let mut lxmf = None;
        let mut theme = None;
        let mut rooms = None;
        for field in fields {
            let Some((key, value)) = field.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "name" | "display_name" => {
                    if value.is_empty() {
                        continue;
                    }
                    display_name = Some(display_optional(value, CHAT_SERVER_DISPLAY_MAX_BYTES));
                }
                "lxmf" => {
                    if value.is_empty() {
                        continue;
                    }
                    match exact_optional(value, CHAT_SERVER_DESTINATION_MAX_BYTES) {
                        Some(value) => lxmf = Some(value),
                        None => return false,
                    }
                }
                "theme" => {
                    if value.is_empty() {
                        continue;
                    }
                    match exact_optional(value, OMENCHAT_DESCRIPTOR_THEME_MAX_BYTES) {
                        Some(value) => theme = Some(value),
                        None => return false,
                    }
                }
                "rooms" | "rooms_hint" => {
                    let Some(value) = parse_bounded_list(
                        value,
                        OMENCHAT_DESCRIPTOR_MAX_ROOM_HINTS,
                        CHAT_ROOM_NAME_MAX_BYTES,
                        OMENCHAT_DESCRIPTOR_MAX_ROOM_HINTS * CHAT_ROOM_NAME_MAX_BYTES,
                    ) else {
                        return false;
                    };
                    rooms = Some(value);
                }
                _ => {}
            }
        }
        if let Some(value) = display_name {
            self.display_name = value;
        }
        if let Some(value) = lxmf {
            self.server_lxmf_destination = value;
        }
        if let Some(value) = theme {
            self.theme_hint = value;
        }
        if let Some(value) = rooms {
            self.rooms_hint = value;
        }
        true
    }
}

pub fn lower_omenchat_blocks(markup: &str) -> String {
    let lines = markup.lines().collect::<Vec<_>>();
    let mut output = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if lines[index].trim() != "[omenchat]" {
            output.push(lines[index].to_owned());
            index += 1;
            continue;
        }

        let start = index;
        index += 1;
        while index < lines.len() {
            let line = lines[index].trim();
            if line.starts_with('[') {
                break;
            }
            if line.is_empty() {
                index += 1;
                break;
            }
            index += 1;
        }

        let block_len = lines[start..index].iter().try_fold(0usize, |total, line| {
            total.checked_add(line.len().saturating_add(1))
        });
        if block_len.is_none_or(|len| len > OMENCHAT_DESCRIPTOR_MAX_BYTES) {
            output.extend(lines[start..index].iter().map(|line| (*line).to_owned()));
            continue;
        }
        let block = lines[start..index].join("\n");
        let Some(descriptor) = OmenChatDescriptor::from_block(&block) else {
            output.extend(lines[start..index].iter().map(|line| (*line).to_owned()));
            continue;
        };
        output.extend(descriptor.to_micron_lines());
    }

    if markup.ends_with('\n') {
        output.join("\n") + "\n"
    } else {
        output.join("\n")
    }
}

impl OmenChatDescriptor {
    pub fn to_micron_lines(&self) -> Vec<String> {
        let label = self
            .display_name
            .as_deref()
            .unwrap_or("Open OMENchat")
            .replace('`', "'");
        let target = format!("omenchat://{}", self.server_destination.replace('`', ""));
        let fields = self.link_fields();
        let link = if fields.is_empty() {
            format!("`[{label}`{target}]")
        } else {
            format!("`[{label}`{target}`{}]", fields.join("|"))
        };
        vec![link]
    }

    fn link_fields(&self) -> Vec<String> {
        let mut fields = Vec::new();
        if let Some(name) = self
            .display_name
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            fields.push(format!("name={}", sanitize_link_field(name)));
        }
        if let Some(lxmf) = self
            .server_lxmf_destination
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            fields.push(format!("lxmf={}", sanitize_link_field(lxmf)));
        }
        if let Some(theme) = self.theme_hint.as_deref().filter(|value| !value.is_empty()) {
            fields.push(format!("theme={}", sanitize_link_field(theme)));
        }
        if !self.rooms_hint.is_empty() {
            fields.push(format!(
                "rooms_hint={}",
                sanitize_link_field(&self.rooms_hint.join(","))
            ));
        }
        fields
    }
}

fn normalize_omenchat_link_destination(link: &str) -> Option<&str> {
    let trimmed = link.trim();
    let destination = trimmed
        .strip_prefix("omenchat://")
        .or_else(|| trimmed.strip_prefix("omenchat:"))?
        .trim()
        .trim_start_matches('/');
    Some(destination)
}

fn sanitize_link_field(value: &str) -> String {
    value.replace(['`', '|'], " ").trim().to_owned()
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
}

fn display_optional(value: &str, max_bytes: usize) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(bounded_chat_text(value, max_bytes))
    }
}

fn exact_optional(value: &str, max_bytes: usize) -> Option<Option<String>> {
    if value.trim().is_empty() {
        Some(None)
    } else if chat_text_fits(value, max_bytes) {
        Some(Some(value.to_owned()))
    } else {
        None
    }
}

fn parse_bounded_list(
    value: &str,
    max_items: usize,
    max_item_bytes: usize,
    max_total_bytes: usize,
) -> Option<Vec<String>> {
    let mut output = Vec::new();
    let mut total_bytes = 0usize;
    for item in value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        if output.len() >= max_items || !chat_text_fits(item, max_item_bytes) {
            return None;
        }
        total_bytes = total_bytes.checked_add(item.len())?;
        if total_bytes > max_total_bytes {
            return None;
        }
        output.push(item.to_owned());
    }
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_omenchat_link() {
        let descriptor =
            OmenChatDescriptor::from_omenchat_link("omenchat://abcd1234").expect("descriptor");
        assert_eq!(descriptor.server_destination, "abcd1234");

        let descriptor =
            OmenChatDescriptor::from_omenchat_link("omenchat:abcd1234").expect("descriptor");
        assert_eq!(descriptor.server_destination, "abcd1234");
    }

    #[test]
    fn parses_declarative_block() {
        let descriptor = OmenChatDescriptor::from_block(
            r#"
[omenchat]
server = "abcd1234"
lxmf = "lxmf5678"
name = "Node Chat"
descriptor = "/omenchat/descriptor"
theme = "field-terminal"
rooms_hint = "lobby, radio, support"
capabilities = "history-v1,bzip2"
descriptor_revision = 2
"#,
        )
        .expect("descriptor");

        assert_eq!(descriptor.server_destination, "abcd1234");
        assert_eq!(
            descriptor.server_lxmf_destination.as_deref(),
            Some("lxmf5678")
        );
        assert_eq!(descriptor.display_name.as_deref(), Some("Node Chat"));
        assert_eq!(descriptor.rooms_hint, vec!["lobby", "radio", "support"]);
        assert_eq!(descriptor.capabilities, vec!["history-v1", "bzip2"]);
        assert_eq!(descriptor.descriptor_revision, Some(2));
    }

    #[test]
    fn lowers_omenchat_block_to_clickable_link() {
        let lowered = lower_omenchat_blocks(
            r#">Node
[omenchat]
server = "abcd1234"
name = "Node Chat"
rooms_hint = "lobby,radio"

after"#,
        );

        assert!(lowered.contains("`[Node Chat`omenchat://abcd1234`"));
        assert!(lowered.contains("name=Node Chat"));
        assert!(lowered.contains("rooms_hint=lobby,radio"));
        assert!(lowered.contains(">Node"));
        assert!(lowered.contains("after"));
    }

    #[test]
    fn leaves_invalid_omenchat_block_untouched() {
        let markup = r#"[omenchat]
name = "No Server"

after"#;

        let lowered = lower_omenchat_blocks(markup);

        assert!(lowered.contains("[omenchat]"));
        assert!(lowered.contains("No Server"));
        assert!(lowered.contains("after"));
    }

    #[test]
    fn sanitizes_lowered_omenchat_link_fields() {
        let lowered = lower_omenchat_blocks(
            r#"[omenchat]
server = "abcd1234"
name = "Node`Chat|Main"
theme = "dark|red"
"#,
        );

        assert!(lowered.contains("`[Node'Chat|Main`omenchat://abcd1234`"));
        assert!(lowered.contains("name=Node Chat Main"));
        assert!(lowered.contains("theme=dark red"));
    }

    #[test]
    fn descriptor_parser_bounds_exact_fields_and_collections() {
        assert!(OmenChatDescriptor::from_omenchat_link(&format!(
            "omenchat://{}",
            "d".repeat(CHAT_SERVER_DESTINATION_MAX_BYTES + 1)
        ))
        .is_none());

        for block in [
            format!(
                "[omenchat]\nserver={}\n",
                "d".repeat(CHAT_SERVER_DESTINATION_MAX_BYTES + 1)
            ),
            format!(
                "[omenchat]\nserver=dest\nrooms_hint={}\n",
                "r".repeat(CHAT_ROOM_NAME_MAX_BYTES + 1)
            ),
            format!(
                "[omenchat]\nserver=dest\nrooms_hint={}\n",
                (0..=OMENCHAT_DESCRIPTOR_MAX_ROOM_HINTS)
                    .map(|index| format!("r{index}"))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            format!(
                "[omenchat]\nserver=dest\ncapabilities={}\n",
                "c".repeat(OMENCHAT_DESCRIPTOR_CAPABILITY_MAX_BYTES + 1)
            ),
            format!(
                "[omenchat]\nserver=dest\nsignature={}\n",
                "s".repeat(OMENCHAT_DESCRIPTOR_SIGNATURE_MAX_BYTES + 1)
            ),
        ] {
            assert!(OmenChatDescriptor::from_block(&block).is_none());
        }
    }

    #[test]
    fn descriptor_display_and_link_field_bounds_are_utf8_safe_and_atomic() {
        let descriptor = OmenChatDescriptor::from_block(&format!(
            "[omenchat]\nserver=dest\nname={}\n",
            "☃".repeat(CHAT_SERVER_DISPLAY_MAX_BYTES)
        ))
        .expect("bounded display descriptor");
        assert!(
            descriptor
                .display_name
                .as_deref()
                .is_some_and(
                    |name| name.len() <= CHAT_SERVER_DISPLAY_MAX_BYTES && name.ends_with('…')
                )
        );

        let mut descriptor =
            OmenChatDescriptor::from_omenchat_link("omenchat://dest").expect("link descriptor");
        descriptor.display_name = Some("original".into());
        descriptor.rooms_hint = vec!["lobby".into()];
        assert!(!descriptor.apply_link_fields(&[
            "name=replacement".into(),
            format!("rooms={}", "r".repeat(CHAT_ROOM_NAME_MAX_BYTES + 1)),
        ]));
        assert_eq!(descriptor.display_name.as_deref(), Some("original"));
        assert_eq!(descriptor.rooms_hint, vec!["lobby"]);

        assert!(descriptor.apply_link_fields(&[
            format!("name={}", "☃".repeat(CHAT_SERVER_DISPLAY_MAX_BYTES)),
            "rooms=ops,radio".into(),
        ]));
        assert!(descriptor
            .display_name
            .as_deref()
            .is_some_and(|name| name.len() <= CHAT_SERVER_DISPLAY_MAX_BYTES));
        assert_eq!(descriptor.rooms_hint, vec!["ops", "radio"]);
    }

    #[test]
    fn oversized_descriptor_block_is_not_joined_or_lowered() {
        let block = format!(
            "[omenchat]\nserver=dest\nunknown={}\n",
            "x".repeat(OMENCHAT_DESCRIPTOR_MAX_BYTES)
        );
        assert!(OmenChatDescriptor::from_block(&block).is_none());
        assert_eq!(lower_omenchat_blocks(&block), block);
    }
}
