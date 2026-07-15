use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::micron::parser::{
    MICRON_LINK_FIELDS_MAX_BYTES, MICRON_LINK_FIELD_MAX_BYTES, MICRON_LINK_MAX_FIELDS,
    MICRON_LINK_RAW_MAX_BYTES, MICRON_LINK_TARGET_MAX_BYTES,
};

pub const PARTIAL_ID_MAX_BYTES: usize = 256;
pub const PARTIAL_SPEC_MAX_ITEMS: usize = 256;
pub const PARTIAL_SPECS_MAX_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PartialSpec {
    pub slot: String,
    pub line_index: usize,
    pub target: String,
    pub refresh_secs: Option<f64>,
    pub fields: Vec<String>,
    pub id: Option<String>,
    pub loop_count: Option<u32>,
    pub remaining: Option<u32>,
}

pub fn parse_partial_descriptor(raw: &str) -> PartialDescriptor {
    try_parse_partial_descriptor(raw).unwrap_or_default()
}

pub fn try_parse_partial_descriptor(raw: &str) -> Option<PartialDescriptor> {
    let descriptor = raw.split('}').next().unwrap_or(raw);
    if descriptor.len() > MICRON_LINK_RAW_MAX_BYTES {
        return None;
    }
    let mut parts = descriptor.split('`');
    let target = parts.next().unwrap_or_default();
    if target.is_empty() || target.len() > MICRON_LINK_TARGET_MAX_BYTES {
        return None;
    }
    let refresh_secs = parts
        .next()
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<f64>().ok())
        .map(|value| value.max(1.0));
    let mut fields = Vec::new();
    let mut id = None;
    let mut loop_count = None;
    let mut field_items = 0usize;
    let mut field_bytes = 0usize;
    if let Some(raw_fields) = parts.next() {
        for field in raw_fields.split('|').filter(|field| !field.is_empty()) {
            if field_items >= MICRON_LINK_MAX_FIELDS || field.len() > MICRON_LINK_FIELD_MAX_BYTES {
                return None;
            }
            field_items += 1;
            field_bytes = field_bytes.checked_add(field.len())?;
            if field_bytes > MICRON_LINK_FIELDS_MAX_BYTES {
                return None;
            }
            if let Some(value) = field.strip_prefix("pid=") {
                if value.len() > PARTIAL_ID_MAX_BYTES {
                    return None;
                }
                id = Some(value.to_string());
            } else if let Some(value) = field.strip_prefix("loop=") {
                loop_count = value.parse::<i64>().ok().map(|count| count.max(0) as u32);
            } else {
                fields.push(field.to_string());
            }
        }
    }
    Some(PartialDescriptor {
        target: target.to_owned(),
        refresh_secs,
        fields,
        id,
        loop_count,
    })
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct PartialDescriptor {
    pub target: String,
    pub refresh_secs: Option<f64>,
    pub fields: Vec<String>,
    pub id: Option<String>,
    pub loop_count: Option<u32>,
}

pub fn extract_partial_specs(markup: &str) -> Vec<PartialSpec> {
    let mut specs = Vec::new();
    let mut retained_bytes = 0usize;
    for (line_index, line) in markup.lines().enumerate() {
        let Some(raw) = line.strip_prefix("`{") else {
            continue;
        };
        let Some(parsed) = try_parse_partial_descriptor(raw) else {
            continue;
        };
        if specs.len() >= PARTIAL_SPEC_MAX_ITEMS {
            break;
        }
        let Some(next_bytes) = retained_bytes.checked_add(raw.len()) else {
            break;
        };
        if next_bytes > PARTIAL_SPECS_MAX_BYTES {
            break;
        }
        retained_bytes = next_bytes;
        let slot = format!(
            "{}:{line_index}:{}",
            parsed.id.as_deref().unwrap_or("partial"),
            line_hash(line)
        );
        specs.push(PartialSpec {
            slot,
            line_index,
            target: parsed.target,
            refresh_secs: parsed.refresh_secs,
            fields: parsed.fields,
            id: parsed.id,
            loop_count: parsed.loop_count,
            remaining: parsed.loop_count,
        });
    }
    specs
}

pub fn compose_markup_with_partials(
    base_markup: &str,
    specs: &[PartialSpec],
    partial_contents: &BTreeMap<String, String>,
) -> String {
    if specs.is_empty() {
        return base_markup.into();
    }
    let specs_by_line = specs
        .iter()
        .map(|spec| (spec.line_index, spec))
        .collect::<BTreeMap<_, _>>();
    let mut output = Vec::new();
    for (line_index, line) in base_markup.lines().enumerate() {
        let Some(spec) = specs_by_line.get(&line_index) else {
            output.push(line.to_string());
            continue;
        };
        if let Some(content) = partial_contents.get(&spec.slot) {
            output.extend(
                strip_partial_document_headers(content)
                    .lines()
                    .map(str::to_string),
            );
        } else {
            output.push(format!(
                "[loading {}]",
                spec.id.as_deref().unwrap_or(&spec.target)
            ));
        }
    }
    output.join("\n")
}

pub fn strip_partial_document_headers(content: &str) -> &str {
    let Some(rest) = content.strip_prefix("#!") else {
        return content;
    };
    let Some((header, remainder)) = rest.split_once('\n') else {
        return if rest.starts_with("c=") { "" } else { content };
    };
    if header.starts_with("c=") {
        remainder
    } else {
        content
    }
}

fn line_hash(line: &str) -> String {
    let digest = Sha256::digest(line.as_bytes());
    format!("{digest:x}").chars().take(8).collect()
}
