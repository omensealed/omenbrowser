use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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
    let descriptor = raw.split('}').next().unwrap_or(raw);
    let parts = descriptor.split('`').collect::<Vec<_>>();
    let target = parts.first().copied().unwrap_or_default().to_string();
    let refresh_secs = parts
        .get(1)
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<f64>().ok())
        .map(|value| value.max(1.0));
    let mut fields = Vec::new();
    let mut id = None;
    let mut loop_count = None;
    if let Some(raw_fields) = parts.get(2) {
        for field in raw_fields.split('|').filter(|field| !field.is_empty()) {
            if let Some(value) = field.strip_prefix("pid=") {
                id = Some(value.to_string());
            } else if let Some(value) = field.strip_prefix("loop=") {
                loop_count = value.parse::<i64>().ok().map(|count| count.max(0) as u32);
            } else {
                fields.push(field.to_string());
            }
        }
    }
    PartialDescriptor {
        target,
        refresh_secs,
        fields,
        id,
        loop_count,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PartialDescriptor {
    pub target: String,
    pub refresh_secs: Option<f64>,
    pub fields: Vec<String>,
    pub id: Option<String>,
    pub loop_count: Option<u32>,
}

pub fn extract_partial_specs(markup: &str) -> Vec<PartialSpec> {
    markup
        .lines()
        .enumerate()
        .filter_map(|(line_index, line)| {
            let raw = line.strip_prefix("`{")?;
            let parsed = parse_partial_descriptor(raw);
            let slot = format!(
                "{}:{line_index}:{}",
                parsed.id.as_deref().unwrap_or("partial"),
                line_hash(line)
            );
            Some(PartialSpec {
                slot,
                line_index,
                target: parsed.target,
                refresh_secs: parsed.refresh_secs,
                fields: parsed.fields,
                id: parsed.id,
                loop_count: parsed.loop_count,
                remaining: parsed.loop_count,
            })
        })
        .collect()
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
