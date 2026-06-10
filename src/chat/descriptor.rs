use super::protocol::PROTOCOL_NAME;

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
        if destination.is_empty() {
            return None;
        }
        Some(Self {
            server_destination: destination,
            ..Self::default()
        })
    }

    pub fn from_block(block: &str) -> Option<Self> {
        let mut descriptor = Self::default();
        let mut in_block = false;
        for raw_line in block.lines() {
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
                "server" => descriptor.server_destination = value,
                "lxmf" => descriptor.server_lxmf_destination = empty_to_none(value),
                "name" => descriptor.display_name = empty_to_none(value),
                "descriptor" => descriptor.descriptor_path = empty_to_none(value),
                "theme" => descriptor.theme_hint = empty_to_none(value),
                "rooms_hint" => {
                    descriptor.rooms_hint = value
                        .split(',')
                        .map(str::trim)
                        .filter(|room| !room.is_empty())
                        .map(ToOwned::to_owned)
                        .collect();
                }
                "capabilities" => {
                    descriptor.capabilities = value
                        .split(',')
                        .map(str::trim)
                        .filter(|capability| !capability.is_empty())
                        .map(ToOwned::to_owned)
                        .collect();
                }
                "descriptor_revision" => descriptor.descriptor_revision = value.parse().ok(),
                "signature" => descriptor.signature = empty_to_none(value),
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

fn normalize_omenchat_link_destination(link: &str) -> Option<String> {
    let trimmed = link.trim();
    let destination = trimmed
        .strip_prefix("omenchat://")
        .or_else(|| trimmed.strip_prefix("omenchat:"))?
        .trim()
        .trim_start_matches('/');
    Some(destination.to_owned())
}

fn sanitize_link_field(value: &str) -> String {
    value.replace(['`', '|'], " ").trim().to_owned()
}

fn unquote(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
        .to_owned()
}

fn empty_to_none(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
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
}
