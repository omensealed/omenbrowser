use std::collections::BTreeMap;
use std::sync::Arc;

use crate::browser::partials::strip_partial_document_headers;
use crate::micron::parser::{
    collect_bounded_link_fields, MICRON_CONTROL_NAME_MAX_BYTES, MICRON_CONTROL_VALUE_MAX_BYTES,
    MICRON_LINK_FIELDS_MAX_BYTES, MICRON_LINK_FIELD_MAX_BYTES, MICRON_LINK_MAX_FIELDS,
    MICRON_LINK_TARGET_MAX_BYTES,
};
use crate::micron::render::{
    default_render_style, render_document, render_document_with_field_cursor, Cell, RenderedRow,
};
use crate::micron::{parse_micron, Alignment, TextStyle};
use serde::{Deserialize, Serialize};

const DEFAULT_COLUMN_GAP: &str = "   ";
const DEFAULT_CONTROL_WIDTH: usize = 24;
const MAX_EXPLICIT_CONTROL_WIDTH: usize = 96;
pub const MICRONPLUS_SOURCE_MAX_BYTES: usize = 4 * 1024 * 1024;
pub const MICRONPLUS_SOURCE_MAX_LINES: usize = 16 * 1024;
pub const MICRONPLUS_SOURCE_LINE_MAX_BYTES: usize = 256 * 1024;
pub const MICRONPLUS_TREE_MAX_DEPTH: usize = 32;
pub const MICRONPLUS_TREE_MAX_NODES: usize = 8 * 1024;
pub const MICRONPLUS_TREE_MAX_COLUMNS: usize = 512;
pub const MICRONPLUS_TREE_MAX_OWNED_BYTES: usize = 8 * 1024 * 1024;
pub const MICRONPLUS_LAYOUT_MAX_WINDOWS: usize = 64;
pub const MICRONPLUS_LAYOUT_MAX_GROUPS: usize = 256;
pub const MICRONPLUS_LAYOUT_MAX_COLUMNS: usize = 512;
pub const MICRONPLUS_LAYOUT_MAX_OWNED_BYTES: usize = 8 * 1024 * 1024;
pub const MICRONPLUS_ATTRIBUTE_MAX_ITEMS: usize = 64;
pub const MICRONPLUS_ATTRIBUTE_KEY_MAX_BYTES: usize = 256;
pub const MICRONPLUS_ATTRIBUTE_VALUE_MAX_BYTES: usize = 64 * 1024;
pub const MICRONPLUS_ATTRIBUTE_MAX_OWNED_BYTES: usize = 128 * 1024;
pub const MICRONPLUS_WIDGET_ID_MAX_BYTES: usize = 256;
pub const MICRONPLUS_WIDGET_TEXT_MAX_BYTES: usize = 16 * 1024;
pub const MICRONPLUS_WIDGET_STYLE_MAX_BYTES: usize = 64;
pub const MICRONPLUS_WIDGET_MARKUP_MAX_BYTES: usize = 256 * 1024;
pub const MICRONPLUS_WIDGET_STORE_MAX_WIDGETS: usize = 256;
pub const MICRONPLUS_WIDGET_STATE_MAX_ITEMS: usize = 1024;
pub const MICRONPLUS_WIDGET_STATE_MAX_OWNED_BYTES: usize = 1024 * 1024;
pub const MICRONPLUS_WIDGET_STORE_MAX_ITEMS: usize = 4096;
pub const MICRONPLUS_WIDGET_STORE_MAX_OWNED_BYTES: usize = 4 * 1024 * 1024;
pub const MICRONPLUS_EXTRACTED_EVENT_MAX_ITEMS: usize = 256;
pub const MICRONPLUS_EXTRACTED_EVENT_MAX_OWNED_BYTES: usize = 1024 * 1024;
pub const MICRONPLUS_CONTROL_EVENT_HISTORY_MAX_ITEMS: usize = 256;
pub const MICRONPLUS_CONTROL_EVENT_HISTORY_MAX_OWNED_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WidgetMarkupMode {
    LowerAll,
    PreserveColumns,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MicronPlusLive {
    pub id: String,
    pub src: String,
    pub refresh_secs: Option<u64>,
    pub loop_count: Option<u32>,
    pub fields: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MicronPlusInput {
    pub name: String,
    pub action: Option<String>,
    pub event: Option<String>,
    pub submit: Option<String>,
    pub fields: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MicronPlusButton {
    pub label: String,
    pub action: Option<String>,
    pub event: Option<String>,
    pub fields: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MicronPlusControlBinding {
    pub source: String,
    pub name: String,
    pub event: String,
    pub action: Option<String>,
    pub submit: Option<String>,
    pub fields: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MicronPlusControlEvent {
    pub event: String,
    pub source: String,
    pub name: Option<String>,
    pub action: Option<String>,
    pub fields: Vec<String>,
    pub value: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MicronPlusWidgetStore {
    widgets: BTreeMap<String, MicronPlusWidgetState>,
    #[serde(default, skip)]
    rejected_events: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MicronPlusWidgetStoreMetrics {
    pub widgets: usize,
    pub items: usize,
    pub owned_bytes: usize,
    pub rejected_events: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MicronPlusWidgetState {
    pub text: Option<String>,
    pub style: Option<String>,
    pub replace: bool,
    pub items: Vec<MicronPlusWidgetItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MicronPlusWidgetItem {
    pub text: String,
    pub style: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub markup: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MicronPlusWidgetEvent {
    StatusUpdate {
        id: String,
        text: String,
        style: Option<String>,
    },
    ScrollboxSet {
        id: String,
        items: Vec<MicronPlusWidgetItem>,
    },
    ScrollboxAppend {
        id: String,
        items: Vec<MicronPlusWidgetItem>,
    },
    LogAppend {
        id: String,
        items: Vec<MicronPlusWidgetItem>,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MicronPlusLowering {
    pub markup: String,
    pub lives: Vec<MicronPlusLive>,
    pub inputs: Vec<MicronPlusInput>,
    pub buttons: Vec<MicronPlusButton>,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MicronPlusWidgetTree {
    pub nodes: Vec<MicronPlusNode>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MicronPlusNode {
    Text {
        text: String,
    },
    Window {
        title: Option<String>,
        children: Vec<MicronPlusNode>,
    },
    Box {
        title: Option<String>,
        children: Vec<MicronPlusNode>,
    },
    Columns {
        columns: Vec<MicronPlusColumnNode>,
    },
    Scrollbox {
        id: Option<String>,
        title: Option<String>,
        height: Option<usize>,
        max: Option<usize>,
        children: Vec<MicronPlusNode>,
    },
    Log {
        id: Option<String>,
        height: Option<usize>,
        max: Option<usize>,
        children: Vec<MicronPlusNode>,
    },
    Input {
        name: String,
        label: Option<String>,
        width: Option<usize>,
        masked: bool,
        submit: Option<String>,
        event: Option<String>,
        action: Option<String>,
        fields: Vec<String>,
        value: Option<String>,
    },
    Button {
        label: String,
        event: Option<String>,
        action: Option<String>,
        fields: Vec<String>,
    },
    Status {
        id: Option<String>,
        text: String,
        style: Option<String>,
    },
    Live {
        id: String,
        src: String,
        refresh_secs: Option<u64>,
        loop_count: Option<u32>,
        fields: Vec<String>,
        children: Vec<MicronPlusNode>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MicronPlusColumnNode {
    pub title: Option<String>,
    pub width: Option<usize>,
    pub weight: usize,
    pub children: Vec<MicronPlusNode>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MicronPlusLayout {
    pub windows: Vec<MicronPlusWindowLayout>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MicronPlusWindowLayout {
    pub title: Option<String>,
    pub column_groups: Vec<MicronPlusColumnGroup>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MicronPlusColumnGroup {
    pub columns: Vec<MicronPlusColumnLayout>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MicronPlusColumnLayout {
    pub title: Option<String>,
    #[serde(default)]
    pub width: Option<usize>,
    pub weight: usize,
    pub raw_markup: String,
}

impl MicronPlusWidgetStore {
    pub fn apply_event(&mut self, event: MicronPlusWidgetEvent) -> bool {
        if validate_widget_event(&event).is_err() {
            self.rejected_events = self.rejected_events.saturating_add(1);
            return false;
        }
        let id = widget_event_id(&event).to_string();
        if !self.widgets.contains_key(&id)
            && self.widgets.len() >= MICRONPLUS_WIDGET_STORE_MAX_WIDGETS
        {
            self.rejected_events = self.rejected_events.saturating_add(1);
            return false;
        }
        let mut candidate = self.clone();
        let is_append = matches!(
            &event,
            MicronPlusWidgetEvent::ScrollboxAppend { .. } | MicronPlusWidgetEvent::LogAppend { .. }
        );
        let append_had_items = match &event {
            MicronPlusWidgetEvent::ScrollboxAppend { items, .. }
            | MicronPlusWidgetEvent::LogAppend { items, .. } => !items.is_empty(),
            _ => false,
        };
        match event {
            MicronPlusWidgetEvent::StatusUpdate { id, text, style } => {
                let state = candidate.widgets.entry(id).or_default();
                state.text = Some(text);
                state.style = style;
            }
            MicronPlusWidgetEvent::ScrollboxSet { id, items } => {
                let state = candidate.widgets.entry(id).or_default();
                state.replace = true;
                state.items = items;
            }
            MicronPlusWidgetEvent::ScrollboxAppend { id, items } => {
                let state = candidate.widgets.entry(id).or_default();
                state.items.extend(items);
            }
            MicronPlusWidgetEvent::LogAppend { id, items } => {
                let state = candidate.widgets.entry(id).or_default();
                state.replace = false;
                state.items.extend(items);
            }
        }
        if is_append
            && matches!(
                candidate.widgets.get(&id),
                Some(MicronPlusWidgetState { items, .. }) if items.len() > MICRONPLUS_WIDGET_STATE_MAX_ITEMS
            )
        {
            trim_widget_append_state(&mut candidate, &id);
        }
        if is_append && validate_widget_store(&candidate).is_err() {
            trim_widget_append_state(&mut candidate, &id);
        }
        if append_had_items
            && candidate
                .widgets
                .get(&id)
                .is_none_or(|state| state.items.is_empty())
        {
            self.rejected_events = self.rejected_events.saturating_add(1);
            return false;
        }
        if validate_widget_store(&candidate).is_err() {
            self.rejected_events = self.rejected_events.saturating_add(1);
            return false;
        }
        candidate.rejected_events = self.rejected_events;
        *self = candidate;
        true
    }

    pub fn get(&self, id: &str) -> Option<&MicronPlusWidgetState> {
        self.widgets.get(id)
    }

    pub fn metrics(&self) -> MicronPlusWidgetStoreMetrics {
        let (items, owned_bytes) = widget_store_usage(self).unwrap_or((usize::MAX, usize::MAX));
        MicronPlusWidgetStoreMetrics {
            widgets: self.widgets.len(),
            items,
            owned_bytes,
            rejected_events: self.rejected_events,
        }
    }
}

fn widget_event_id(event: &MicronPlusWidgetEvent) -> &str {
    match event {
        MicronPlusWidgetEvent::StatusUpdate { id, .. }
        | MicronPlusWidgetEvent::ScrollboxSet { id, .. }
        | MicronPlusWidgetEvent::ScrollboxAppend { id, .. }
        | MicronPlusWidgetEvent::LogAppend { id, .. } => id,
    }
}

fn validate_widget_event(event: &MicronPlusWidgetEvent) -> Result<usize, String> {
    let id = widget_event_id(event);
    if id.is_empty() || id.len() > MICRONPLUS_WIDGET_ID_MAX_BYTES {
        return Err(format!(
            "MicronPlus widget id must contain 1..={MICRONPLUS_WIDGET_ID_MAX_BYTES} bytes"
        ));
    }
    let mut owned = id.len();
    match event {
        MicronPlusWidgetEvent::StatusUpdate { text, style, .. } => {
            validate_widget_scalar("text", text, MICRONPLUS_WIDGET_TEXT_MAX_BYTES)?;
            owned = owned.saturating_add(text.len());
            if let Some(style) = style {
                validate_widget_scalar("style", style, MICRONPLUS_WIDGET_STYLE_MAX_BYTES)?;
                owned = owned.saturating_add(style.len());
            }
        }
        MicronPlusWidgetEvent::ScrollboxSet { items, .. }
        | MicronPlusWidgetEvent::ScrollboxAppend { items, .. }
        | MicronPlusWidgetEvent::LogAppend { items, .. } => {
            if items.len() > MICRONPLUS_WIDGET_STATE_MAX_ITEMS {
                return Err(format!(
                    "MicronPlus widget event exceeds {MICRONPLUS_WIDGET_STATE_MAX_ITEMS} items"
                ));
            }
            for item in items {
                owned = owned
                    .checked_add(widget_item_owned_bytes(item)?)
                    .ok_or_else(|| {
                        "MicronPlus widget event byte accounting overflow".to_string()
                    })?;
            }
            validate_widget_items_structure(items)?;
        }
    }
    if owned > MICRONPLUS_WIDGET_STATE_MAX_OWNED_BYTES {
        return Err(format!(
            "MicronPlus widget event exceeds {MICRONPLUS_WIDGET_STATE_MAX_OWNED_BYTES} owned bytes"
        ));
    }
    Ok(owned)
}

fn validate_widget_scalar(label: &str, value: &str, maximum: usize) -> Result<(), String> {
    if value.len() > maximum {
        Err(format!("MicronPlus widget {label} exceeds {maximum} bytes"))
    } else {
        Ok(())
    }
}

fn widget_item_owned_bytes(item: &MicronPlusWidgetItem) -> Result<usize, String> {
    validate_widget_scalar("item text", &item.text, MICRONPLUS_WIDGET_TEXT_MAX_BYTES)?;
    let mut owned = item.text.len();
    if let Some(style) = &item.style {
        validate_widget_scalar("item style", style, MICRONPLUS_WIDGET_STYLE_MAX_BYTES)?;
        owned = owned.saturating_add(style.len());
    }
    if let Some(markup) = &item.markup {
        validate_widget_scalar("item markup", markup, MICRONPLUS_WIDGET_MARKUP_MAX_BYTES)?;
        validate_micronplus_source(markup)?;
        owned = owned.saturating_add(markup.len());
    }
    Ok(owned)
}

fn widget_state_owned_bytes(state: &MicronPlusWidgetState) -> Result<usize, String> {
    if state.items.len() > MICRONPLUS_WIDGET_STATE_MAX_ITEMS {
        return Err(format!(
            "MicronPlus widget state exceeds {MICRONPLUS_WIDGET_STATE_MAX_ITEMS} items"
        ));
    }
    let mut owned = 0usize;
    if let Some(text) = &state.text {
        validate_widget_scalar("text", text, MICRONPLUS_WIDGET_TEXT_MAX_BYTES)?;
        owned = owned.saturating_add(text.len());
    }
    if let Some(style) = &state.style {
        validate_widget_scalar("style", style, MICRONPLUS_WIDGET_STYLE_MAX_BYTES)?;
        owned = owned.saturating_add(style.len());
    }
    for item in &state.items {
        owned = owned
            .checked_add(widget_item_owned_bytes(item)?)
            .ok_or_else(|| "MicronPlus widget state byte accounting overflow".to_string())?;
    }
    validate_widget_items_structure(&state.items)?;
    if owned > MICRONPLUS_WIDGET_STATE_MAX_OWNED_BYTES {
        return Err(format!(
            "MicronPlus widget state exceeds {MICRONPLUS_WIDGET_STATE_MAX_OWNED_BYTES} owned bytes"
        ));
    }
    Ok(owned)
}

fn validate_widget_items_structure(items: &[MicronPlusWidgetItem]) -> Result<(), String> {
    let mut nodes = 0usize;
    let mut columns = 0usize;
    for item in items {
        if let Some(markup) = &item.markup {
            let tree = try_parse_micronplus_tree(markup)?;
            let stats = micronplus_tree_stats(&tree)?;
            nodes = nodes.saturating_add(stats.nodes);
            columns = columns.saturating_add(stats.columns);
        } else {
            nodes = nodes.saturating_add(1);
        }
        if nodes > MICRONPLUS_TREE_MAX_NODES || columns > MICRONPLUS_TREE_MAX_COLUMNS {
            return Err(format!(
                "MicronPlus widget items exceed {MICRONPLUS_TREE_MAX_NODES} derived nodes or {MICRONPLUS_TREE_MAX_COLUMNS} columns"
            ));
        }
    }
    Ok(())
}

fn widget_store_usage(store: &MicronPlusWidgetStore) -> Result<(usize, usize), String> {
    if store.widgets.len() > MICRONPLUS_WIDGET_STORE_MAX_WIDGETS {
        return Err(format!(
            "MicronPlus widget store exceeds {MICRONPLUS_WIDGET_STORE_MAX_WIDGETS} widgets"
        ));
    }
    let mut items = 0usize;
    let mut owned = 0usize;
    for (id, state) in &store.widgets {
        if id.is_empty() || id.len() > MICRONPLUS_WIDGET_ID_MAX_BYTES {
            return Err("MicronPlus widget store contains an invalid id".into());
        }
        items = items.saturating_add(state.items.len());
        owned = owned
            .checked_add(id.len().saturating_add(widget_state_owned_bytes(state)?))
            .ok_or_else(|| "MicronPlus widget store byte accounting overflow".to_string())?;
    }
    Ok((items, owned))
}

fn validate_widget_store(store: &MicronPlusWidgetStore) -> Result<(), String> {
    let (items, owned) = widget_store_usage(store)?;
    if items > MICRONPLUS_WIDGET_STORE_MAX_ITEMS {
        return Err(format!(
            "MicronPlus widget store exceeds {MICRONPLUS_WIDGET_STORE_MAX_ITEMS} items"
        ));
    }
    if owned > MICRONPLUS_WIDGET_STORE_MAX_OWNED_BYTES {
        return Err(format!(
            "MicronPlus widget store exceeds {MICRONPLUS_WIDGET_STORE_MAX_OWNED_BYTES} owned bytes"
        ));
    }
    Ok(())
}

fn trim_widget_append_state(store: &mut MicronPlusWidgetStore, id: &str) {
    let mut other_items = 0usize;
    let mut other_owned = 0usize;
    for (other_id, state) in &store.widgets {
        if other_id == id {
            continue;
        }
        other_items = other_items.saturating_add(state.items.len());
        let Ok(state_owned) = widget_state_owned_bytes(state) else {
            return;
        };
        other_owned = other_owned.saturating_add(other_id.len().saturating_add(state_owned));
    }
    let Some(state) = store.widgets.get_mut(id) else {
        return;
    };
    let base_owned =
        state.text.as_ref().map_or(0, String::len) + state.style.as_ref().map_or(0, String::len);
    let allowed_items = MICRONPLUS_WIDGET_STATE_MAX_ITEMS
        .min(MICRONPLUS_WIDGET_STORE_MAX_ITEMS.saturating_sub(other_items));
    let allowed_owned = MICRONPLUS_WIDGET_STATE_MAX_OWNED_BYTES.min(
        MICRONPLUS_WIDGET_STORE_MAX_OWNED_BYTES
            .saturating_sub(other_owned)
            .saturating_sub(id.len()),
    );
    if base_owned > allowed_owned {
        state.items.clear();
        return;
    }
    let mut retained_owned = base_owned;
    let mut retained_items = 0usize;
    for item in state.items.iter().rev() {
        let Ok(item_owned) = widget_item_owned_bytes(item) else {
            break;
        };
        if retained_items >= allowed_items
            || retained_owned.saturating_add(item_owned) > allowed_owned
        {
            break;
        }
        retained_items += 1;
        retained_owned += item_owned;
    }
    let remove = state.items.len().saturating_sub(retained_items);
    state.items.drain(..remove);
}

impl MicronPlusWidgetItem {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: None,
            markup: None,
        }
    }

    pub fn styled(text: impl Into<String>, style: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: Some(style.into()),
            markup: None,
        }
    }

    pub fn markup(markup: impl Into<String>) -> Self {
        Self {
            text: String::new(),
            style: None,
            markup: Some(markup.into()),
        }
    }
}

pub fn widget_event_from_control_event(
    event: &MicronPlusControlEvent,
) -> Option<MicronPlusWidgetEvent> {
    let style = event_field_value(event, "style");
    if let Some(id) = event.event.strip_prefix("status.update.") {
        return (!id.is_empty()).then_some(MicronPlusWidgetEvent::StatusUpdate {
            id: id.to_string(),
            text: event_payload_text(event).unwrap_or_default(),
            style,
        });
    }

    if let Some(id) = event.event.strip_prefix("scrollbox.set.") {
        return (!id.is_empty()).then_some(MicronPlusWidgetEvent::ScrollboxSet {
            id: id.to_string(),
            items: vec![control_event_widget_item(event, style)],
        });
    }

    if let Some(id) = event.event.strip_prefix("scrollbox.append.") {
        return (!id.is_empty()).then_some(MicronPlusWidgetEvent::ScrollboxAppend {
            id: id.to_string(),
            items: vec![control_event_widget_item(event, style)],
        });
    }

    if let Some(id) = event.event.strip_prefix("log.append.") {
        return (!id.is_empty()).then_some(MicronPlusWidgetEvent::LogAppend {
            id: id.to_string(),
            items: vec![control_event_widget_item(event, style)],
        });
    }

    None
}

pub fn retain_micronplus_control_event(
    history: &mut Vec<MicronPlusControlEvent>,
    event: MicronPlusControlEvent,
) -> bool {
    if micronplus_control_event_owned_bytes(&event).is_err() {
        return false;
    }
    history.push(event);
    while history.len() > MICRONPLUS_CONTROL_EVENT_HISTORY_MAX_ITEMS
        || micronplus_control_event_history_owned_bytes(history)
            > MICRONPLUS_CONTROL_EVENT_HISTORY_MAX_OWNED_BYTES
    {
        history.remove(0);
    }
    true
}

fn micronplus_control_event_history_owned_bytes(history: &[MicronPlusControlEvent]) -> usize {
    history.iter().fold(0usize, |owned, event| {
        owned.saturating_add(micronplus_control_event_owned_bytes(event).unwrap_or(usize::MAX))
    })
}

fn micronplus_control_event_owned_bytes(event: &MicronPlusControlEvent) -> Result<usize, String> {
    validate_control_event_scalar("event", &event.event, MICRONPLUS_WIDGET_ID_MAX_BYTES)?;
    validate_control_event_scalar("source", &event.source, MICRON_CONTROL_NAME_MAX_BYTES)?;
    let mut owned = event.event.len().saturating_add(event.source.len());
    if let Some(name) = &event.name {
        validate_control_event_scalar("name", name, MICRON_CONTROL_NAME_MAX_BYTES)?;
        owned = owned.saturating_add(name.len());
    }
    if let Some(action) = &event.action {
        validate_control_event_scalar("action", action, MICRON_LINK_TARGET_MAX_BYTES)?;
        owned = owned.saturating_add(action.len());
    }
    if let Some(value) = &event.value {
        if value.len() > MICRON_CONTROL_VALUE_MAX_BYTES {
            return Err(format!(
                "MicronPlus control event value exceeds {MICRON_CONTROL_VALUE_MAX_BYTES} bytes"
            ));
        }
        owned = owned.saturating_add(value.len());
    }
    if event.fields.len() > MICRON_LINK_MAX_FIELDS {
        return Err(format!(
            "MicronPlus control event exceeds {MICRON_LINK_MAX_FIELDS} fields"
        ));
    }
    let mut field_owned = 0usize;
    for field in &event.fields {
        validate_control_event_scalar("field", field, MICRON_LINK_FIELD_MAX_BYTES)?;
        field_owned = field_owned.saturating_add(field.len());
    }
    if field_owned > MICRON_LINK_FIELDS_MAX_BYTES {
        return Err(format!(
            "MicronPlus control event fields exceed {MICRON_LINK_FIELDS_MAX_BYTES} bytes"
        ));
    }
    Ok(owned.saturating_add(field_owned))
}

fn validate_control_event_scalar(label: &str, value: &str, maximum: usize) -> Result<(), String> {
    if value.is_empty() || value.len() > maximum {
        Err(format!(
            "MicronPlus control event {label} must contain 1..={maximum} bytes"
        ))
    } else {
        Ok(())
    }
}

pub fn extract_micronplus_widget_events(markup: &str) -> (String, Vec<MicronPlusWidgetEvent>) {
    if validate_micronplus_source(markup).is_err() {
        return (markup.to_string(), Vec::new());
    }
    let mut retained = Vec::new();
    let mut events = Vec::new();
    let mut event_owned = 0usize;

    for line in markup.lines() {
        if let Some(event) = parse_widget_event_line(line) {
            let admitted = validate_widget_event(&event).ok().is_some_and(|owned| {
                events.len() < MICRONPLUS_EXTRACTED_EVENT_MAX_ITEMS
                    && event_owned.saturating_add(owned)
                        <= MICRONPLUS_EXTRACTED_EVENT_MAX_OWNED_BYTES
            });
            if admitted {
                event_owned = event_owned.saturating_add(
                    validate_widget_event(&event).expect("admitted widget event validates"),
                );
                events.push(event);
            } else {
                retained.push(line.to_string());
            }
        } else {
            retained.push(line.to_string());
        }
    }

    let mut cleaned = retained.join("\n");
    if markup.ends_with('\n') && !cleaned.is_empty() {
        cleaned.push('\n');
    }
    (cleaned, events)
}

fn control_event_widget_item(
    event: &MicronPlusControlEvent,
    style: Option<String>,
) -> MicronPlusWidgetItem {
    MicronPlusWidgetItem {
        text: event_payload_text(event).unwrap_or_else(|| control_event_summary(event)),
        style,
        markup: event_field_value(event, "markup"),
    }
}

fn event_payload_text(event: &MicronPlusControlEvent) -> Option<String> {
    event
        .value
        .clone()
        .filter(|value| !value.is_empty())
        .or_else(|| event_field_value(event, "text"))
}

fn event_field_value(event: &MicronPlusControlEvent, key: &str) -> Option<String> {
    event.fields.iter().find_map(|field| {
        let (field_key, value) = field.split_once('=')?;
        (field_key == key && !value.is_empty()).then(|| value.to_string())
    })
}

fn control_event_summary(event: &MicronPlusControlEvent) -> String {
    let name = event.name.as_deref().unwrap_or_default();
    let action = event.action.as_deref().unwrap_or_default();
    if name.is_empty() && action.is_empty() {
        event.source.clone()
    } else if action.is_empty() {
        format!("{}: {name}", event.source)
    } else if name.is_empty() {
        format!("{}: {action}", event.source)
    } else {
        format!("{}: {name} -> {action}", event.source)
    }
}

pub fn lower_micronplus_markup(markup: &str) -> MicronPlusLowering {
    lower_micronplus_markup_with_widgets(markup, None)
}

pub fn parse_micronplus_tree(markup: &str) -> MicronPlusWidgetTree {
    try_parse_micronplus_tree(markup).unwrap_or_default()
}

pub fn try_parse_micronplus_tree(markup: &str) -> Result<MicronPlusWidgetTree, String> {
    let source = admitted_micronplus_source(markup)?;
    let lines = source.lines().collect::<Vec<_>>();
    let mut index = 0usize;
    let mut budget = MicronPlusTreeBudget::default();
    let tree = MicronPlusWidgetTree {
        nodes: parse_micronplus_nodes(&lines, &mut index, None, 1, &mut budget)?,
    };
    validate_micronplus_tree_owned_bytes(&tree)?;
    Ok(tree)
}

pub fn has_micronplus_markup(markup: &str) -> bool {
    markup.lines().any(|line| {
        parse_tag(line).is_some_and(|(tag, _)| {
            matches!(
                tag.as_str(),
                "window"
                    | "box"
                    | "columns"
                    | "column"
                    | "scrollbox"
                    | "log"
                    | "input"
                    | "textbox"
                    | "button"
                    | "status"
                    | "live"
            )
        })
    })
}

pub fn apply_micronplus_tree_partial(
    tree: &mut MicronPlusWidgetTree,
    slot: &str,
    content: &str,
) -> bool {
    let Ok(fragment) = try_parse_micronplus_tree(strip_partial_document_headers(content)) else {
        return false;
    };
    let Ok(existing) = micronplus_tree_stats(tree) else {
        return false;
    };
    let Ok(fragment_stats) = micronplus_tree_stats(&fragment) else {
        return false;
    };
    let (matches, deepest_target) = micronplus_live_matches(tree, slot);
    if matches == 0
        || existing
            .nodes
            .saturating_add(fragment_stats.nodes.saturating_mul(matches))
            > MICRONPLUS_TREE_MAX_NODES
        || existing
            .columns
            .saturating_add(fragment_stats.columns.saturating_mul(matches))
            > MICRONPLUS_TREE_MAX_COLUMNS
        || existing
            .owned_bytes
            .saturating_add(fragment_stats.owned_bytes.saturating_mul(matches))
            > MICRONPLUS_TREE_MAX_OWNED_BYTES
        || deepest_target.saturating_add(fragment_stats.max_depth) > MICRONPLUS_TREE_MAX_DEPTH
    {
        return false;
    }
    let mut candidate = tree.clone();
    if !apply_micronplus_nodes_partial(&mut candidate.nodes, slot, &fragment.nodes)
        || micronplus_tree_stats(&candidate).is_err()
    {
        return false;
    }
    *tree = candidate;
    true
}

pub fn lower_micronplus_markup_with_widgets(
    markup: &str,
    widgets: Option<&MicronPlusWidgetStore>,
) -> MicronPlusLowering {
    lower_micronplus_markup_inner(markup, widgets, WidgetMarkupMode::LowerAll, None, None)
}

fn lower_micronplus_markup_inner(
    markup: &str,
    widgets: Option<&MicronPlusWidgetStore>,
    widget_markup_mode: WidgetMarkupMode,
    field_values: Option<&BTreeMap<String, String>>,
    implicit_control_width: Option<usize>,
) -> MicronPlusLowering {
    if let Err(error) = validate_micronplus_source(markup) {
        return MicronPlusLowering {
            markup: markup.to_string(),
            diagnostics: vec![error],
            ..MicronPlusLowering::default()
        };
    }
    let mut lives = Vec::new();
    let mut inputs = Vec::new();
    let mut buttons = Vec::new();
    let mut diagnostics = Vec::new();
    let mut output = Vec::new();
    let mut column_index = 0usize;
    let source = dedent_micronplus_source(markup);
    let lines = source.lines().collect::<Vec<_>>();
    let mut index = 0usize;

    while index < lines.len() {
        let line = lines[index];
        let repaired_line = repair_micronplus_tag_line(line);
        let parse_line = repaired_line.as_deref().unwrap_or(line);
        let Some((tag, attrs)) = parse_tag_for_lowering(parse_line, index + 1, &mut diagnostics)
        else {
            if let Some(closing_tag) = supported_closing_tag(line) {
                emit_closing_tag(&mut output, closing_tag);
                index += 1;
                continue;
            }
            output.push(line.to_string());
            index += 1;
            continue;
        };
        warn_unknown_attrs(&tag, &attrs, index + 1, &mut diagnostics);

        match tag.as_str() {
            "window" => {
                if let Some(title) = attrs.get("title").filter(|title| !title.is_empty()) {
                    output.push(format!(">{title}"));
                    output.push(section_header(title));
                } else {
                    output.push(section_rule());
                }
            }
            "box" => {
                if let Some(title) = attrs
                    .get("title")
                    .or_else(|| attrs.get("label"))
                    .filter(|title| !title.is_empty())
                {
                    output.push(section_header(title));
                } else {
                    output.push(section_rule());
                }
            }
            "columns" => {
                column_index = 0;
            }
            "column" => {
                column_index = column_index.saturating_add(1);
                if column_index > 1 {
                    output.push(column_separator());
                }
                if let Some(title) = attrs
                    .get("title")
                    .or_else(|| attrs.get("label"))
                    .filter(|title| !title.is_empty())
                {
                    output.push(format!("`F888[{title}]`f"));
                }
            }
            "scrollbox" => {
                let Some(end_index) = find_closing_tag(&lines, index + 1, "scrollbox") else {
                    output.push(line.to_string());
                    index += 1;
                    continue;
                };
                if let Some(title) = attrs
                    .get("title")
                    .or_else(|| attrs.get("label"))
                    .filter(|title| !title.is_empty())
                {
                    output.push(section_header(title));
                } else {
                    output.push(section_rule());
                }
                emit_scrollbox_body(
                    &mut output,
                    &attrs,
                    &lines[index + 1..end_index],
                    widgets,
                    widget_markup_mode,
                );
                output.push(section_rule());
                index = end_index + 1;
                continue;
            }
            "log" => {
                let Some(end_index) = find_closing_tag(&lines, index + 1, "log") else {
                    output.push(line.to_string());
                    index += 1;
                    continue;
                };
                if let Some(id) = attrs.get("id").filter(|id| !id.is_empty()) {
                    output.push(section_header(&format!("log: {id}")));
                } else {
                    output.push(section_rule());
                }
                emit_log_body(
                    &mut output,
                    &attrs,
                    &lines[index + 1..end_index],
                    widgets,
                    widget_markup_mode,
                );
                output.push(section_rule());
                index = end_index + 1;
                continue;
            }
            "status" => {
                let widget_state = attrs
                    .get("id")
                    .and_then(|id| widgets.and_then(|store| store.get(id)));
                let text = widget_state
                    .and_then(|state| state.text.clone())
                    .or_else(|| attrs.get("text").cloned())
                    .unwrap_or_else(|| "status".into());
                let style = widget_state
                    .and_then(|state| state.style.as_deref())
                    .or_else(|| attrs.get("style").map(String::as_str))
                    .unwrap_or("info");
                output.push(status_line(&text, style));
            }
            "live" => {
                let id = attrs.get("id").cloned().unwrap_or_else(|| "live".into());
                let src = attrs.get("src").cloned().unwrap_or_default();
                if src.is_empty() {
                    output.push(line.to_string());
                    index += 1;
                    continue;
                }
                let refresh_secs = attrs
                    .get("refresh")
                    .and_then(|value| parse_refresh_secs(value));
                let loop_count = attrs.get("loop").and_then(|value| parse_loop_count(value));
                let Some(fields) = split_fields(attrs.get("fields").map(String::as_str)) else {
                    diagnostics.push(format!(
                        "line {}: <live> fields exceed Micron link limits",
                        index + 1
                    ));
                    output.push(line.to_string());
                    index += 1;
                    continue;
                };
                lives.push(MicronPlusLive {
                    id: id.clone(),
                    src: src.clone(),
                    refresh_secs,
                    loop_count,
                    fields: fields.clone(),
                });
                output.push(partial_line(&src, refresh_secs, loop_count, &id, &fields));
            }
            "input" | "textbox" => {
                let mut attrs = attrs;
                let Some(name) = attrs.get("name").cloned() else {
                    output.push(line.to_string());
                    index += 1;
                    continue;
                };
                if let Some(value) = field_values.and_then(|values| values.get(&name)) {
                    attrs.insert("value".into(), value.clone());
                }
                let Some(fields) = split_fields(attrs.get("fields").map(String::as_str)) else {
                    diagnostics.push(format!(
                        "line {}: <{tag}> fields exceed Micron link limits",
                        index + 1
                    ));
                    output.push(line.to_string());
                    index += 1;
                    continue;
                };
                inputs.push(MicronPlusInput {
                    name: name.clone(),
                    action: attrs.get("action").cloned(),
                    event: attrs.get("event").cloned(),
                    submit: attrs.get("submit").cloned(),
                    fields,
                });
                if let Some(label) = attrs
                    .get("label")
                    .or_else(|| attrs.get("title"))
                    .filter(|label| !label.is_empty())
                {
                    output.push(format!("`F888{label}`f"));
                }
                output.push(control_line(
                    &attrs,
                    &name,
                    implicit_control_width.unwrap_or(DEFAULT_CONTROL_WIDTH),
                ));
            }
            "button" => {
                let label = attrs
                    .get("label")
                    .cloned()
                    .unwrap_or_else(|| "Button".into());
                let action = attrs.get("action").cloned();
                let event = attrs.get("event").cloned();
                let Some(fields) = split_fields(attrs.get("fields").map(String::as_str)) else {
                    diagnostics.push(format!(
                        "line {}: <button> fields exceed Micron link limits",
                        index + 1
                    ));
                    output.push(line.to_string());
                    index += 1;
                    continue;
                };
                let target = action.clone().unwrap_or_else(|| {
                    event
                        .as_ref()
                        .map(|event| micronplus_event_target(event))
                        .unwrap_or_else(|| "#unsupported".into())
                });
                buttons.push(MicronPlusButton {
                    label: label.clone(),
                    action,
                    event,
                    fields: fields.clone(),
                });
                output.push(link_line(&label, &target, &fields));
            }
            _ => output.push(line.to_string()),
        }
        index += 1;
    }

    MicronPlusLowering {
        markup: cleanup_lowered_lines(output),
        lives,
        inputs,
        buttons,
        diagnostics,
    }
}

pub fn extract_micronplus_layout(markup: &str) -> MicronPlusLayout {
    try_extract_micronplus_layout(markup).unwrap_or_default()
}

pub fn try_extract_micronplus_layout(markup: &str) -> Result<MicronPlusLayout, String> {
    let source = admitted_micronplus_source(markup)?;
    let lines = source.lines().collect::<Vec<_>>();
    let mut windows = Vec::new();
    let mut root_groups = Vec::new();
    let mut index = 0usize;
    let mut budget = MicronPlusLayoutBudget::default();

    while index < lines.len() {
        let Some((tag, attrs)) = parse_tag(lines[index]) else {
            index += 1;
            continue;
        };

        match tag.as_str() {
            "window" => {
                let Some(end_index) = find_closing_tag(&lines, index + 1, "window") else {
                    index += 1;
                    continue;
                };
                let column_groups =
                    extract_column_groups_from_lines(&lines[index + 1..end_index], &mut budget)?;
                if !column_groups.is_empty() {
                    budget.admit_window()?;
                    windows.push(MicronPlusWindowLayout {
                        title: attrs.get("title").cloned(),
                        column_groups,
                    });
                }
                index = end_index + 1;
            }
            "columns" => {
                let Some(end_index) = find_closing_tag(&lines, index + 1, "columns") else {
                    index += 1;
                    continue;
                };
                if let Some(group) =
                    parse_column_group_bounded(&lines[index + 1..end_index], &mut budget)?
                {
                    root_groups.push(group);
                }
                index = end_index + 1;
            }
            _ => index += 1,
        }
    }

    if !root_groups.is_empty() {
        budget.admit_window()?;
        windows.push(MicronPlusWindowLayout {
            title: None,
            column_groups: root_groups,
        });
    }

    let layout = MicronPlusLayout { windows };
    validate_micronplus_layout_owned_bytes(&layout)?;
    Ok(layout)
}

pub fn apply_micronplus_layout_partial(
    layout: &mut MicronPlusLayout,
    slot: &str,
    content: &str,
) -> bool {
    if slot.len() > crate::browser::partials::PARTIAL_ID_MAX_BYTES
        || validate_micronplus_source(content).is_err()
    {
        return false;
    }
    let Ok(existing_owned) = micronplus_layout_owned_bytes(layout) else {
        return false;
    };
    let content = strip_partial_document_headers(content);
    let matches = micronplus_layout_partial_matches(layout, slot);
    let marker_bytes = partial_marker("BEGIN", slot)
        .len()
        .saturating_add(partial_marker("END", slot).len())
        .saturating_add(4);
    if matches == 0
        || existing_owned.saturating_add(
            content
                .len()
                .saturating_add(marker_bytes)
                .saturating_mul(matches),
        ) > MICRONPLUS_LAYOUT_MAX_OWNED_BYTES
    {
        return false;
    }
    let mut candidate = layout.clone();
    let mut changed = false;
    for window in &mut candidate.windows {
        for group in &mut window.column_groups {
            for column in &mut group.columns {
                let mut output = Vec::new();
                let mut column_changed = false;
                let lines = column.raw_markup.lines().collect::<Vec<_>>();
                let mut index = 0usize;
                while index < lines.len() {
                    let line = lines[index];
                    if partial_marker_matches(line, "BEGIN", slot) {
                        output.push(partial_marker("BEGIN", slot));
                        output.extend(content.lines().map(str::to_string));
                        if let Some(end_index) = find_partial_marker_end(&lines, index + 1, slot) {
                            output.push(partial_marker("END", slot));
                            index = end_index + 1;
                        } else {
                            output.push(partial_marker("END", slot));
                            index += 1;
                        }
                        column_changed = true;
                        continue;
                    }
                    let is_target_live = parse_tag(line).is_some_and(|(tag, attrs)| {
                        tag == "live"
                            && attrs
                                .get("id")
                                .map(|id| id == slot)
                                .unwrap_or(slot == "live")
                    });
                    if is_target_live {
                        output.push(partial_marker("BEGIN", slot));
                        output.extend(content.lines().map(str::to_string));
                        output.push(partial_marker("END", slot));
                        column_changed = true;
                    } else {
                        output.push(line.to_string());
                    }
                    index += 1;
                }
                if column_changed {
                    column.raw_markup = output.join("\n");
                    changed = true;
                }
            }
        }
    }
    if !changed || validate_micronplus_layout_owned_bytes(&candidate).is_err() {
        return false;
    }
    *layout = candidate;
    true
}

fn micronplus_layout_partial_matches(layout: &MicronPlusLayout, slot: &str) -> usize {
    let mut matches = 0usize;
    for window in &layout.windows {
        for group in &window.column_groups {
            for column in &group.columns {
                let lines = column.raw_markup.lines().collect::<Vec<_>>();
                let mut index = 0usize;
                while index < lines.len() {
                    let line = lines[index];
                    if partial_marker_matches(line, "BEGIN", slot) {
                        matches = matches.saturating_add(1);
                        index = find_partial_marker_end(&lines, index + 1, slot)
                            .map_or(index + 1, |end| end + 1);
                        continue;
                    }
                    if parse_tag(line).is_some_and(|(tag, attrs)| {
                        tag == "live"
                            && attrs
                                .get("id")
                                .map(|id| id == slot)
                                .unwrap_or(slot == "live")
                    }) {
                        matches = matches.saturating_add(1);
                    }
                    index += 1;
                }
            }
        }
    }
    matches
}

fn partial_marker(kind: &str, slot: &str) -> String {
    format!("# OMENBROWSER_RS_PARTIAL_{kind} {slot}")
}

fn partial_marker_matches(line: &str, kind: &str, slot: &str) -> bool {
    line.trim() == partial_marker(kind, slot)
}

fn find_partial_marker_end(lines: &[&str], start: usize, slot: &str) -> Option<usize> {
    lines
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(index, line)| partial_marker_matches(line, "END", slot).then_some(index))
}

pub fn render_column_group_preview(group: &MicronPlusColumnGroup, width: usize) -> Vec<String> {
    render_column_group_rows(group, width)
        .into_iter()
        .map(|row| row.text().trim_end().to_string())
        .collect()
}

pub fn render_column_group_rows(group: &MicronPlusColumnGroup, width: usize) -> Vec<RenderedRow> {
    render_column_group_rows_with_widgets(group, width, None)
}

pub fn render_column_group_rows_with_widgets(
    group: &MicronPlusColumnGroup,
    width: usize,
    widgets: Option<&MicronPlusWidgetStore>,
) -> Vec<RenderedRow> {
    render_column_group_rows_with_widgets_and_field_cursor(group, width, widgets, None)
}

pub fn render_column_group_rows_with_widgets_and_field_cursor(
    group: &MicronPlusColumnGroup,
    width: usize,
    widgets: Option<&MicronPlusWidgetStore>,
    field_cursor: Option<(&str, usize)>,
) -> Vec<RenderedRow> {
    render_column_group_rows_with_widgets_fields_and_cursor(
        group,
        width,
        widgets,
        None,
        field_cursor,
    )
}

pub fn render_column_group_rows_with_widgets_fields_and_cursor(
    group: &MicronPlusColumnGroup,
    width: usize,
    widgets: Option<&MicronPlusWidgetStore>,
    field_values: Option<&BTreeMap<String, String>>,
    field_cursor: Option<(&str, usize)>,
) -> Vec<RenderedRow> {
    if group.columns.is_empty() || width == 0 {
        return Vec::new();
    }
    let gutter = DEFAULT_COLUMN_GAP;
    let gutter_total = gutter
        .len()
        .saturating_mul(group.columns.len().saturating_sub(1));
    let available = width.saturating_sub(gutter_total).max(group.columns.len());
    let column_widths = distribute_column_widths(group, available);
    let rendered_columns = group
        .columns
        .iter()
        .zip(column_widths.iter())
        .map(|(column, column_width)| {
            render_column_preview_rows(column, *column_width, widgets, field_values, field_cursor)
        })
        .collect::<Vec<_>>();
    let row_count = rendered_columns
        .iter()
        .map(Vec::len)
        .max()
        .unwrap_or_default();
    let mut rows = Vec::new();

    for row_index in 0..row_count {
        let mut cells = Vec::new();
        for (column_index, column_lines) in rendered_columns.iter().enumerate() {
            if column_index > 0 {
                cells.extend(gutter_cells(gutter));
            }
            let column_width = column_widths[column_index];
            let column_cells = column_lines
                .get(row_index)
                .map(|row| row.cells.as_slice())
                .unwrap_or(&[]);
            cells.extend(pad_or_truncate_cells(column_cells, column_width));
        }
        trim_trailing_plain_spaces(&mut cells);
        rows.push(RenderedRow {
            cells,
            align: Alignment::Left,
            depth: 0,
            base_style: TextStyle::default(),
            wrap: false,
        });
    }

    rows
}

pub fn render_micronplus_rows_with_widgets(
    markup: &str,
    width: usize,
    widgets: Option<&MicronPlusWidgetStore>,
) -> Vec<RenderedRow> {
    render_micronplus_rows_with_widgets_and_field_cursor(markup, width, widgets, None)
}

pub fn render_micronplus_rows_with_widgets_and_field_cursor(
    markup: &str,
    width: usize,
    widgets: Option<&MicronPlusWidgetStore>,
    field_cursor: Option<(&str, usize)>,
) -> Vec<RenderedRow> {
    render_micronplus_fragment_rows(markup, width, widgets, None, field_cursor)
}

pub fn render_micronplus_tree_rows_with_widgets_and_field_cursor(
    tree: &MicronPlusWidgetTree,
    width: usize,
    widgets: Option<&MicronPlusWidgetStore>,
    field_values: Option<&BTreeMap<String, String>>,
    field_cursor: Option<(&str, usize)>,
) -> Vec<RenderedRow> {
    let context = TreeRenderContext {
        widgets,
        field_values,
        field_cursor,
    };
    render_micronplus_nodes_rows(&tree.nodes, width.max(1), context)
}

pub fn render_micronplus_tree_rows_with_widgets(
    tree: &MicronPlusWidgetTree,
    width: usize,
    widgets: Option<&MicronPlusWidgetStore>,
) -> Vec<RenderedRow> {
    render_micronplus_tree_rows_with_widgets_and_field_cursor(tree, width, widgets, None, None)
}

#[derive(Clone, Copy)]
struct TreeRenderContext<'a> {
    widgets: Option<&'a MicronPlusWidgetStore>,
    field_values: Option<&'a BTreeMap<String, String>>,
    field_cursor: Option<(&'a str, usize)>,
}

#[derive(Default)]
struct MicronPlusLayoutBudget {
    windows: usize,
    groups: usize,
    columns: usize,
}

impl MicronPlusLayoutBudget {
    fn admit_window(&mut self) -> Result<(), String> {
        self.windows = self.windows.saturating_add(1);
        if self.windows > MICRONPLUS_LAYOUT_MAX_WINDOWS {
            return Err(format!(
                "MicronPlus layout exceeds {MICRONPLUS_LAYOUT_MAX_WINDOWS} windows"
            ));
        }
        Ok(())
    }

    fn admit_group(&mut self) -> Result<(), String> {
        self.groups = self.groups.saturating_add(1);
        if self.groups > MICRONPLUS_LAYOUT_MAX_GROUPS {
            return Err(format!(
                "MicronPlus layout exceeds {MICRONPLUS_LAYOUT_MAX_GROUPS} column groups"
            ));
        }
        Ok(())
    }

    fn admit_column(&mut self) -> Result<(), String> {
        self.columns = self.columns.saturating_add(1);
        if self.columns > MICRONPLUS_LAYOUT_MAX_COLUMNS {
            return Err(format!(
                "MicronPlus layout exceeds {MICRONPLUS_LAYOUT_MAX_COLUMNS} columns"
            ));
        }
        Ok(())
    }
}

fn extract_column_groups_from_lines(
    lines: &[&str],
    budget: &mut MicronPlusLayoutBudget,
) -> Result<Vec<MicronPlusColumnGroup>, String> {
    let mut groups = Vec::new();
    let mut index = 0usize;
    while index < lines.len() {
        let Some((tag, _attrs)) = parse_tag(lines[index]) else {
            index += 1;
            continue;
        };
        if tag != "columns" {
            index += 1;
            continue;
        }
        let Some(end_index) = find_closing_tag(lines, index + 1, "columns") else {
            index += 1;
            continue;
        };
        if let Some(group) = parse_column_group_bounded(&lines[index + 1..end_index], budget)? {
            groups.push(group);
        }
        index = end_index + 1;
    }
    Ok(groups)
}

fn parse_column_group(lines: &[&str]) -> Option<MicronPlusColumnGroup> {
    parse_column_group_bounded(lines, &mut MicronPlusLayoutBudget::default())
        .ok()
        .flatten()
}

fn parse_column_group_bounded(
    lines: &[&str],
    budget: &mut MicronPlusLayoutBudget,
) -> Result<Option<MicronPlusColumnGroup>, String> {
    let mut columns = Vec::new();
    let mut index = 0usize;
    while index < lines.len() {
        let Some((tag, attrs)) = parse_tag(lines[index]) else {
            index += 1;
            continue;
        };
        if tag != "column" {
            index += 1;
            continue;
        }
        let Some(end_index) = find_closing_tag(lines, index + 1, "column") else {
            index += 1;
            continue;
        };
        budget.admit_column()?;
        columns.push(MicronPlusColumnLayout {
            title: attrs.get("title").or_else(|| attrs.get("label")).cloned(),
            width: attrs
                .get("width")
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|width| *width > 0),
            weight: attrs
                .get("weight")
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|weight| *weight > 0)
                .unwrap_or(1),
            raw_markup: lines[index + 1..end_index].join("\n"),
        });
        index = end_index + 1;
    }

    if columns.is_empty() {
        Ok(None)
    } else {
        budget.admit_group()?;
        Ok(Some(MicronPlusColumnGroup { columns }))
    }
}

fn validate_micronplus_layout_owned_bytes(layout: &MicronPlusLayout) -> Result<(), String> {
    micronplus_layout_owned_bytes(layout).map(|_| ())
}

fn micronplus_layout_owned_bytes(layout: &MicronPlusLayout) -> Result<usize, String> {
    let mut owned = 0usize;
    for window in &layout.windows {
        if let Some(title) = &window.title {
            if title.len() > crate::browser::page::BROWSER_PAGE_TITLE_MAX_BYTES {
                return Err(format!(
                    "MicronPlus layout title exceeds {} bytes",
                    crate::browser::page::BROWSER_PAGE_TITLE_MAX_BYTES
                ));
            }
            owned = owned
                .checked_add(title.len())
                .ok_or_else(|| "MicronPlus layout byte accounting overflow".to_string())?;
        }
        for group in &window.column_groups {
            for column in &group.columns {
                if let Some(title) = &column.title {
                    if title.len() > crate::browser::page::BROWSER_PAGE_TITLE_MAX_BYTES {
                        return Err(format!(
                            "MicronPlus column title exceeds {} bytes",
                            crate::browser::page::BROWSER_PAGE_TITLE_MAX_BYTES
                        ));
                    }
                    owned = owned
                        .checked_add(title.len())
                        .ok_or_else(|| "MicronPlus layout byte accounting overflow".to_string())?;
                }
                owned = owned
                    .checked_add(column.raw_markup.len())
                    .ok_or_else(|| "MicronPlus layout byte accounting overflow".to_string())?;
                if owned > MICRONPLUS_LAYOUT_MAX_OWNED_BYTES {
                    return Err(format!(
                        "MicronPlus layout exceeds {MICRONPLUS_LAYOUT_MAX_OWNED_BYTES} owned bytes"
                    ));
                }
            }
        }
    }
    Ok(owned)
}

fn distribute_column_widths(group: &MicronPlusColumnGroup, available: usize) -> Vec<usize> {
    let mut widths = vec![0; group.columns.len()];
    let mut remaining = available;
    let mut weighted_indices = Vec::new();
    let mut total_weight = 0usize;

    for (index, column) in group.columns.iter().enumerate() {
        if let Some(width) = column.width {
            widths[index] = width.max(8);
            remaining = remaining.saturating_sub(widths[index]);
        } else {
            weighted_indices.push(index);
            total_weight = total_weight.saturating_add(column.weight.max(1));
        }
    }

    let min_weighted_width = 8usize.saturating_mul(weighted_indices.len());
    remaining = remaining.max(min_weighted_width);
    let total_weight = total_weight.max(1);
    let mut assigned = 0usize;
    for (offset, index) in weighted_indices.iter().enumerate() {
        let weight = group.columns[*index].weight.max(1);
        widths[*index] = if offset == weighted_indices.len().saturating_sub(1) {
            remaining.saturating_sub(assigned).max(8)
        } else {
            let slice_width = remaining.saturating_mul(weight) / total_weight;
            let slice_width = slice_width.max(8);
            assigned = assigned.saturating_add(slice_width);
            slice_width
        };
    }

    widths
}

fn render_column_preview_rows(
    column: &MicronPlusColumnLayout,
    width: usize,
    widgets: Option<&MicronPlusWidgetStore>,
    field_values: Option<&BTreeMap<String, String>>,
    field_cursor: Option<(&str, usize)>,
) -> Vec<RenderedRow> {
    let mut markup = Vec::new();
    if let Some(title) = &column.title {
        markup.push(format!("`F888[{title}]`f"));
    }
    let mut rows = render_micronplus_fragment_rows(
        &markup.join("\n"),
        width,
        widgets,
        field_values,
        field_cursor,
    );
    rows.extend(render_micronplus_fragment_rows(
        &column.raw_markup,
        width,
        widgets,
        field_values,
        field_cursor,
    ));
    rows
}

fn render_micronplus_fragment_rows(
    markup: &str,
    width: usize,
    widgets: Option<&MicronPlusWidgetStore>,
    field_values: Option<&BTreeMap<String, String>>,
    field_cursor: Option<(&str, usize)>,
) -> Vec<RenderedRow> {
    let lines = markup.lines().collect::<Vec<_>>();
    let mut rows = Vec::new();
    let mut plain_lines = Vec::new();
    let mut index = 0usize;

    while index < lines.len() {
        let Some((tag, attrs)) = parse_tag(lines[index]) else {
            plain_lines.push(lines[index]);
            index += 1;
            continue;
        };
        if !matches!(
            tag.as_str(),
            "columns" | "scrollbox" | "log" | "window" | "box"
        ) {
            plain_lines.push(lines[index]);
            index += 1;
            continue;
        }

        let Some(end_index) = find_closing_tag(&lines, index + 1, &tag) else {
            plain_lines.push(lines[index]);
            index += 1;
            continue;
        };

        flush_lowered_fragment_rows(
            &mut rows,
            &mut plain_lines,
            width,
            widgets,
            field_values,
            field_cursor,
        );
        match tag.as_str() {
            "columns" => {
                if let Some(group) = parse_column_group(&lines[index + 1..end_index]) {
                    rows.extend(render_column_group_rows_with_widgets_fields_and_cursor(
                        &group,
                        width,
                        widgets,
                        field_values,
                        field_cursor,
                    ));
                }
            }
            "scrollbox" => rows.extend(render_structured_scrollbox_rows(
                &attrs,
                &lines[index + 1..end_index],
                width,
                widgets,
                field_values,
                field_cursor,
            )),
            "log" => rows.extend(render_structured_log_rows(
                &attrs,
                &lines[index + 1..end_index],
                width,
                widgets,
                field_values,
                field_cursor,
            )),
            "window" | "box" => rows.extend(render_structured_container_rows(
                &tag,
                &attrs,
                &lines[index + 1..end_index],
                width,
                widgets,
                field_values,
                field_cursor,
            )),
            _ => {}
        }
        index = end_index + 1;
    }

    flush_lowered_fragment_rows(
        &mut rows,
        &mut plain_lines,
        width,
        widgets,
        field_values,
        field_cursor,
    );
    rows
}

fn render_micronplus_nodes_rows(
    nodes: &[MicronPlusNode],
    width: usize,
    context: TreeRenderContext<'_>,
) -> Vec<RenderedRow> {
    let mut rows = Vec::new();
    let mut plain_lines = Vec::new();

    for node in nodes {
        match node {
            MicronPlusNode::Text { text } => plain_lines.push(text.as_str()),
            MicronPlusNode::Window { title, children } => {
                flush_lowered_fragment_rows(
                    &mut rows,
                    &mut plain_lines,
                    width,
                    context.widgets,
                    context.field_values,
                    context.field_cursor,
                );
                rows.extend(render_tree_container_rows(
                    title.as_deref(),
                    children,
                    width,
                    context,
                    true,
                ));
            }
            MicronPlusNode::Box { title, children } => {
                flush_lowered_fragment_rows(
                    &mut rows,
                    &mut plain_lines,
                    width,
                    context.widgets,
                    context.field_values,
                    context.field_cursor,
                );
                rows.extend(render_tree_container_rows(
                    title.as_deref(),
                    children,
                    width,
                    context,
                    false,
                ));
            }
            MicronPlusNode::Columns { columns } => {
                flush_lowered_fragment_rows(
                    &mut rows,
                    &mut plain_lines,
                    width,
                    context.widgets,
                    context.field_values,
                    context.field_cursor,
                );
                rows.extend(render_micronplus_column_nodes_rows(columns, width, context));
            }
            MicronPlusNode::Scrollbox {
                id,
                title,
                height,
                max: _,
                children,
            } => {
                flush_lowered_fragment_rows(
                    &mut rows,
                    &mut plain_lines,
                    width,
                    context.widgets,
                    context.field_values,
                    context.field_cursor,
                );
                rows.extend(render_tree_scrollbox_rows(
                    id.as_deref(),
                    title.as_deref(),
                    *height,
                    children,
                    width,
                    context,
                ));
            }
            MicronPlusNode::Log {
                id,
                height,
                max,
                children,
            } => {
                flush_lowered_fragment_rows(
                    &mut rows,
                    &mut plain_lines,
                    width,
                    context.widgets,
                    context.field_values,
                    context.field_cursor,
                );
                rows.extend(render_tree_log_rows(
                    id.as_deref(),
                    *height,
                    *max,
                    children,
                    width,
                    context,
                ));
            }
            MicronPlusNode::Input {
                name,
                label,
                width: control_width,
                masked,
                submit: _,
                event: _,
                action: _,
                fields: _,
                value,
            } => {
                flush_lowered_fragment_rows(
                    &mut rows,
                    &mut plain_lines,
                    width,
                    context.widgets,
                    context.field_values,
                    context.field_cursor,
                );
                if let Some(label) = label {
                    rows.extend(render_document(
                        &parse_micron(&format!("`F888{label}`f")),
                        width,
                    ));
                }
                let current_value = context
                    .field_values
                    .and_then(|values| values.get(name))
                    .map(String::as_str)
                    .or(value.as_deref());
                let resolved_width = control_width.unwrap_or(width).max(1);
                let attrs = control_node_attrs(name, resolved_width, *masked, current_value);
                rows.extend(render_document_with_field_cursor(
                    &parse_micron(&control_line(&attrs, name, resolved_width)),
                    width,
                    context.field_cursor.map(|(name, _)| name),
                    context.field_cursor,
                ));
            }
            MicronPlusNode::Button {
                label,
                event,
                action,
                fields,
            } => {
                flush_lowered_fragment_rows(
                    &mut rows,
                    &mut plain_lines,
                    width,
                    context.widgets,
                    context.field_values,
                    context.field_cursor,
                );
                let target = action.clone().unwrap_or_else(|| {
                    event
                        .as_deref()
                        .map(micronplus_event_target)
                        .unwrap_or_else(|| "#unsupported".into())
                });
                rows.extend(render_document(
                    &parse_micron(&link_line(label, &target, fields)),
                    width,
                ));
            }
            MicronPlusNode::Status { id, text, style } => {
                flush_lowered_fragment_rows(
                    &mut rows,
                    &mut plain_lines,
                    width,
                    context.widgets,
                    context.field_values,
                    context.field_cursor,
                );
                let widget_state = id
                    .as_ref()
                    .and_then(|id| context.widgets.and_then(|store| store.get(id)));
                let text = widget_state
                    .and_then(|state| state.text.as_deref())
                    .unwrap_or(text);
                let style = widget_state
                    .and_then(|state| state.style.as_deref())
                    .or(style.as_deref())
                    .unwrap_or("info");
                rows.extend(render_document(
                    &parse_micron(&status_line(text, style)),
                    width,
                ));
            }
            MicronPlusNode::Live { children, .. } => {
                flush_lowered_fragment_rows(
                    &mut rows,
                    &mut plain_lines,
                    width,
                    context.widgets,
                    context.field_values,
                    context.field_cursor,
                );
                if !children.is_empty() {
                    rows.extend(render_micronplus_nodes_rows(children, width, context));
                }
            }
        }
    }

    flush_lowered_fragment_rows(
        &mut rows,
        &mut plain_lines,
        width,
        context.widgets,
        context.field_values,
        context.field_cursor,
    );
    rows
}

fn render_tree_container_rows(
    title: Option<&str>,
    children: &[MicronPlusNode],
    width: usize,
    context: TreeRenderContext<'_>,
    _window: bool,
) -> Vec<RenderedRow> {
    let inner_width = width.saturating_sub(2).max(12);
    let rendered_inner = render_micronplus_nodes_rows(children, inner_width, context);
    boxed_rows(
        rendered_inner,
        inner_width,
        title.filter(|title| !title.is_empty()),
        None,
    )
}

fn render_tree_scrollbox_rows(
    id: Option<&str>,
    title: Option<&str>,
    height: Option<usize>,
    children: &[MicronPlusNode],
    width: usize,
    context: TreeRenderContext<'_>,
) -> Vec<RenderedRow> {
    let inner_width = width.saturating_sub(2).max(8);
    let body_nodes = widget_augmented_nodes(id, children, context.widgets, false);
    let mut body_rows = render_micronplus_nodes_rows(&body_nodes, inner_width, context);
    if let Some(height) = height {
        let start = body_rows.len().saturating_sub(height);
        body_rows = body_rows.split_off(start);
    }
    boxed_rows(
        body_rows,
        inner_width,
        title.filter(|title| !title.is_empty()),
        height,
    )
}

fn boxed_rows(
    inner_rows: Vec<RenderedRow>,
    inner_width: usize,
    title: Option<&str>,
    height: Option<usize>,
) -> Vec<RenderedRow> {
    let mut output = Vec::new();
    let mut body_rows = inner_rows;
    if let Some(height) = height {
        if body_rows.len() > height {
            let start = body_rows.len().saturating_sub(height);
            body_rows = body_rows.split_off(start);
        }
        while body_rows.len() < height {
            body_rows.push(empty_row());
        }
    }

    output.push(box_border_row('┌', '─', '┐', inner_width, title));
    for row in body_rows {
        let mut cells = Vec::with_capacity(inner_width.saturating_add(2));
        cells.push(plain_cell('│'));
        cells.extend(pad_or_truncate_cells(&row.cells, inner_width));
        cells.push(plain_cell('│'));
        output.push(rendered_plain_row(cells));
    }
    output.push(box_border_row('└', '─', '┘', inner_width, None));

    output
}

fn box_border_row(
    left: char,
    fill: char,
    right: char,
    inner_width: usize,
    title: Option<&str>,
) -> RenderedRow {
    let mut cells = Vec::with_capacity(inner_width.saturating_add(2));
    cells.push(plain_cell(left));
    if let Some(title) = title.filter(|title| !title.is_empty()) {
        let title_text = format!(" {title} ");
        let title_cells = title_text
            .chars()
            .take(inner_width)
            .map(|ch| {
                let mut cell = plain_cell(ch);
                Arc::make_mut(&mut cell.style).bold = true;
                cell
            })
            .collect::<Vec<_>>();
        let remaining = inner_width.saturating_sub(title_cells.len());
        cells.extend(title_cells);
        cells.extend((0..remaining).map(|_| plain_cell(fill)));
    } else {
        cells.extend((0..inner_width).map(|_| plain_cell(fill)));
    }
    cells.push(plain_cell(right));
    rendered_plain_row(cells)
}

fn render_tree_log_rows(
    id: Option<&str>,
    height: Option<usize>,
    max: Option<usize>,
    children: &[MicronPlusNode],
    width: usize,
    context: TreeRenderContext<'_>,
) -> Vec<RenderedRow> {
    let inner_width = width.saturating_sub(2).max(8);
    let body_nodes = widget_augmented_nodes(id, children, context.widgets, false);
    let mut body_rows = render_micronplus_nodes_rows(&body_nodes, inner_width, context);
    let limit = match (max, height) {
        (Some(max), Some(height)) => Some(max.min(height)),
        (Some(max), None) => Some(max),
        (None, Some(height)) => Some(height),
        (None, None) => None,
    };
    if let Some(limit) = limit {
        let start = body_rows.len().saturating_sub(limit);
        body_rows = body_rows.split_off(start);
    }
    boxed_rows(body_rows, inner_width, None, height)
}

fn render_micronplus_column_nodes_rows(
    columns: &[MicronPlusColumnNode],
    width: usize,
    context: TreeRenderContext<'_>,
) -> Vec<RenderedRow> {
    if columns.is_empty() || width == 0 {
        return Vec::new();
    }
    let gutter = DEFAULT_COLUMN_GAP;
    let gutter_total = gutter.len().saturating_mul(columns.len().saturating_sub(1));
    let available = width.saturating_sub(gutter_total).max(columns.len());
    let column_widths = distribute_column_node_widths(columns, available);
    let rendered_columns = columns
        .iter()
        .zip(column_widths.iter())
        .map(|(column, column_width)| {
            let mut rows = Vec::new();
            if let Some(title) = &column.title {
                rows.extend(render_document(
                    &parse_micron(&format!("`F888[{title}]`f")),
                    *column_width,
                ));
            }
            rows.extend(render_micronplus_nodes_rows(
                &column.children,
                *column_width,
                context,
            ));
            rows
        })
        .collect::<Vec<_>>();
    let row_count = rendered_columns
        .iter()
        .map(Vec::len)
        .max()
        .unwrap_or_default();
    let mut rows = Vec::new();

    for row_index in 0..row_count {
        let mut cells = Vec::new();
        for (column_index, column_rows) in rendered_columns.iter().enumerate() {
            if column_index > 0 {
                cells.extend(gutter_cells(gutter));
            }
            let column_width = column_widths[column_index];
            let column_cells = column_rows
                .get(row_index)
                .map(|row| row.cells.as_slice())
                .unwrap_or(&[]);
            cells.extend(pad_or_truncate_cells(column_cells, column_width));
        }
        trim_trailing_plain_spaces(&mut cells);
        rows.push(RenderedRow {
            cells,
            align: Alignment::Left,
            depth: 0,
            base_style: TextStyle::default(),
            wrap: false,
        });
    }

    rows
}

fn distribute_column_node_widths(columns: &[MicronPlusColumnNode], available: usize) -> Vec<usize> {
    let group = MicronPlusColumnGroup {
        columns: columns
            .iter()
            .map(|column| MicronPlusColumnLayout {
                title: column.title.clone(),
                width: column.width,
                weight: column.weight,
                raw_markup: String::new(),
            })
            .collect(),
    };
    distribute_column_widths(&group, available)
}

fn widget_augmented_nodes(
    id: Option<&str>,
    children: &[MicronPlusNode],
    widgets: Option<&MicronPlusWidgetStore>,
    replace_default: bool,
) -> Vec<MicronPlusNode> {
    let Some(state) = id.and_then(|id| widgets.and_then(|store| store.get(id))) else {
        return children.to_vec();
    };
    let mut nodes = if state.replace || replace_default {
        Vec::new()
    } else {
        children.to_vec()
    };
    nodes.extend(widget_items_to_nodes(&state.items));
    nodes
}

fn widget_items_to_nodes(items: &[MicronPlusWidgetItem]) -> Vec<MicronPlusNode> {
    items
        .iter()
        .flat_map(|item| {
            if let Some(markup) = item.markup.as_deref() {
                parse_micronplus_tree(markup).nodes
            } else {
                vec![MicronPlusNode::Text {
                    text: match item.style.as_deref() {
                        Some(style) => status_line(&item.text, style),
                        None => item.text.clone(),
                    },
                }]
            }
        })
        .collect()
}

fn control_node_attrs(
    name: &str,
    width: usize,
    masked: bool,
    value: Option<&str>,
) -> BTreeMap<String, String> {
    let mut attrs = BTreeMap::from([
        ("name".into(), name.to_string()),
        ("width".into(), width.to_string()),
    ]);
    if masked {
        attrs.insert("masked".into(), "true".into());
    }
    if let Some(value) = value {
        attrs.insert("value".into(), value.to_string());
    }
    attrs
}

fn flush_lowered_fragment_rows(
    rows: &mut Vec<RenderedRow>,
    plain_lines: &mut Vec<&str>,
    width: usize,
    widgets: Option<&MicronPlusWidgetStore>,
    field_values: Option<&BTreeMap<String, String>>,
    field_cursor: Option<(&str, usize)>,
) {
    if plain_lines.is_empty() {
        return;
    }
    let lowered = lower_micronplus_markup_inner(
        &plain_lines.join("\n"),
        widgets,
        WidgetMarkupMode::PreserveColumns,
        field_values,
        Some(width.max(1)),
    )
    .markup;
    if !lowered.is_empty() {
        if lowered
            .lines()
            .any(|line| parse_tag(line).is_some_and(|(tag, _)| tag == "columns"))
        {
            rows.extend(render_micronplus_fragment_rows(
                &lowered,
                width,
                widgets,
                field_values,
                field_cursor,
            ));
        } else {
            rows.extend(render_document_with_field_cursor(
                &parse_micron(&lowered),
                width,
                field_cursor.map(|(name, _)| name),
                field_cursor,
            ));
        }
    }
    plain_lines.clear();
}

fn pad_or_truncate_cells(cells: &[Cell], width: usize) -> Vec<Cell> {
    let mut out = cells.iter().take(width).cloned().collect::<Vec<_>>();
    if out.len() < width {
        out.extend(vec![plain_cell(' '); width - out.len()]);
    }
    out
}

fn gutter_cells(gutter: &str) -> Vec<Cell> {
    gutter.chars().map(plain_cell).collect()
}

fn plain_cell(ch: char) -> Cell {
    Cell {
        ch,
        style: default_render_style(),
        link: None,
        control: None,
        cursor: false,
    }
}

fn empty_row() -> RenderedRow {
    rendered_plain_row(Vec::new())
}

fn rendered_plain_row(cells: Vec<Cell>) -> RenderedRow {
    RenderedRow {
        cells,
        align: Alignment::Left,
        depth: 0,
        base_style: TextStyle::default(),
        wrap: false,
    }
}

fn trim_trailing_plain_spaces(cells: &mut Vec<Cell>) {
    while cells.last().is_some_and(|cell| {
        cell.ch == ' ' && cell.link.is_none() && cell.control.is_none() && !cell.cursor
    }) {
        cells.pop();
    }
}

fn find_closing_tag(lines: &[&str], start: usize, tag: &str) -> Option<usize> {
    let closing = format!("[/{tag}]");
    lines
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(index, line)| line.trim().eq_ignore_ascii_case(&closing).then_some(index))
}

fn emit_scrollbox_body(
    output: &mut Vec<String>,
    attrs: &BTreeMap<String, String>,
    body: &[&str],
    widgets: Option<&MicronPlusWidgetStore>,
    widget_markup_mode: WidgetMarkupMode,
) {
    let body_lines = widget_body_lines(attrs, body, widgets, widget_markup_mode);
    let lowered = lower_micronplus_markup_inner(
        &body_lines.join("\n"),
        widgets,
        widget_markup_mode,
        None,
        None,
    )
    .markup;
    let lowered_lines = lowered.lines().map(str::to_string).collect::<Vec<_>>();
    let body_refs = lowered_lines.iter().map(String::as_str).collect::<Vec<_>>();
    let visible_lines = bounded_body_lines(&body_refs, parse_usize_attr(attrs, "height"), false);
    output.extend(visible_lines);
    if let Some(height) = parse_usize_attr(attrs, "height") {
        if body_refs.len() > height {
            output.push("`F888...`f".into());
        }
    }
}

fn emit_log_body(
    output: &mut Vec<String>,
    attrs: &BTreeMap<String, String>,
    body: &[&str],
    widgets: Option<&MicronPlusWidgetStore>,
    widget_markup_mode: WidgetMarkupMode,
) {
    let max_lines = parse_usize_attr(attrs, "max");
    let height = parse_usize_attr(attrs, "height");
    let limit = match (max_lines, height) {
        (Some(max), Some(height)) => Some(max.min(height)),
        (Some(max), None) => Some(max),
        (None, Some(height)) => Some(height),
        (None, None) => None,
    };
    let body_lines = widget_body_lines(attrs, body, widgets, widget_markup_mode);
    let lowered = lower_micronplus_markup_inner(
        &body_lines.join("\n"),
        widgets,
        widget_markup_mode,
        None,
        None,
    )
    .markup;
    let lowered_lines = lowered.lines().map(str::to_string).collect::<Vec<_>>();
    let body_refs = lowered_lines.iter().map(String::as_str).collect::<Vec<_>>();
    output.extend(bounded_body_lines(&body_refs, limit, true));
}

fn widget_body_lines(
    attrs: &BTreeMap<String, String>,
    body: &[&str],
    widgets: Option<&MicronPlusWidgetStore>,
    widget_markup_mode: WidgetMarkupMode,
) -> Vec<String> {
    let widget_state = attrs
        .get("id")
        .and_then(|id| widgets.and_then(|store| store.get(id)));
    let mut body_lines = if widget_state.is_some_and(|state| state.replace) {
        Vec::new()
    } else {
        body.iter().map(|line| (*line).to_string()).collect()
    };
    if let Some(state) = widget_state {
        body_lines.extend(widget_items_to_markup(&state.items, widget_markup_mode));
    }
    body_lines
}

fn render_structured_scrollbox_rows(
    attrs: &BTreeMap<String, String>,
    body: &[&str],
    width: usize,
    widgets: Option<&MicronPlusWidgetStore>,
    field_values: Option<&BTreeMap<String, String>>,
    field_cursor: Option<(&str, usize)>,
) -> Vec<RenderedRow> {
    let mut rows = Vec::new();
    let header = attrs
        .get("title")
        .or_else(|| attrs.get("label"))
        .filter(|title| !title.is_empty())
        .map(|title| section_header(title))
        .unwrap_or_else(section_rule);
    rows.extend(render_document(&parse_micron(&header), width));

    let body_lines = widget_body_lines(attrs, body, widgets, WidgetMarkupMode::PreserveColumns);
    let mut body_rows = render_micronplus_fragment_rows(
        &body_lines.join("\n"),
        width,
        widgets,
        field_values,
        field_cursor,
    );
    let clipped = if let Some(height) = parse_usize_attr(attrs, "height") {
        let clipped = body_rows.len() > height;
        body_rows.truncate(height);
        clipped
    } else {
        false
    };
    rows.extend(body_rows);
    if clipped {
        rows.extend(render_document(&parse_micron("`F888...`f"), width));
    }
    rows.extend(render_document(&parse_micron(&section_rule()), width));
    rows
}

fn render_structured_log_rows(
    attrs: &BTreeMap<String, String>,
    body: &[&str],
    width: usize,
    widgets: Option<&MicronPlusWidgetStore>,
    field_values: Option<&BTreeMap<String, String>>,
    field_cursor: Option<(&str, usize)>,
) -> Vec<RenderedRow> {
    let mut rows = Vec::new();
    let header = attrs
        .get("id")
        .filter(|id| !id.is_empty())
        .map(|id| section_header(&format!("log: {id}")))
        .unwrap_or_else(section_rule);
    rows.extend(render_document(&parse_micron(&header), width));

    let max_lines = parse_usize_attr(attrs, "max");
    let height = parse_usize_attr(attrs, "height");
    let limit = match (max_lines, height) {
        (Some(max), Some(height)) => Some(max.min(height)),
        (Some(max), None) => Some(max),
        (None, Some(height)) => Some(height),
        (None, None) => None,
    };
    let body_lines = widget_body_lines(attrs, body, widgets, WidgetMarkupMode::PreserveColumns);
    let mut body_rows = render_micronplus_fragment_rows(
        &body_lines.join("\n"),
        width,
        widgets,
        field_values,
        field_cursor,
    );
    if let Some(limit) = limit {
        let start = body_rows.len().saturating_sub(limit);
        body_rows = body_rows.split_off(start);
    }
    rows.extend(body_rows);
    rows.extend(render_document(&parse_micron(&section_rule()), width));
    rows
}

fn render_structured_container_rows(
    tag: &str,
    attrs: &BTreeMap<String, String>,
    body: &[&str],
    width: usize,
    widgets: Option<&MicronPlusWidgetStore>,
    field_values: Option<&BTreeMap<String, String>>,
    field_cursor: Option<(&str, usize)>,
) -> Vec<RenderedRow> {
    let mut rows = Vec::new();
    let header = attrs
        .get("title")
        .or_else(|| attrs.get("label"))
        .filter(|title| !title.is_empty())
        .map(|title| {
            if tag == "window" {
                let mut lines = vec![format!(">{title}")];
                lines.push(section_header(title));
                lines.join("\n")
            } else {
                section_header(title)
            }
        })
        .unwrap_or_else(section_rule);
    rows.extend(render_document(&parse_micron(&header), width));
    rows.extend(render_micronplus_fragment_rows(
        &body.join("\n"),
        width,
        widgets,
        field_values,
        field_cursor,
    ));
    rows.extend(render_document(&parse_micron(&section_rule()), width));
    rows
}

fn widget_items_to_markup(
    items: &[MicronPlusWidgetItem],
    widget_markup_mode: WidgetMarkupMode,
) -> Vec<String> {
    items
        .iter()
        .flat_map(|item| match item.markup.as_deref() {
            Some(markup)
                if widget_markup_mode == WidgetMarkupMode::PreserveColumns
                    && markup
                        .lines()
                        .any(|line| parse_tag(line).is_some_and(|(tag, _)| tag == "columns")) =>
            {
                dedent_micronplus_source(markup)
                    .lines()
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            }
            Some(markup) => lower_micronplus_markup(markup)
                .markup
                .lines()
                .map(str::to_string)
                .collect::<Vec<_>>(),
            None => vec![match item.style.as_deref() {
                Some(style) => status_line(&item.text, style),
                None => item.text.clone(),
            }],
        })
        .collect()
}

#[derive(Default)]
struct MicronPlusTreeBudget {
    nodes: usize,
    columns: usize,
}

impl MicronPlusTreeBudget {
    fn admit_node(&mut self, depth: usize) -> Result<(), String> {
        if depth > MICRONPLUS_TREE_MAX_DEPTH {
            return Err(format!(
                "MicronPlus tree exceeds depth {MICRONPLUS_TREE_MAX_DEPTH}"
            ));
        }
        self.nodes = self.nodes.saturating_add(1);
        if self.nodes > MICRONPLUS_TREE_MAX_NODES {
            return Err(format!(
                "MicronPlus tree exceeds {MICRONPLUS_TREE_MAX_NODES} nodes"
            ));
        }
        Ok(())
    }

    fn admit_column(&mut self) -> Result<(), String> {
        self.columns = self.columns.saturating_add(1);
        if self.columns > MICRONPLUS_TREE_MAX_COLUMNS {
            return Err(format!(
                "MicronPlus tree exceeds {MICRONPLUS_TREE_MAX_COLUMNS} columns"
            ));
        }
        Ok(())
    }
}

fn parse_micronplus_nodes(
    lines: &[&str],
    index: &mut usize,
    end_tag: Option<&str>,
    depth: usize,
    budget: &mut MicronPlusTreeBudget,
) -> Result<Vec<MicronPlusNode>, String> {
    let mut nodes = Vec::new();
    while *index < lines.len() {
        let line = lines[*index];
        if let Some(closing) = closing_tag_name(line) {
            *index += 1;
            if end_tag.is_some_and(|end_tag| closing == end_tag) {
                break;
            }
            continue;
        }

        let Some((tag, attrs)) = parse_tag(line) else {
            budget.admit_node(depth)?;
            nodes.push(MicronPlusNode::Text {
                text: line.to_string(),
            });
            *index += 1;
            continue;
        };

        match tag.as_str() {
            "window" => {
                *index += 1;
                budget.admit_node(depth)?;
                nodes.push(MicronPlusNode::Window {
                    title: non_empty_attr(&attrs, "title"),
                    children: parse_micronplus_nodes(
                        lines,
                        index,
                        Some("window"),
                        depth + 1,
                        budget,
                    )?,
                });
            }
            "box" => {
                *index += 1;
                budget.admit_node(depth)?;
                nodes.push(MicronPlusNode::Box {
                    title: non_empty_attr(&attrs, "title")
                        .or_else(|| non_empty_attr(&attrs, "label")),
                    children: parse_micronplus_nodes(lines, index, Some("box"), depth + 1, budget)?,
                });
            }
            "columns" => {
                *index += 1;
                budget.admit_node(depth)?;
                nodes.push(MicronPlusNode::Columns {
                    columns: parse_micronplus_column_nodes(lines, index, depth + 1, budget)?,
                });
            }
            "scrollbox" => {
                *index += 1;
                budget.admit_node(depth)?;
                nodes.push(MicronPlusNode::Scrollbox {
                    id: non_empty_attr(&attrs, "id"),
                    title: non_empty_attr(&attrs, "title")
                        .or_else(|| non_empty_attr(&attrs, "label")),
                    height: parse_usize_attr(&attrs, "height"),
                    max: parse_usize_attr(&attrs, "max"),
                    children: parse_micronplus_nodes(
                        lines,
                        index,
                        Some("scrollbox"),
                        depth + 1,
                        budget,
                    )?,
                });
            }
            "log" => {
                *index += 1;
                budget.admit_node(depth)?;
                nodes.push(MicronPlusNode::Log {
                    id: non_empty_attr(&attrs, "id"),
                    height: parse_usize_attr(&attrs, "height"),
                    max: parse_usize_attr(&attrs, "max"),
                    children: parse_micronplus_nodes(lines, index, Some("log"), depth + 1, budget)?,
                });
            }
            "input" | "textbox" => {
                let Some(fields) = split_fields(attrs.get("fields").map(String::as_str)) else {
                    budget.admit_node(depth)?;
                    nodes.push(MicronPlusNode::Text {
                        text: line.to_string(),
                    });
                    *index += 1;
                    continue;
                };
                budget.admit_node(depth)?;
                nodes.push(MicronPlusNode::Input {
                    name: attrs
                        .get("name")
                        .filter(|name| !name.is_empty())
                        .cloned()
                        .unwrap_or_else(|| tag.clone()),
                    label: non_empty_attr(&attrs, "label")
                        .or_else(|| non_empty_attr(&attrs, "title")),
                    width: explicit_control_width(&attrs),
                    masked: attrs
                        .get("masked")
                        .or_else(|| attrs.get("password"))
                        .or_else(|| attrs.get("secret"))
                        .is_some_and(|value| !matches!(value.as_str(), "false" | "0" | "no")),
                    submit: non_empty_attr(&attrs, "submit"),
                    event: non_empty_attr(&attrs, "event"),
                    action: non_empty_attr(&attrs, "action"),
                    fields,
                    value: non_empty_attr(&attrs, "value")
                        .or_else(|| non_empty_attr(&attrs, "default"))
                        .or_else(|| non_empty_attr(&attrs, "placeholder")),
                });
                *index += 1;
            }
            "button" => {
                let Some(fields) = split_fields(attrs.get("fields").map(String::as_str)) else {
                    budget.admit_node(depth)?;
                    nodes.push(MicronPlusNode::Text {
                        text: line.to_string(),
                    });
                    *index += 1;
                    continue;
                };
                budget.admit_node(depth)?;
                nodes.push(MicronPlusNode::Button {
                    label: attrs
                        .get("label")
                        .filter(|label| !label.is_empty())
                        .cloned()
                        .unwrap_or_else(|| "Button".into()),
                    event: non_empty_attr(&attrs, "event"),
                    action: non_empty_attr(&attrs, "action"),
                    fields,
                });
                *index += 1;
            }
            "status" => {
                budget.admit_node(depth)?;
                nodes.push(MicronPlusNode::Status {
                    id: non_empty_attr(&attrs, "id"),
                    text: attrs.get("text").cloned().unwrap_or_default(),
                    style: non_empty_attr(&attrs, "style"),
                });
                *index += 1;
            }
            "live" => {
                let Some(fields) = split_fields(attrs.get("fields").map(String::as_str)) else {
                    budget.admit_node(depth)?;
                    nodes.push(MicronPlusNode::Text {
                        text: line.to_string(),
                    });
                    *index += 1;
                    continue;
                };
                budget.admit_node(depth)?;
                nodes.push(MicronPlusNode::Live {
                    id: attrs.get("id").cloned().unwrap_or_else(|| "live".into()),
                    src: attrs.get("src").cloned().unwrap_or_default(),
                    refresh_secs: attrs
                        .get("refresh")
                        .and_then(|value| parse_refresh_secs(value)),
                    loop_count: attrs.get("loop").and_then(|value| parse_loop_count(value)),
                    fields,
                    children: Vec::new(),
                });
                *index += 1;
            }
            _ => {
                budget.admit_node(depth)?;
                nodes.push(MicronPlusNode::Text {
                    text: line.to_string(),
                });
                *index += 1;
            }
        }
    }
    Ok(nodes)
}

fn parse_micronplus_column_nodes(
    lines: &[&str],
    index: &mut usize,
    depth: usize,
    budget: &mut MicronPlusTreeBudget,
) -> Result<Vec<MicronPlusColumnNode>, String> {
    let mut columns = Vec::new();
    while *index < lines.len() {
        if let Some(closing) = closing_tag_name(lines[*index]) {
            *index += 1;
            if closing == "columns" {
                break;
            }
            continue;
        }
        let Some((tag, attrs)) = parse_tag(lines[*index]) else {
            *index += 1;
            continue;
        };
        if tag != "column" {
            *index += 1;
            continue;
        }
        *index += 1;
        budget.admit_column()?;
        columns.push(MicronPlusColumnNode {
            title: non_empty_attr(&attrs, "title").or_else(|| non_empty_attr(&attrs, "label")),
            width: parse_usize_attr(&attrs, "width"),
            weight: parse_usize_attr(&attrs, "weight").unwrap_or(1).max(1),
            children: parse_micronplus_nodes(lines, index, Some("column"), depth + 1, budget)?,
        });
    }
    Ok(columns)
}

fn apply_micronplus_nodes_partial(
    nodes: &mut [MicronPlusNode],
    slot: &str,
    content: &[MicronPlusNode],
) -> bool {
    let mut changed = false;
    for node in nodes {
        match node {
            MicronPlusNode::Live { id, children, .. } if id == slot => {
                *children = content.to_vec();
                changed = true;
            }
            MicronPlusNode::Window { children, .. }
            | MicronPlusNode::Box { children, .. }
            | MicronPlusNode::Scrollbox { children, .. }
            | MicronPlusNode::Log { children, .. }
            | MicronPlusNode::Live { children, .. } => {
                changed |= apply_micronplus_nodes_partial(children, slot, content);
            }
            MicronPlusNode::Columns { columns } => {
                for column in columns {
                    changed |= apply_micronplus_nodes_partial(&mut column.children, slot, content);
                }
            }
            MicronPlusNode::Text { .. }
            | MicronPlusNode::Input { .. }
            | MicronPlusNode::Button { .. }
            | MicronPlusNode::Status { .. } => {}
        }
    }
    changed
}

fn closing_tag_name(line: &str) -> Option<String> {
    let inner = line
        .trim()
        .strip_prefix("[/")?
        .strip_suffix(']')?
        .trim()
        .to_ascii_lowercase();
    (!inner.is_empty()).then_some(inner)
}

fn non_empty_attr(attrs: &BTreeMap<String, String>, name: &str) -> Option<String> {
    attrs.get(name).filter(|value| !value.is_empty()).cloned()
}

fn bounded_body_lines(body: &[&str], limit: Option<usize>, from_tail: bool) -> Vec<String> {
    let Some(limit) = limit else {
        return body.iter().map(|line| (*line).to_string()).collect();
    };
    if limit == 0 {
        return Vec::new();
    }
    if from_tail {
        let start = body.len().saturating_sub(limit);
        body[start..]
            .iter()
            .map(|line| (*line).to_string())
            .collect()
    } else {
        body.iter()
            .take(limit)
            .map(|line| (*line).to_string())
            .collect()
    }
}

fn parse_usize_attr(attrs: &BTreeMap<String, String>, name: &str) -> Option<usize> {
    attrs.get(name)?.parse::<usize>().ok()
}

fn supported_closing_tag(line: &str) -> Option<&'static str> {
    match line.trim().to_ascii_lowercase().as_str() {
        "[/window]" => Some("window"),
        "[/box]" => Some("box"),
        "[/columns]" => Some("columns"),
        "[/column]" => Some("column"),
        "[/scrollbox]" => Some("scrollbox"),
        "[/log]" => Some("log"),
        _ => None,
    }
}

fn emit_closing_tag(output: &mut Vec<String>, tag: &str) {
    match tag {
        "window" | "box" => output.push(section_rule()),
        "column" => output.push(String::new()),
        _ => {}
    }
}

fn section_header(title: &str) -> String {
    format!("`F555+-- {title} --+`f")
}

fn section_rule() -> String {
    "`F555+------------------------------+`f".into()
}

fn column_separator() -> String {
    "`F555------------------------------`f".into()
}

fn parse_tag(line: &str) -> Option<(String, BTreeMap<String, String>)> {
    let line = line.trim();
    let inner = line.strip_prefix('[')?.strip_suffix(']')?;
    if inner.starts_with('/') {
        return None;
    }
    let mut parts = inner.splitn(2, char::is_whitespace);
    let tag = parts.next()?.to_ascii_lowercase();
    let attrs = parse_attrs_checked(parts.next().unwrap_or_default()).ok()?;
    Some((tag, attrs))
}

fn parse_tag_for_lowering(
    line: &str,
    line_number: usize,
    diagnostics: &mut Vec<String>,
) -> Option<(String, BTreeMap<String, String>)> {
    let line = line.trim();
    let inner = line.strip_prefix('[')?.strip_suffix(']')?;
    if inner.starts_with('/') {
        return None;
    }
    let mut parts = inner.splitn(2, char::is_whitespace);
    let tag = parts.next()?.to_ascii_lowercase();
    match parse_attrs_checked(parts.next().unwrap_or_default()) {
        Ok(attrs) => Some((tag, attrs)),
        Err(error) => {
            diagnostics.push(format!(
                "MicronPlus syntax warning line={line_number}: {error}. Content: {}",
                line.chars().take(160).collect::<String>()
            ));
            None
        }
    }
}

fn warn_unknown_attrs(
    tag: &str,
    attrs: &BTreeMap<String, String>,
    line_number: usize,
    diagnostics: &mut Vec<String>,
) {
    let allowed = allowed_attrs(tag);
    if allowed.is_empty() {
        return;
    }
    let unknown = attrs
        .keys()
        .filter(|key| !allowed.contains(&key.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if unknown.is_empty() {
        return;
    }
    diagnostics.push(format!(
        "MicronPlus syntax warning line={line_number}: [{tag}] ignores unsupported attrs: {}",
        unknown.join(", ")
    ));
}

fn allowed_attrs(tag: &str) -> &'static [&'static str] {
    match tag {
        "window" => &["title"],
        "box" => &["title", "label"],
        "columns" => &[],
        "column" => &["title", "label", "width", "weight"],
        "scrollbox" | "log" => &["id", "title", "label", "height", "max"],
        "input" | "textbox" => &[
            "name",
            "label",
            "title",
            "width",
            "cols",
            "size",
            "value",
            "default",
            "placeholder",
            "masked",
            "password",
            "secret",
            "submit",
            "event",
            "action",
            "fields",
        ],
        "button" => &["label", "event", "action", "fields"],
        "status" => &["id", "text", "style"],
        "live" => &["id", "src", "refresh", "loop", "fields"],
        "event" | "widget-event" => &[
            "name", "target", "text", "message", "value", "style", "markup",
        ],
        _ => &[],
    }
}

fn parse_widget_event_line(line: &str) -> Option<MicronPlusWidgetEvent> {
    let (tag, attrs) = parse_tag(line)?;
    if tag != "event" && tag != "widget-event" {
        return None;
    }

    let name = attrs
        .get("name")
        .or_else(|| attrs.get("target"))
        .map(String::as_str)?;
    let markup = attrs
        .get("markup")
        .filter(|markup| !markup.is_empty())
        .cloned();
    let text = attrs
        .get("text")
        .or_else(|| attrs.get("message"))
        .or_else(|| attrs.get("value"))
        .cloned()
        .unwrap_or_default();
    let style = attrs
        .get("style")
        .filter(|style| !style.is_empty())
        .cloned();

    if let Some(id) = name.strip_prefix("status.update.") {
        return (!id.is_empty()).then_some(MicronPlusWidgetEvent::StatusUpdate {
            id: id.to_string(),
            text,
            style,
        });
    }

    if let Some(id) = name.strip_prefix("scrollbox.set.") {
        return (!id.is_empty()).then_some(MicronPlusWidgetEvent::ScrollboxSet {
            id: id.to_string(),
            items: vec![MicronPlusWidgetItem {
                text: text.clone(),
                style: style.clone(),
                markup: markup.clone(),
            }],
        });
    }

    if let Some(id) = name.strip_prefix("scrollbox.append.") {
        return (!id.is_empty()).then_some(MicronPlusWidgetEvent::ScrollboxAppend {
            id: id.to_string(),
            items: vec![MicronPlusWidgetItem {
                text: text.clone(),
                style: style.clone(),
                markup: markup.clone(),
            }],
        });
    }

    if let Some(id) = name.strip_prefix("log.append.") {
        return (!id.is_empty()).then_some(MicronPlusWidgetEvent::LogAppend {
            id: id.to_string(),
            items: vec![MicronPlusWidgetItem {
                text,
                style,
                markup,
            }],
        });
    }

    None
}

fn parse_attrs_checked(input: &str) -> Result<BTreeMap<String, String>, String> {
    if input.len() > MICRONPLUS_SOURCE_LINE_MAX_BYTES {
        return Err(format!(
            "attribute source exceeds {MICRONPLUS_SOURCE_LINE_MAX_BYTES} bytes"
        ));
    }
    let mut attrs = BTreeMap::new();
    let chars = input.chars().collect::<Vec<_>>();
    let mut index = 0;
    let mut parsed_items = 0usize;
    let mut owned_bytes = 0usize;
    while index < chars.len() {
        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }
        let key_start = index;
        while index < chars.len() && !chars[index].is_whitespace() && chars[index] != '=' {
            index += 1;
        }
        if key_start == index {
            break;
        }
        let key = chars[key_start..index].iter().collect::<String>();
        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }
        let value = if chars.get(index) != Some(&'=') {
            "true".to_string()
        } else {
            index += 1;
            while index < chars.len() && chars[index].is_whitespace() {
                index += 1;
            }
            if chars.get(index) == Some(&'"') {
                index += 1;
                let value_start = index;
                while index < chars.len() && chars[index] != '"' {
                    index += 1;
                }
                if index >= chars.len() {
                    return Err(format!("unterminated quoted attribute value for {key}"));
                }
                let value = chars[value_start..index].iter().collect::<String>();
                index += 1;
                value
            } else if chars.get(index) == Some(&'\'') {
                index += 1;
                let value_start = index;
                while index < chars.len() && chars[index] != '\'' {
                    index += 1;
                }
                if index >= chars.len() {
                    return Err(format!("unterminated quoted attribute value for {key}"));
                }
                let value = chars[value_start..index].iter().collect::<String>();
                index += 1;
                value
            } else {
                let value_start = index;
                while index < chars.len() && !chars[index].is_whitespace() {
                    index += 1;
                }
                chars[value_start..index].iter().collect::<String>()
            }
        };
        parsed_items = parsed_items.saturating_add(1);
        if parsed_items > MICRONPLUS_ATTRIBUTE_MAX_ITEMS {
            return Err(format!(
                "attribute list exceeds {MICRONPLUS_ATTRIBUTE_MAX_ITEMS} items"
            ));
        }
        if key.len() > MICRONPLUS_ATTRIBUTE_KEY_MAX_BYTES {
            return Err(format!(
                "attribute key exceeds {MICRONPLUS_ATTRIBUTE_KEY_MAX_BYTES} bytes"
            ));
        }
        if value.len() > MICRONPLUS_ATTRIBUTE_VALUE_MAX_BYTES {
            return Err(format!(
                "attribute value exceeds {MICRONPLUS_ATTRIBUTE_VALUE_MAX_BYTES} bytes"
            ));
        }
        owned_bytes = owned_bytes
            .checked_add(key.len().saturating_add(value.len()))
            .ok_or_else(|| "attribute byte accounting overflow".to_string())?;
        if owned_bytes > MICRONPLUS_ATTRIBUTE_MAX_OWNED_BYTES {
            return Err(format!(
                "attribute list exceeds {MICRONPLUS_ATTRIBUTE_MAX_OWNED_BYTES} owned bytes"
            ));
        }
        attrs.insert(key, value);
    }
    Ok(attrs)
}

fn repair_micronplus_tag_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.starts_with("ive ") && trimmed.contains("src=") {
        return Some(if trimmed.ends_with(']') {
            format!("[l{trimmed}")
        } else {
            format!("[l{trimmed}]")
        });
    }
    None
}

fn admitted_micronplus_source(markup: &str) -> Result<String, String> {
    validate_micronplus_source(markup)?;
    Ok(dedent_micronplus_source(markup))
}

fn validate_micronplus_source(markup: &str) -> Result<(), String> {
    if markup.len() > MICRONPLUS_SOURCE_MAX_BYTES {
        return Err(format!(
            "MicronPlus source exceeds {MICRONPLUS_SOURCE_MAX_BYTES} bytes"
        ));
    }
    let mut lines = 0usize;
    for line in markup.lines() {
        lines = lines.saturating_add(1);
        if lines > MICRONPLUS_SOURCE_MAX_LINES {
            return Err(format!(
                "MicronPlus source exceeds {MICRONPLUS_SOURCE_MAX_LINES} lines"
            ));
        }
        if line.len() > MICRONPLUS_SOURCE_LINE_MAX_BYTES {
            return Err(format!(
                "MicronPlus source line exceeds {MICRONPLUS_SOURCE_LINE_MAX_BYTES} bytes"
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct MicronPlusTreeStats {
    nodes: usize,
    columns: usize,
    owned_bytes: usize,
    max_depth: usize,
}

fn validate_micronplus_tree_owned_bytes(tree: &MicronPlusWidgetTree) -> Result<(), String> {
    micronplus_tree_stats(tree).map(|_| ())
}

fn micronplus_tree_stats(tree: &MicronPlusWidgetTree) -> Result<MicronPlusTreeStats, String> {
    let mut stats = MicronPlusTreeStats::default();
    let mut pending = tree
        .nodes
        .iter()
        .map(|node| (node, 1usize))
        .collect::<Vec<_>>();
    while let Some((node, depth)) = pending.pop() {
        stats.nodes = stats.nodes.saturating_add(1);
        stats.max_depth = stats.max_depth.max(depth);
        if stats.nodes > MICRONPLUS_TREE_MAX_NODES {
            return Err(format!(
                "MicronPlus tree exceeds {MICRONPLUS_TREE_MAX_NODES} nodes"
            ));
        }
        if depth > MICRONPLUS_TREE_MAX_DEPTH {
            return Err(format!(
                "MicronPlus tree exceeds depth {MICRONPLUS_TREE_MAX_DEPTH}"
            ));
        }
        let mut strings = Vec::new();
        match node {
            MicronPlusNode::Text { text } => strings.push(text.as_str()),
            MicronPlusNode::Window { title, children }
            | MicronPlusNode::Box { title, children } => {
                strings.extend(title.iter().map(String::as_str));
                pending.extend(children.iter().map(|child| (child, depth + 1)));
            }
            MicronPlusNode::Columns { columns } => {
                stats.columns = stats.columns.saturating_add(columns.len());
                if stats.columns > MICRONPLUS_TREE_MAX_COLUMNS {
                    return Err(format!(
                        "MicronPlus tree exceeds {MICRONPLUS_TREE_MAX_COLUMNS} columns"
                    ));
                }
                for column in columns {
                    strings.extend(column.title.iter().map(String::as_str));
                    pending.extend(column.children.iter().map(|child| (child, depth + 2)));
                }
            }
            MicronPlusNode::Scrollbox {
                id,
                title,
                children,
                ..
            } => {
                strings.extend(id.iter().map(String::as_str));
                strings.extend(title.iter().map(String::as_str));
                pending.extend(children.iter().map(|child| (child, depth + 1)));
            }
            MicronPlusNode::Log { id, children, .. } => {
                strings.extend(id.iter().map(String::as_str));
                pending.extend(children.iter().map(|child| (child, depth + 1)));
            }
            MicronPlusNode::Input {
                name,
                label,
                submit,
                event,
                action,
                fields,
                value,
                ..
            } => {
                strings.push(name);
                strings.extend(label.iter().map(String::as_str));
                strings.extend(submit.iter().map(String::as_str));
                strings.extend(event.iter().map(String::as_str));
                strings.extend(action.iter().map(String::as_str));
                strings.extend(fields.iter().map(String::as_str));
                strings.extend(value.iter().map(String::as_str));
            }
            MicronPlusNode::Button {
                label,
                event,
                action,
                fields,
            } => {
                strings.push(label);
                strings.extend(event.iter().map(String::as_str));
                strings.extend(action.iter().map(String::as_str));
                strings.extend(fields.iter().map(String::as_str));
            }
            MicronPlusNode::Status { id, text, style } => {
                strings.extend(id.iter().map(String::as_str));
                strings.push(text);
                strings.extend(style.iter().map(String::as_str));
            }
            MicronPlusNode::Live {
                id,
                src,
                fields,
                children,
                ..
            } => {
                strings.push(id);
                strings.push(src);
                strings.extend(fields.iter().map(String::as_str));
                pending.extend(children.iter().map(|child| (child, depth + 1)));
            }
        }
        for string in strings {
            stats.owned_bytes = stats
                .owned_bytes
                .checked_add(string.len())
                .ok_or_else(|| "MicronPlus tree byte accounting overflow".to_string())?;
            if stats.owned_bytes > MICRONPLUS_TREE_MAX_OWNED_BYTES {
                return Err(format!(
                    "MicronPlus tree exceeds {MICRONPLUS_TREE_MAX_OWNED_BYTES} owned bytes"
                ));
            }
        }
    }
    Ok(stats)
}

fn micronplus_live_matches(tree: &MicronPlusWidgetTree, slot: &str) -> (usize, usize) {
    let mut matches = 0usize;
    let mut deepest = 0usize;
    let mut pending = tree
        .nodes
        .iter()
        .map(|node| (node, 1usize))
        .collect::<Vec<_>>();
    while let Some((node, depth)) = pending.pop() {
        match node {
            MicronPlusNode::Window { children, .. }
            | MicronPlusNode::Box { children, .. }
            | MicronPlusNode::Scrollbox { children, .. }
            | MicronPlusNode::Log { children, .. } => {
                pending.extend(children.iter().map(|child| (child, depth + 1)));
            }
            MicronPlusNode::Columns { columns } => {
                for column in columns {
                    pending.extend(column.children.iter().map(|child| (child, depth + 2)));
                }
            }
            MicronPlusNode::Live { id, children, .. } => {
                if id == slot {
                    matches = matches.saturating_add(1);
                    deepest = deepest.max(depth);
                }
                pending.extend(children.iter().map(|child| (child, depth + 1)));
            }
            MicronPlusNode::Text { .. }
            | MicronPlusNode::Input { .. }
            | MicronPlusNode::Button { .. }
            | MicronPlusNode::Status { .. } => {}
        }
    }
    (matches, deepest)
}

fn dedent_micronplus_source(markup: &str) -> String {
    let source = markup.trim_matches('\n');
    let min_indent = source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.chars()
                .take_while(|ch| *ch == ' ' || *ch == '\t')
                .count()
        })
        .min()
        .unwrap_or(0);
    if min_indent == 0 {
        return source.to_string();
    }
    source
        .lines()
        .map(|line| {
            if line.trim().is_empty() {
                ""
            } else {
                line.char_indices()
                    .nth(min_indent)
                    .map(|(index, _)| &line[index..])
                    .unwrap_or("")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn cleanup_lowered_lines(lines: Vec<String>) -> String {
    let mut cleaned = Vec::new();
    let mut blank_run = 0usize;
    for line in lines {
        let line = line.trim_end().to_string();
        if line.is_empty() {
            blank_run = blank_run.saturating_add(1);
            if blank_run <= 1 {
                cleaned.push(String::new());
            }
            continue;
        }
        blank_run = 0;
        cleaned.push(line);
    }
    while cleaned.first().is_some_and(|line| line.is_empty()) {
        cleaned.remove(0);
    }
    while cleaned.last().is_some_and(|line| line.is_empty()) {
        cleaned.pop();
    }
    cleaned.join("\n")
}

fn split_fields(raw: Option<&str>) -> Option<Vec<String>> {
    collect_bounded_link_fields(
        raw.unwrap_or_default()
            .split(['|', ','])
            .map(str::trim)
            .filter(|field| !field.is_empty()),
    )
}

fn parse_refresh_secs(value: &str) -> Option<u64> {
    value
        .parse::<f64>()
        .ok()
        .map(|seconds| seconds.max(1.0).ceil() as u64)
}

fn parse_loop_count(value: &str) -> Option<u32> {
    value.parse::<i64>().ok().map(|count| count.max(0) as u32)
}

fn partial_line(
    src: &str,
    refresh_secs: Option<u64>,
    loop_count: Option<u32>,
    id: &str,
    fields: &[String],
) -> String {
    let mut descriptor = format!("`{{{src}");
    if let Some(refresh_secs) = refresh_secs {
        descriptor.push('`');
        descriptor.push_str(&refresh_secs.to_string());
    }
    let mut params = fields.to_vec();
    params.push(format!("pid={id}"));
    if let Some(loop_count) = loop_count {
        params.push(format!("loop={loop_count}"));
    }
    if !params.is_empty() {
        if refresh_secs.is_none() {
            descriptor.push('`');
        }
        descriptor.push('`');
        descriptor.push_str(&params.join("|"));
    }
    descriptor.push('}');
    descriptor
}

fn link_line(label: &str, target: &str, fields: &[String]) -> String {
    if fields.is_empty() {
        format!("`[{label}`{target}]")
    } else {
        format!("`[{label}`{target}`{}]", fields.join("|"))
    }
}

pub fn micronplus_event_target(event: &str) -> String {
    format!("#micronplus-event:{event}")
}

pub fn micronplus_event_from_target(target: &str) -> Option<&str> {
    target.strip_prefix("#micronplus-event:")
}

pub fn micronplus_control_binding_for_field(
    layout: &MicronPlusLayout,
    name: &str,
) -> Option<MicronPlusControlBinding> {
    for window in &layout.windows {
        for group in &window.column_groups {
            for column in &group.columns {
                for line in column.raw_markup.lines() {
                    let Some((tag, attrs)) = parse_tag(line) else {
                        continue;
                    };
                    if !matches!(tag.as_str(), "input" | "textbox") {
                        continue;
                    }
                    if attrs.get("name").map(String::as_str) != Some(name) {
                        continue;
                    }
                    let event = attrs
                        .get("event")
                        .filter(|event| !event.is_empty())
                        .cloned()
                        .or_else(|| {
                            attrs
                                .get("action")
                                .filter(|action| !action.is_empty())
                                .map(|_| "input.submit".to_string())
                        });
                    let Some(event) = event else {
                        continue;
                    };
                    return Some(MicronPlusControlBinding {
                        source: tag,
                        name: name.to_string(),
                        event,
                        action: attrs
                            .get("action")
                            .filter(|action| !action.is_empty())
                            .cloned(),
                        submit: attrs
                            .get("submit")
                            .filter(|submit| !submit.is_empty())
                            .cloned(),
                        fields: split_fields(attrs.get("fields").map(String::as_str))?,
                    });
                }
            }
        }
    }
    None
}

fn explicit_control_width(attrs: &BTreeMap<String, String>) -> Option<usize> {
    attrs
        .get("width")
        .or_else(|| attrs.get("cols"))
        .or_else(|| attrs.get("size"))
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|width| *width > 0)
        .map(|width| width.min(MAX_EXPLICIT_CONTROL_WIDTH))
}

fn control_line(attrs: &BTreeMap<String, String>, name: &str, fallback_width: usize) -> String {
    let width = explicit_control_width(attrs).unwrap_or_else(|| fallback_width.max(1));
    let value = attrs
        .get("value")
        .or_else(|| attrs.get("default"))
        .or_else(|| attrs.get("placeholder"))
        .cloned()
        .unwrap_or_default();
    let masked = attrs
        .get("masked")
        .or_else(|| attrs.get("password"))
        .or_else(|| attrs.get("secret"))
        .is_some_and(|value| !matches!(value.as_str(), "false" | "0" | "no"));
    let mask = if masked { "!" } else { "" };
    format!("`<{mask}{width}|{name}`{value}>")
}

fn status_line(text: &str, style: &str) -> String {
    let color = match style {
        "success" | "ok" => "0f0",
        "warning" | "warn" => "ff0",
        "error" | "danger" => "f55",
        "muted" | "dim" | "disabled" => "888",
        "accent" | "primary" => "f8f",
        "notice" | "info" => "6cf",
        _ => "6cf",
    };
    format!("`F{color}{text}`f")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::micron::fixtures::{render_markup_report, REGRESSION_WIDTHS};
    use std::path::PathBuf;

    fn nested_boxes(count: usize) -> String {
        let mut markup = "[box]\n".repeat(count);
        markup.push_str("leaf\n");
        markup.push_str(&"[/box]\n".repeat(count));
        markup
    }

    fn columns(count: usize) -> String {
        let mut markup = String::from("[columns]\n");
        for _ in 0..count {
            markup.push_str("[column]\ncontent\n[/column]\n");
        }
        markup.push_str("[/columns]\n");
        markup
    }

    #[test]
    fn micronplus_tree_enforces_node_and_depth_budgets() {
        assert!(try_parse_micronplus_tree(&nested_boxes(MICRONPLUS_TREE_MAX_DEPTH - 1)).is_ok());
        assert!(
            try_parse_micronplus_tree(&nested_boxes(MICRONPLUS_TREE_MAX_DEPTH))
                .is_err_and(|error| error.contains("exceeds depth"))
        );

        let exact_nodes = "text\n".repeat(MICRONPLUS_TREE_MAX_NODES);
        assert_eq!(
            try_parse_micronplus_tree(&exact_nodes)
                .expect("exact node ceiling")
                .nodes
                .len(),
            MICRONPLUS_TREE_MAX_NODES
        );
        let excessive_nodes = "text\n".repeat(MICRONPLUS_TREE_MAX_NODES + 1);
        assert!(try_parse_micronplus_tree(&excessive_nodes)
            .is_err_and(|error| error.contains("exceeds 8192 nodes")));
    }

    #[test]
    fn micronplus_layout_enforces_column_budget() {
        let exact = try_extract_micronplus_layout(&columns(MICRONPLUS_LAYOUT_MAX_COLUMNS))
            .expect("exact column ceiling");
        assert_eq!(exact.windows[0].column_groups[0].columns.len(), 512);

        assert!(
            try_extract_micronplus_layout(&columns(MICRONPLUS_LAYOUT_MAX_COLUMNS + 1))
                .is_err_and(|error| error.contains("exceeds 512 columns"))
        );
    }

    #[test]
    fn micronplus_source_and_attribute_preflight_reject_before_structural_parsing() {
        let oversized_line = "x".repeat(MICRONPLUS_SOURCE_LINE_MAX_BYTES + 1);
        assert!(try_parse_micronplus_tree(&oversized_line)
            .is_err_and(|error| error.contains("source line")));
        let lowered = lower_micronplus_markup(&oversized_line);
        assert_eq!(lowered.markup, oversized_line);
        assert_eq!(lowered.diagnostics.len(), 1);

        let exact_attrs = (0..MICRONPLUS_ATTRIBUTE_MAX_ITEMS)
            .map(|index| format!("key{index}=value"))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            parse_attrs_checked(&exact_attrs)
                .expect("exact attribute ceiling")
                .len(),
            MICRONPLUS_ATTRIBUTE_MAX_ITEMS
        );
        let excessive_attrs = format!("{exact_attrs} one_more=value");
        assert!(parse_attrs_checked(&excessive_attrs)
            .is_err_and(|error| error.contains("exceeds 64 items")));
    }

    #[test]
    fn micronplus_retained_string_validators_accept_exact_and_reject_next_byte() {
        let mut tree = MicronPlusWidgetTree {
            nodes: vec![MicronPlusNode::Text {
                text: "x".repeat(MICRONPLUS_TREE_MAX_OWNED_BYTES),
            }],
        };
        assert_eq!(validate_micronplus_tree_owned_bytes(&tree), Ok(()));
        let MicronPlusNode::Text { text } = &mut tree.nodes[0] else {
            panic!("text fixture");
        };
        text.push('x');
        assert!(validate_micronplus_tree_owned_bytes(&tree).is_err());

        let mut layout = MicronPlusLayout {
            windows: vec![MicronPlusWindowLayout {
                title: None,
                column_groups: vec![MicronPlusColumnGroup {
                    columns: vec![MicronPlusColumnLayout {
                        title: None,
                        width: None,
                        weight: 1,
                        raw_markup: "x".repeat(MICRONPLUS_LAYOUT_MAX_OWNED_BYTES),
                    }],
                }],
            }],
        };
        assert_eq!(validate_micronplus_layout_owned_bytes(&layout), Ok(()));
        layout.windows[0].column_groups[0].columns[0]
            .raw_markup
            .push('x');
        assert!(validate_micronplus_layout_owned_bytes(&layout).is_err());
    }

    #[test]
    fn micronplus_partial_multiplication_is_rejected_atomically() {
        let large_fragment = format!("{}\n", "x".repeat(MICRONPLUS_SOURCE_LINE_MAX_BYTES - 1))
            .repeat(MICRONPLUS_SOURCE_MAX_BYTES / MICRONPLUS_SOURCE_LINE_MAX_BYTES);
        assert_eq!(large_fragment.len(), MICRONPLUS_SOURCE_MAX_BYTES);

        let long_source = format!(":/{}.mu", "a".repeat(128));
        let mut tree = try_parse_micronplus_tree(&format!(
            "[live id=shared src={long_source}]\n[live id=shared src={long_source}]"
        ))
        .expect("tree fixture");
        let original_tree = tree.clone();
        assert!(!apply_micronplus_tree_partial(
            &mut tree,
            "shared",
            &large_fragment,
        ));
        assert_eq!(tree, original_tree);

        let mut layout = try_extract_micronplus_layout(
            "[columns]\n[column]\n[live id=shared src=:/one.mu]\n[/column]\n[column]\n[live id=shared src=:/two.mu]\n[/column]\n[/columns]",
        )
        .expect("layout fixture");
        let original_layout = layout.clone();
        assert!(!apply_micronplus_layout_partial(
            &mut layout,
            "shared",
            &large_fragment,
        ));
        assert_eq!(layout, original_layout);
    }

    #[test]
    fn micronplus_widget_store_is_item_byte_and_widget_bounded() {
        let mut store = MicronPlusWidgetStore::default();
        for index in 0..MICRONPLUS_WIDGET_STORE_MAX_WIDGETS {
            assert!(store.apply_event(MicronPlusWidgetEvent::StatusUpdate {
                id: format!("widget-{index}"),
                text: "ready".into(),
                style: None,
            }));
        }
        assert!(!store.apply_event(MicronPlusWidgetEvent::StatusUpdate {
            id: "one-too-many".into(),
            text: "rejected".into(),
            style: None,
        }));
        assert_eq!(store.metrics().widgets, MICRONPLUS_WIDGET_STORE_MAX_WIDGETS);
        assert_eq!(store.metrics().rejected_events, 1);

        assert!(store.apply_event(MicronPlusWidgetEvent::ScrollboxSet {
            id: "widget-0".into(),
            items: (0..MICRONPLUS_WIDGET_STATE_MAX_ITEMS)
                .map(|index| MicronPlusWidgetItem::text(format!("old-{index}")))
                .collect(),
        }));
        assert!(store.apply_event(MicronPlusWidgetEvent::ScrollboxAppend {
            id: "widget-0".into(),
            items: vec![MicronPlusWidgetItem::text("newest")],
        }));
        let state = store.get("widget-0").expect("widget state");
        assert_eq!(state.items.len(), MICRONPLUS_WIDGET_STATE_MAX_ITEMS);
        assert_eq!(
            state.items.last().map(|item| item.text.as_str()),
            Some("newest")
        );
        assert_ne!(
            state.items.first().map(|item| item.text.as_str()),
            Some("old-0")
        );
        assert!(store.metrics().items <= MICRONPLUS_WIDGET_STORE_MAX_ITEMS);
        assert!(store.metrics().owned_bytes <= MICRONPLUS_WIDGET_STORE_MAX_OWNED_BYTES);
    }

    #[test]
    fn micronplus_widget_store_rejects_invalid_or_structurally_excessive_items_atomically() {
        let mut store = MicronPlusWidgetStore::default();
        assert!(store.apply_event(MicronPlusWidgetEvent::StatusUpdate {
            id: "status".into(),
            text: "previous".into(),
            style: None,
        }));
        assert!(!store.apply_event(MicronPlusWidgetEvent::StatusUpdate {
            id: "status".into(),
            text: "x".repeat(MICRONPLUS_WIDGET_TEXT_MAX_BYTES + 1),
            style: None,
        }));
        assert_eq!(
            store.get("status").and_then(|state| state.text.as_deref()),
            Some("previous")
        );

        let excessive_nodes = "x\n".repeat(MICRONPLUS_TREE_MAX_NODES + 1);
        assert!(!store.apply_event(MicronPlusWidgetEvent::ScrollboxSet {
            id: "structured".into(),
            items: vec![MicronPlusWidgetItem::markup(excessive_nodes)],
        }));
        assert!(store.get("structured").is_none());
        assert_eq!(store.metrics().rejected_events, 2);
    }

    #[test]
    fn micronplus_widget_event_extraction_is_item_and_byte_bounded() {
        let markup = (0..=MICRONPLUS_EXTRACTED_EVENT_MAX_ITEMS)
            .map(|index| format!("[event name=log.append.feed text=item-{index}]"))
            .collect::<Vec<_>>()
            .join("\n");
        let (cleaned, events) = extract_micronplus_widget_events(&markup);
        assert_eq!(events.len(), MICRONPLUS_EXTRACTED_EVENT_MAX_ITEMS);
        assert!(cleaned.contains("item-256"));

        let markup = (0..80)
            .map(|index| {
                let text = format!(
                    "{index:02}{}",
                    "x".repeat(MICRONPLUS_WIDGET_TEXT_MAX_BYTES - 2)
                );
                format!("[event name=log.append.feed text={text}]")
            })
            .collect::<Vec<_>>()
            .join("\n");
        let (cleaned, events) = extract_micronplus_widget_events(&markup);
        assert!(events.len() < 80);
        assert!(!cleaned.is_empty());
    }

    #[test]
    fn micronplus_control_event_history_retains_recent_bounded_edge() {
        let mut history = Vec::new();
        for index in 0..=MICRONPLUS_CONTROL_EVENT_HISTORY_MAX_ITEMS {
            assert!(retain_micronplus_control_event(
                &mut history,
                MicronPlusControlEvent {
                    event: format!("button.{index}"),
                    source: "button".into(),
                    name: Some("send".into()),
                    action: None,
                    fields: Vec::new(),
                    value: Some("x".repeat(MICRON_CONTROL_VALUE_MAX_BYTES)),
                },
            ));
        }
        assert!(history.len() < MICRONPLUS_CONTROL_EVENT_HISTORY_MAX_ITEMS);
        assert_eq!(
            history.last().map(|event| event.event.as_str()),
            Some("button.256")
        );
        assert!(
            micronplus_control_event_history_owned_bytes(&history)
                <= MICRONPLUS_CONTROL_EVENT_HISTORY_MAX_OWNED_BYTES
        );
        let previous = history.clone();
        assert!(!retain_micronplus_control_event(
            &mut history,
            MicronPlusControlEvent {
                event: "button.invalid".into(),
                source: "button".into(),
                name: None,
                action: Some("x".repeat(MICRON_LINK_TARGET_MAX_BYTES + 1)),
                fields: Vec::new(),
                value: None,
            },
        ));
        assert_eq!(history, previous);
    }

    #[test]
    fn lowers_live_input_and_button_to_micron_primitives() {
        let lowered = lower_micronplus_markup(
            r#"[live id="feed" src=":/feed.mu" refresh=1.2 loop=3 fields="message"]
[input name="message" submit=enter event="input.changed" action="p:feed:log" fields="message"]
[button label="Refresh" event="button.clicked" action="p:feed:log" fields="message"]"#,
        );

        assert!(lowered
            .markup
            .contains("`{:/feed.mu`2`message|pid=feed|loop=3}"));
        assert!(lowered.markup.contains("`<24|message`>"));
        assert!(lowered.markup.contains("`[Refresh`p:feed:log`message]"));
        assert_eq!(lowered.lives.len(), 1);
        assert_eq!(lowered.lives[0].refresh_secs, Some(2));
        assert_eq!(lowered.lives[0].loop_count, Some(3));
        assert_eq!(lowered.inputs.len(), 1);
        assert_eq!(lowered.inputs[0].event.as_deref(), Some("input.changed"));
        assert_eq!(lowered.inputs[0].submit.as_deref(), Some("enter"));
        assert_eq!(lowered.buttons.len(), 1);
        assert_eq!(lowered.buttons[0].event.as_deref(), Some("button.clicked"));
    }

    #[test]
    fn oversized_micronplus_fields_remain_non_actionable() {
        use crate::micron::parser::MICRON_LINK_FIELD_MAX_BYTES;

        let line = format!(
            "[button label=Reject action=:/target fields={}]",
            "f".repeat(MICRON_LINK_FIELD_MAX_BYTES + 1)
        );
        let lowered = lower_micronplus_markup(&line);
        assert!(lowered.buttons.is_empty());
        assert_eq!(lowered.markup, line);
        assert!(lowered
            .diagnostics
            .iter()
            .any(|message| message.contains("fields exceed Micron link limits")));

        let tree = parse_micronplus_tree(&line);
        assert!(matches!(
            tree.nodes.as_slice(),
            [MicronPlusNode::Text { text }] if text == &line
        ));
    }

    #[test]
    fn event_only_button_lowers_to_local_micronplus_event_target() {
        let lowered = lower_micronplus_markup(
            r#"[button label="Ping" event="status.update.state" fields="message"]"#,
        );

        assert!(lowered
            .markup
            .contains("`[Ping`#micronplus-event:status.update.state`message]"));
        assert_eq!(lowered.buttons.len(), 1);
        assert_eq!(lowered.buttons[0].action, None);
        assert_eq!(
            lowered.buttons[0].event.as_deref(),
            Some("status.update.state")
        );
    }

    #[test]
    fn extracts_input_control_event_binding_from_typed_layout() {
        let layout = extract_micronplus_layout(
            r#"[window]
[columns]
[column]
[textbox name="message" submit=enter event="message.submit" action="p:feed:log" fields="message|topic=mesh"]
[/column]
[/columns]
[/window]"#,
        );

        let binding = micronplus_control_binding_for_field(&layout, "message").expect("binding");
        assert_eq!(binding.source, "textbox");
        assert_eq!(binding.event, "message.submit");
        assert_eq!(binding.action.as_deref(), Some("p:feed:log"));
        assert_eq!(binding.submit.as_deref(), Some("enter"));
        assert_eq!(binding.fields, vec!["message", "topic=mesh"]);
        assert!(micronplus_control_binding_for_field(&layout, "missing").is_none());
    }

    #[test]
    fn action_only_enter_binding_defaults_to_submit_event() {
        let layout = extract_micronplus_layout(
            r#"[window]
[columns]
[column]
[input name="message" submit=enter action="p:chatstate:chatpane:topicline:userlist" fields="session=test|message"]
[/column]
[/columns]
[/window]"#,
        );

        let binding = micronplus_control_binding_for_field(&layout, "message").expect("binding");
        assert_eq!(binding.event, "input.submit");
        assert_eq!(
            binding.action.as_deref(),
            Some("p:chatstate:chatpane:topicline:userlist")
        );
        assert_eq!(binding.fields, vec!["session=test", "message"]);
    }

    #[test]
    fn control_events_map_to_supported_widget_events() {
        let status = MicronPlusControlEvent {
            event: "status.update.state".into(),
            source: "textbox".into(),
            name: Some("message".into()),
            action: None,
            fields: vec!["style=success".into()],
            value: Some("Ready".into()),
        };
        assert_eq!(
            widget_event_from_control_event(&status),
            Some(MicronPlusWidgetEvent::StatusUpdate {
                id: "state".into(),
                text: "Ready".into(),
                style: Some("success".into()),
            })
        );

        let log = MicronPlusControlEvent {
            event: "log.append.events".into(),
            source: "button".into(),
            name: Some("Ping".into()),
            action: Some("p:feed:log".into()),
            fields: Vec::new(),
            value: None,
        };
        assert_eq!(
            widget_event_from_control_event(&log),
            Some(MicronPlusWidgetEvent::LogAppend {
                id: "events".into(),
                items: vec![MicronPlusWidgetItem::text("button: Ping -> p:feed:log")],
            })
        );

        let unsupported = MicronPlusControlEvent {
            event: "custom.event".into(),
            source: "button".into(),
            name: None,
            action: None,
            fields: Vec::new(),
            value: None,
        };
        assert_eq!(widget_event_from_control_event(&unsupported), None);
    }

    #[test]
    fn live_loop_and_refresh_coercion_match_python_plugin_bounds() {
        let lowered = lower_micronplus_markup(
            r#"[live id="zero" src=":/zero.mu" refresh=0.2 loop=-1]
[live id="bad" src=":/bad.mu" refresh=bad loop=bad]"#,
        );

        assert_eq!(lowered.lives[0].refresh_secs, Some(1));
        assert_eq!(lowered.lives[0].loop_count, Some(0));
        assert_eq!(lowered.lives[1].refresh_secs, None);
        assert_eq!(lowered.lives[1].loop_count, None);
    }

    #[test]
    fn repairs_observed_truncated_live_tag_without_spilling_source() {
        let lowered = lower_micronplus_markup(
            r#"ive id="sample_badge" src=":/page/status-card.mu" refresh=1 loop=7 fields="started_at=1|seed=1"]"#,
        );

        assert!(lowered
            .markup
            .contains("`{:/page/status-card.mu`1`started_at=1|seed=1|pid=sample_badge|loop=7}"));
        assert!(!lowered.markup.contains("ive id="));
        assert_eq!(lowered.lives.len(), 1);
        assert_eq!(lowered.lives[0].id, "sample_badge");
    }

    #[test]
    fn truncated_nomadnet_style_fragment_lowers_without_style_or_live_spill() {
        let lowered = lower_micronplus_markup(
            r#"c=0
fg=f00

f00
══════════════════════════════════════════════╗
               SAMPLE PORTAL                  ║
══════════════════════════════════════════════╝
ive id="sample_badge" src=":/page/status-card.mu" refresh=1 loop=7 fields="started_at=1|seed=1"]

0f0OPEN . MESH . ENCRYPTION . NETWORK . NODE`f
ddd
8f8Contact LXMF:`f `_`[lxmf@00112233445566778899aabbccddeeff`lxmf@00112233445566778899aabbccddeeff]`_`
400▐█`f`B400`Ff00 SAMPLE NAVIGATION `f`b`F400█▌`f
`[Home`:/page/index.mu]`_`   `_`[Hub`:/page/hub.mu]`_`"#,
        );
        let document = parse_micron(&lowered.markup);
        let rendered = render_document(&document, 88)
            .into_iter()
            .map(|row| row.text())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            lowered.markup.contains("`{:/page/status-card.mu`1`"),
            "{}",
            lowered.markup
        );
        assert!(!rendered.contains("ive id="));
        assert!(!rendered.contains("\nf00\n"));
        assert!(!rendered.contains("\nddd\n"));
        assert!(rendered.contains("SAMPLE PORTAL"));
        assert!(rendered.contains("Contact LXMF:"));
        assert!(rendered.contains("SAMPLE NAVIGATION"));
        assert!(rendered.contains("Home"));
        assert!(rendered.contains("Hub"));
        assert!(!rendered.contains("/page/index.mu]"));
    }

    #[test]
    fn button_partial_actions_preserve_python_partial_action_target() {
        let lowered = lower_micronplus_markup(
            r#"[button label="Refresh" action="p:feed:log" fields="message"]
[live id="feed" src=":/feed.mu" refresh=2 loop=3 fields="message"]"#,
        );

        assert!(lowered.markup.contains("`[Refresh`p:feed:log`message]"));
        assert_eq!(lowered.lives.len(), 1);
        assert_eq!(lowered.buttons.len(), 1);
    }

    #[test]
    fn lowers_textbox_and_masked_inputs_to_existing_micron_controls() {
        let lowered = lower_micronplus_markup(
            r#"[textbox name="body" label="Message" width=40 value="hello" action=":/send.mu" fields="body"]
[input name="secret" width=12 masked=true value="hunter2"]"#,
        );

        assert!(lowered.markup.contains("`F888Message`f"));
        assert!(lowered.markup.contains("`<40|body`hello>"));
        assert!(lowered.markup.contains("`<!12|secret`hunter2>"));
        assert_eq!(lowered.inputs.len(), 2);
        assert_eq!(lowered.inputs[0].name, "body");
        assert_eq!(lowered.inputs[0].action.as_deref(), Some(":/send.mu"));
    }

    #[test]
    fn widthless_input_renders_to_available_fragment_width() {
        let rows = render_micronplus_rows_with_widgets_and_field_cursor(
            r#"[input name="message" submit=enter action="p:chatpane" fields="message"]"#,
            42,
            None,
            None,
        );
        let control_cells = rows
            .iter()
            .flat_map(|row| row.cells.iter())
            .filter(|cell| {
                cell.control
                    .as_ref()
                    .is_some_and(|control| control.name.as_ref() == "message")
            })
            .count();

        assert_eq!(control_cells, 42);
    }

    #[test]
    fn explicit_input_width_still_wins_over_available_fragment_width() {
        let rows = render_micronplus_rows_with_widgets_and_field_cursor(
            r#"[input name="message" width=12 submit=enter action="p:chatpane" fields="message"]"#,
            42,
            None,
            None,
        );
        let control_cells = rows
            .iter()
            .flat_map(|row| row.cells.iter())
            .filter(|cell| {
                cell.control
                    .as_ref()
                    .is_some_and(|control| control.name.as_ref() == "message")
            })
            .count();

        assert_eq!(control_cells, 12);
    }

    #[test]
    fn lowers_box_and_status_variants_without_code_spill() {
        let lowered = lower_micronplus_markup(
            r#"[box title="Panel"]
[status text="Waiting" style="muted"]
[status text="Primary" style="accent"]
[/box]"#,
        );

        assert!(lowered.markup.contains("`F555+-- Panel --+`f"));
        assert!(lowered
            .markup
            .contains("`F555+------------------------------+`f"));
        assert!(lowered.markup.contains("`F888Waiting`f"));
        assert!(lowered.markup.contains("`Ff8fPrimary`f"));
        assert!(!lowered.markup.contains("[box"));
        assert!(!lowered.markup.contains("[/box]"));
    }

    #[test]
    fn lowers_structural_micronplus_tags_without_code_spill() {
        let lowered = lower_micronplus_markup(
            r#"[window title="MicronPlus Demo"]
[columns]
[column weight=3]
[status text="MicronPlus detected" style="success"]
Body
[/column]
[column weight=2]
[scrollbox height=6]
Side
[/scrollbox]
[log height=5 max=4 id="demo_log"]
entry
[/log]
[/column]
[/columns]
[/window]"#,
        );

        assert!(lowered.markup.contains(">MicronPlus Demo"));
        assert!(lowered.markup.contains("`F0f0MicronPlus detected`f"));
        assert!(lowered
            .markup
            .contains("`F555------------------------------`f"));
        assert!(lowered.markup.contains("log: demo_log"));
        assert!(lowered.markup.contains("Body"));
        assert!(lowered.markup.contains("Side"));
        assert!(!lowered.markup.contains("[columns]"));
        assert!(!lowered.markup.contains("[/window]"));
    }

    #[test]
    fn extracts_widget_events_and_strips_control_lines() {
        let (cleaned, events) = extract_micronplus_widget_events(
            r#"before
[event name="status.update.state" text="Ready" style="success"]
[event name="scrollbox.set.feed" text="first item"]
[event target="scrollbox.append.feed" text="second item" style="warning"]
[widget-event name="log.append.events" text="runtime event"]
after"#,
        );

        assert_eq!(cleaned, "before\nafter");
        assert_eq!(
            events,
            vec![
                MicronPlusWidgetEvent::StatusUpdate {
                    id: "state".into(),
                    text: "Ready".into(),
                    style: Some("success".into()),
                },
                MicronPlusWidgetEvent::ScrollboxSet {
                    id: "feed".into(),
                    items: vec![MicronPlusWidgetItem::text("first item")],
                },
                MicronPlusWidgetEvent::ScrollboxAppend {
                    id: "feed".into(),
                    items: vec![MicronPlusWidgetItem::styled("second item", "warning")],
                },
                MicronPlusWidgetEvent::LogAppend {
                    id: "events".into(),
                    items: vec![MicronPlusWidgetItem::text("runtime event")],
                },
            ]
        );
    }

    #[test]
    fn preserves_unknown_widget_event_lines_as_markup() {
        let (cleaned, events) = extract_micronplus_widget_events(
            r#"[event name="unknown.action.feed" text="do not hide"]
[event name="status.update." text="missing id"]
body"#,
        );

        assert!(events.is_empty());
        assert!(cleaned.contains("unknown.action.feed"));
        assert!(cleaned.contains("status.update."));
        assert!(cleaned.contains("body"));
    }

    #[test]
    fn applies_partial_content_to_typed_column_layout() {
        let mut layout = extract_micronplus_layout(
            r#"[window]
[columns]
[column]
before
[live id="feed" src="mock.node:/feed.mu" refresh=2]
after
[/column]
[/columns]
[/window]"#,
        );

        assert!(apply_micronplus_layout_partial(
            &mut layout,
            "feed",
            "loaded\nbody"
        ));
        let raw = &layout.windows[0].column_groups[0].columns[0].raw_markup;
        assert!(raw.contains("before"));
        assert!(raw.contains("loaded\nbody"));
        assert!(raw.contains("after"));
        assert!(!raw.contains("[live"));
        assert!(raw.contains("OMENBROWSER_RS_PARTIAL_BEGIN feed"));

        assert!(apply_micronplus_layout_partial(
            &mut layout,
            "feed",
            "#!c=0\nupdated"
        ));
        let raw = &layout.windows[0].column_groups[0].columns[0].raw_markup;
        assert!(raw.contains("before"));
        assert!(raw.contains("updated"));
        assert!(!raw.contains("loaded\nbody"));
        assert!(!raw.contains("#!c=0"));
        assert!(raw.contains("after"));
    }

    #[test]
    fn live_partial_nested_columns_render_in_place_without_tag_spill() {
        let mut layout = extract_micronplus_layout(
            r#"[window title="Parent"]
[columns]
[column]
Status:
[live id="sample_badge" src=":/page/status-card.mu" refresh=1 loop=7 fields="started_at=1|seed=1"]
Ready
[/column]
[/columns]
[/window]"#,
        );

        assert!(apply_micronplus_layout_partial(
            &mut layout,
            "sample_badge",
            r#"[columns]
[column weight=1]
[/column]
[column width=14]
`F8f8`!SAMPLE/READY`!`f
[/column]
[column weight=1]
[/column]
[/columns]"#,
        ));

        let rows = render_column_group_preview(&layout.windows[0].column_groups[0], 48).join("\n");

        assert!(rows.contains("Status:"));
        assert!(rows.contains("SAMPLE/READY"));
        assert!(rows.contains("Ready"));
        assert!(!rows.contains("[columns]"));
        assert!(!rows.contains("[column"));
        assert!(!rows.contains("[/columns]"));
        assert!(rows.lines().all(|line| line.chars().count() <= 48));
    }

    #[test]
    fn clips_scrollbox_body_to_declared_height() {
        let lowered = lower_micronplus_markup(
            r#"[scrollbox title="Recent" height=2]
one
two
three
[/scrollbox]"#,
        );

        assert!(lowered.markup.contains("Recent"));
        assert!(lowered.markup.contains("one"));
        assert!(lowered.markup.contains("two"));
        assert!(!lowered.markup.contains("three"));
        assert!(!lowered.markup.contains("[/scrollbox]"));
        assert!(lowered.markup.contains("`F888...`f"));
    }

    #[test]
    fn renders_livechat_userlist_window_buttons_inside_scrollbox() {
        let rows = render_micronplus_rows_with_widgets_and_field_cursor(
            r#"[window title="Present Now"]
`F8f82 operators in lounge`f
[scrollbox id="chat_users_panel" height=18]
[button label="Alice" action="p:whoispane" fields="session=test|whois=alice"]
`F777Operator`f
[button label="Bob" action="p:whoispane" fields="session=test|whois=bob"]
`F777Relay`f
[/scrollbox]
[/window]"#,
            48,
            None,
            None,
        );
        let text = rows
            .iter()
            .map(|row| row.text())
            .collect::<Vec<_>>()
            .join("\n");
        let links = rows
            .iter()
            .flat_map(|row| row.cells.iter())
            .filter_map(|cell| cell.link.as_ref())
            .collect::<Vec<_>>();

        assert!(text.contains("Present Now"));
        assert!(text.contains("Alice"));
        assert!(text.contains("Bob"));
        assert!(text.contains("Operator"));
        assert!(!text.contains("[button"));
        assert!(links.iter().any(|link| {
            link.target == "p:whoispane"
                && link.fields == vec!["session=test".to_string(), "whois=alice".to_string()]
        }));
    }

    #[test]
    fn tree_renderer_composes_livechat_like_boxed_columns() {
        let mut tree = parse_micronplus_tree(
            r#"[window title="ExampleChat // MicronPlus"]
[status id="chat_state" text="Ready." style="info"]
[columns]
[column weight=4]
[live id="topicline" src=":/page/sample-chat-topic.mu" refresh=2]
[live id="chatpane" src=":/page/sample-chat-messages.mu" refresh=1]
`F777Type a message then press Enter.`f
[input name="message" submit=enter action="p:chatstate:chatpane:topicline:userlist"]
[button label="Send" action="p:chatstate:chatpane:topicline:userlist"]
[/column]
[column weight=1]
[live id="userlist" src=":/page/sample-chat-users.mu" refresh=2]
[live id="whoispane" src=":/page/sample-chat-whois.mu" refresh=3600]
[/column]
[/columns]
[/window]"#,
        );

        assert!(apply_micronplus_tree_partial(
            &mut tree,
            "topicline",
            r#"[box title="Topic"]
Welcome to ExampleChat
Set by ExampleHost
[/box]"#,
        ));
        assert!(apply_micronplus_tree_partial(
            &mut tree,
            "chatpane",
            r#"[scrollbox id="chatlog_panel" height=4]
(Sun,19:30) `Ff88`!<ExampleHost>`!`f test
(Sun,21:15) `Ff88`!<ExampleHost>`!`f test test
(Tue,13:49) `Ff88`!<ExampleHost>`!`f micronplus enabled chat is really nice
(Wed,19:49) `Ff88`!<ExampleHost>`!`f latest
[/scrollbox]"#,
        ));
        assert!(apply_micronplus_tree_partial(
            &mut tree,
            "userlist",
            r#"[window title="Present Now"]
`F8f81 operators in lounge`f
[scrollbox id="chat_users_panel" height=3]
[button label="ExampleHost" action="p:whoispane" fields="whois=1"]
`F777Directory Note`f
[/scrollbox]
[/window]"#,
        ));
        assert!(apply_micronplus_tree_partial(
            &mut tree,
            "whoispane",
            r#"[box title="Whois"]
Click a nickname in the live list.
[/box]"#,
        ));

        let rows =
            render_micronplus_tree_rows_with_widgets_and_field_cursor(&tree, 100, None, None, None);
        let text = rows
            .iter()
            .map(|row| row.text())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(text.contains("┌ ExampleChat // MicronPlus "));
        assert!(text.contains("┌ Topic "));
        assert!(text.contains("┌ Present Now "));
        assert!(text.contains("┌ Whois "));
        assert!(text.contains("micronplus enabled chat"));
        assert!(text.contains("Type a message then press Enter."));
        assert!(text.contains("Send"));
        assert!(!text.contains("[window"));
        assert!(!text.contains("[scrollbox"));
        assert!(rows.iter().any(|row| row.text().contains("│")));
    }

    #[test]
    fn trims_log_body_to_recent_max_and_height() {
        let lowered = lower_micronplus_markup(
            r#"[log id="demo" height=3 max=4]
oldest
older
newer
newest
latest
[/log]"#,
        );

        assert!(lowered.markup.contains("log: demo"));
        assert!(!lowered.markup.contains("oldest"));
        assert!(!lowered.markup.contains("older"));
        assert!(lowered.markup.contains("newer"));
        assert!(lowered.markup.contains("newest"));
        assert!(lowered.markup.contains("latest"));
        assert!(!lowered.markup.contains("[/log]"));
    }

    #[test]
    fn widget_events_update_status_scrollbox_and_log_fallback() {
        let mut widgets = MicronPlusWidgetStore::default();
        widgets.apply_event(MicronPlusWidgetEvent::StatusUpdate {
            id: "state".into(),
            text: "Updated".into(),
            style: Some("success".into()),
        });
        widgets.apply_event(MicronPlusWidgetEvent::ScrollboxSet {
            id: "feed".into(),
            items: vec![MicronPlusWidgetItem::text("dynamic one")],
        });
        widgets.apply_event(MicronPlusWidgetEvent::ScrollboxAppend {
            id: "feed".into(),
            items: vec![MicronPlusWidgetItem::styled("dynamic two", "warning")],
        });
        widgets.apply_event(MicronPlusWidgetEvent::LogAppend {
            id: "events".into(),
            items: vec![MicronPlusWidgetItem::text("runtime event")],
        });
        let lowered = lower_micronplus_markup_with_widgets(
            r#"[status id="state" text="Initial" style="error"]
[scrollbox id="feed" height=3]
static
[/scrollbox]
[log id="events" height=2 max=2]
old
[/log]"#,
            Some(&widgets),
        );

        assert!(lowered.markup.contains("`F0f0Updated`f"));
        assert!(!lowered.markup.contains("static"));
        assert!(lowered.markup.contains("dynamic one"));
        assert!(lowered.markup.contains("`Fff0dynamic two`f"));
        assert!(lowered.markup.contains("old"));
        assert!(lowered.markup.contains("runtime event"));
    }

    #[test]
    fn widget_items_can_render_nested_micronplus_markup_without_tag_spill() {
        let mut widgets = MicronPlusWidgetStore::default();
        widgets.apply_event(MicronPlusWidgetEvent::ScrollboxSet {
            id: "feed".into(),
            items: vec![MicronPlusWidgetItem::markup(
                r#"[status text="Nested OK" style="success"]
[button label="Act" event="status.update.state"]"#,
            )],
        });

        let lowered = lower_micronplus_markup_with_widgets(
            r#"[scrollbox id="feed" height=4]
static
[/scrollbox]"#,
            Some(&widgets),
        );

        assert!(!lowered.markup.contains("static"));
        assert!(lowered.markup.contains("`F0f0Nested OK`f"));
        assert!(lowered
            .markup
            .contains("`[Act`#micronplus-event:status.update.state]"));
        assert!(!lowered.markup.contains("[status"));
        assert!(!lowered.markup.contains("[button"));
    }

    #[test]
    fn column_renderer_preserves_nested_widget_columns_inside_scrollbox() {
        let mut widgets = MicronPlusWidgetStore::default();
        widgets.apply_event(MicronPlusWidgetEvent::ScrollboxSet {
            id: "feed".into(),
            items: vec![MicronPlusWidgetItem::markup(
                r#"[columns]
[column width=10]
left
[/column]
[column weight=1]
right
[/column]
[/columns]"#,
            )],
        });
        let layout = extract_micronplus_layout(
            r#"[columns]
[column weight=1]
[scrollbox id="feed" height=2]
static
[/scrollbox]
[/column]
[/columns]"#,
        );
        let rows = render_column_group_rows_with_widgets(
            &layout.windows[0].column_groups[0],
            42,
            Some(&widgets),
        )
        .into_iter()
        .map(|row| row.text().trim_end().to_string())
        .collect::<Vec<_>>();
        let joined = rows.join("\n");

        assert!(rows
            .iter()
            .any(|row| row.contains("left") && row.contains("right")));
        assert!(!joined.contains("static"));
        assert!(!joined.contains("[columns]"));
        assert!(!joined.contains("[column"));
        assert!(!joined.contains("[scrollbox"));
        assert!(rows.iter().all(|row| row.chars().count() <= 42));
    }

    #[test]
    fn structured_widget_scrollbox_clips_after_nested_columns_render() {
        let mut widgets = MicronPlusWidgetStore::default();
        widgets.apply_event(MicronPlusWidgetEvent::ScrollboxSet {
            id: "feed".into(),
            items: vec![MicronPlusWidgetItem::markup(
                r#"[columns]
[column width=10]
first
second
third
[/column]
[column weight=1]
one
two
three
[/column]
[/columns]"#,
            )],
        });
        let layout = extract_micronplus_layout(
            r#"[columns]
[column weight=1]
[scrollbox id="feed" height=2]
static
[/scrollbox]
[/column]
[/columns]"#,
        );
        let rows = render_column_group_rows_with_widgets(
            &layout.windows[0].column_groups[0],
            42,
            Some(&widgets),
        )
        .into_iter()
        .map(|row| row.text().trim_end().to_string())
        .collect::<Vec<_>>();
        let joined = rows.join("\n");

        assert!(joined.contains("first"));
        assert!(joined.contains("second"));
        assert!(!joined.contains("third"));
        assert!(joined.contains("..."));
        assert!(!joined.contains("[columns]"));
        assert!(!joined.contains("[column"));
    }

    #[test]
    fn structured_widget_log_clips_recent_rows_after_nested_columns_render() {
        let mut widgets = MicronPlusWidgetStore::default();
        widgets.apply_event(MicronPlusWidgetEvent::LogAppend {
            id: "events".into(),
            items: vec![MicronPlusWidgetItem::markup(
                r#"[columns]
[column width=10]
old
new
latest
[/column]
[column weight=1]
left
middle
right
[/column]
[/columns]"#,
            )],
        });
        let layout = extract_micronplus_layout(
            r#"[columns]
[column weight=1]
[log id="events" height=2 max=2]
seed
[/log]
[/column]
[/columns]"#,
        );
        let rows = render_column_group_rows_with_widgets(
            &layout.windows[0].column_groups[0],
            42,
            Some(&widgets),
        )
        .into_iter()
        .map(|row| row.text().trim_end().to_string())
        .collect::<Vec<_>>();
        let joined = rows.join("\n");

        assert!(!joined.contains("old"));
        assert!(joined.contains("new"));
        assert!(joined.contains("latest"));
        assert!(!joined.contains("[columns]"));
        assert!(!joined.contains("[column"));
    }

    #[test]
    fn widget_event_line_preserves_markup_payloads() {
        let (cleaned, events) = extract_micronplus_widget_events(
            r#"[event name="scrollbox.set.feed" markup="[status text='Nested' style='success']"]
body"#,
        );

        assert_eq!(cleaned, "body");
        assert_eq!(events.len(), 1);
        let MicronPlusWidgetEvent::ScrollboxSet { id, items } = &events[0] else {
            panic!("expected scrollbox set");
        };
        assert_eq!(id, "feed");
        assert_eq!(
            items[0].markup.as_deref(),
            Some("[status text='Nested' style='success']")
        );
    }

    #[test]
    fn lowering_dedents_and_cleans_blank_runs_like_python_transform() {
        let lowered = lower_micronplus_markup(
            r#"
                [window title="Panel"]


                Body
                [status text="Ready" style="success"]

                [/window]
            "#,
        );

        assert!(lowered.diagnostics.is_empty());
        assert!(!lowered.markup.starts_with('\n'));
        assert!(!lowered.markup.ends_with('\n'));
        assert!(!lowered.markup.contains("\n\n\n"));
        assert!(lowered.markup.contains(">Panel"));
        assert!(lowered.markup.contains("Body"));
        assert!(lowered.markup.contains("`F0f0Ready`f"));
        assert!(!lowered.markup.contains("                Body"));
    }

    #[test]
    fn lowering_reports_unsupported_attrs_and_preserves_malformed_lines() {
        let lowered = lower_micronplus_markup(
            r#"[status text="Ready" unknown="x"]
[button label="Broken action=:/bad.mu]
after"#,
        );

        assert!(lowered
            .diagnostics
            .iter()
            .any(|message| message.contains("unsupported attrs: unknown")));
        assert!(lowered
            .diagnostics
            .iter()
            .any(|message| message.contains("unterminated quoted attribute value")));
        assert!(lowered.markup.contains("`F6cfReady`f"));
        assert!(lowered
            .markup
            .contains("[button label=\"Broken action=:/bad.mu]"));
        assert!(lowered.markup.contains("after"));
    }

    #[test]
    fn preserves_unclosed_structural_blocks() {
        let lowered = lower_micronplus_markup("[scrollbox height=2]\none\ntwo");

        assert_eq!(lowered.markup, "[scrollbox height=2]\none\ntwo");
    }

    #[test]
    fn preserves_unknown_or_malformed_micronplus_lines() {
        let lowered = lower_micronplus_markup("[unknown title=\"x\"]\n[input label=\"missing\"]");

        assert_eq!(
            lowered.markup,
            "[unknown title=\"x\"]\n[input label=\"missing\"]"
        );
        assert!(lowered.inputs.is_empty());
    }

    #[test]
    fn micronplus_window_fixture_lowers_without_supported_tag_spill() {
        let markup = include_str!("../../fixtures/micron/micronplus_window.mu");
        let lowered = lower_micronplus_markup(markup);

        assert_no_supported_micronplus_spill(&lowered.markup);
        assert!(lowered.markup.contains(">Trusted Node Panel"));
        assert!(lowered.markup.contains("`F555+-- Login --+`f"));
        assert!(lowered.markup.contains("`<18|username`guest>"));
        assert!(lowered.markup.contains("`<!18|password`>"));
        assert!(lowered
            .markup
            .contains("`[Enter`:/page/login.mu`username|password]"));
        assert_eq!(lowered.inputs.len(), 2);
        assert_eq!(lowered.buttons.len(), 1);
    }

    #[test]
    fn micronplus_columns_fixture_lowers_and_renders_at_regression_widths() {
        let markup = include_str!("../../fixtures/micron/micronplus_columns.mu");
        let lowered = lower_micronplus_markup(markup);

        assert_no_supported_micronplus_spill(&lowered.markup);
        assert!(lowered
            .markup
            .contains("`{:/page/feed.mu`3`message|pid=feed|loop=2}"));
        assert!(lowered.markup.contains("Scroll"));
        assert!(lowered.markup.contains("`F555+-- Scroll --+`f"));
        assert!(lowered
            .markup
            .contains("`F555+------------------------------+`f"));
        assert!(!lowered.markup.contains("row 3"));
        assert!(!lowered.markup.contains("\nold\n"));
        assert!(lowered.markup.contains("new"));
        assert!(lowered.markup.contains("latest"));

        for width in REGRESSION_WIDTHS {
            let report = render_markup_report(
                PathBuf::from("fixtures/micron/micronplus_columns.mu"),
                &lowered.markup,
                *width,
            );
            assert!(
                report.suspected_style_spill.is_empty(),
                "style spill at width {width}: {:?}",
                report.suspected_style_spill
            );
            assert!(report.controls > 0, "expected controls at width {width}");
            assert!(report.links > 0, "expected links at width {width}");
        }
    }

    #[test]
    fn micronplus_columns_fixture_extracts_typed_layout() {
        let markup = include_str!("../../fixtures/micron/micronplus_columns.mu");
        let layout = extract_micronplus_layout(markup);

        assert_eq!(layout.windows.len(), 1);
        assert_eq!(layout.windows[0].title.as_deref(), Some("Live Dashboard"));
        assert_eq!(layout.windows[0].column_groups.len(), 1);
        let group = &layout.windows[0].column_groups[0];
        assert_eq!(group.columns.len(), 2);
        assert_eq!(group.columns[0].title.as_deref(), Some("Controls"));
        assert_eq!(group.columns[0].width, None);
        assert_eq!(group.columns[0].weight, 3);
        assert!(group.columns[0].raw_markup.contains("[live id=\"feed\""));
        assert_eq!(group.columns[1].title.as_deref(), Some("Recent"));
        assert_eq!(group.columns[1].width, None);
        assert_eq!(group.columns[1].weight, 2);
        assert!(group.columns[1].raw_markup.contains("[scrollbox"));
    }

    #[test]
    fn micronplus_column_preview_renders_side_by_side_without_tag_spill() {
        let markup = include_str!("../../fixtures/micron/micronplus_columns.mu");
        let layout = extract_micronplus_layout(markup);
        let group = &layout.windows[0].column_groups[0];
        let rows = render_column_group_preview(group, 64);
        let joined = rows.join("\n");

        assert!(!rows.is_empty());
        assert!(joined.contains("[Controls]"));
        assert!(joined.contains("[Recent]"));
        assert!(rows.iter().any(|row| row.contains("   ")));
        assert!(!joined.contains("[columns]"));
        assert!(!joined.contains("[scrollbox"));
        assert!(!joined.contains("[textbox"));
        assert!(!joined.contains("[/column]"));
        assert!(joined.lines().all(|line| line.chars().count() <= 64));
    }

    #[test]
    fn micronplus_column_width_distribution_honors_explicit_widths() {
        let layout = extract_micronplus_layout(
            r#"[columns]
[column width=12]
left
[/column]
[column weight=2]
right
[/column]
[/columns]"#,
        );
        let group = &layout.windows[0].column_groups[0];

        assert_eq!(group.columns[0].width, Some(12));
        assert_eq!(distribute_column_widths(group, 37), vec![12, 25]);
        let rows = render_column_group_preview(group, 40);

        assert!(rows.iter().any(|row| row.starts_with("left")));
        assert!(rows.iter().any(|row| row.contains("   right")));
    }

    fn assert_no_supported_micronplus_spill(markup: &str) {
        for tag in [
            "window",
            "box",
            "columns",
            "column",
            "scrollbox",
            "log",
            "status",
            "live",
            "input",
            "textbox",
            "button",
        ] {
            assert!(
                !markup.contains(&format!("[{tag}")) && !markup.contains(&format!("[/{tag}]")),
                "supported MicronPlus tag leaked after lowering: {tag}\n{markup}"
            );
        }
    }
}
