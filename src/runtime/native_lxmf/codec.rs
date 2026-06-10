use std::collections::BTreeMap;
use std::path::Path;

#[cfg(feature = "native-rns-net")]
use rand_core::RngCore;
use reticulum_rs::core::identity::PrivateIdentity;
use reticulum_rs::core::ratchets::decrypt_with_identity;
#[cfg(feature = "native-rns-net")]
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};
use crate::messaging::{
    AttachmentSummary, DeliveryMode, MessageEnvelope, MessageSummary,
    TransportMethod as AppTransportMethod,
};

const FIELD_FILE_ATTACHMENTS: i64 = 0x05;
#[cfg(feature = "native-rns-net")]
const PROPAGATION_STAMP_SIZE: usize = 32;
#[cfg(feature = "native-rns-net")]
const PROPAGATION_LXMF_OVERHEAD: usize = 112;
#[cfg(feature = "native-rns-net")]
const PROPAGATION_WORKBLOCK_EXPAND_ROUNDS: u32 = 1000;
#[cfg(feature = "native-rns-net")]
pub const DEFAULT_PROPAGATION_STAMP_MAX_ATTEMPTS: u64 = 1 << 22;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeLxmfWireApi {
    pub message_type: &'static str,
    pub payload_type: &'static str,
    pub wire_message_type: &'static str,
}

#[derive(Clone, Debug)]
pub struct NativeLxmfOutbound {
    pub message: lxmf::Message,
    pub delivery: lxmf::DeliveryDecision,
    pub include_ticket: bool,
    pub attachments: Vec<AttachmentSummary>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeLxmfParity {
    pub payload_stamps_supported: bool,
    pub stamp_validation_supported: bool,
    pub stamp_generation_supported: bool,
    pub include_ticket_supported: bool,
}

pub fn native_lxmf_wire_api() -> NativeLxmfWireApi {
    NativeLxmfWireApi {
        message_type: core::any::type_name::<lxmf::Message>(),
        payload_type: core::any::type_name::<lxmf::Payload>(),
        wire_message_type: core::any::type_name::<lxmf::WireMessage>(),
    }
}

pub fn native_lxmf_parity() -> NativeLxmfParity {
    NativeLxmfParity {
        payload_stamps_supported: true,
        stamp_validation_supported: cfg!(feature = "native-rns-net"),
        stamp_generation_supported: cfg!(feature = "native-rns-net"),
        include_ticket_supported: false,
    }
}

pub fn native_delivery_type_name() -> &'static str {
    "lxmf::Message"
}

pub fn delivery_display_name_from_app_data(app_data: &[u8]) -> Option<String> {
    lxmf::wire::announce::display_name_from_delivery_app_data(app_data)
}

pub fn encode_delivery_display_name_app_data(display_name: &str) -> AppResult<Vec<u8>> {
    lxmf::wire::announce::encode_delivery_display_name_app_data(display_name)
        .ok_or_else(|| AppError::Runtime("LXMF delivery announce app-data failed".into()))
}

pub fn propagation_display_name_from_app_data(app_data: &[u8]) -> Option<String> {
    let data = parse_propagation_announce_data(app_data)?;
    let metadata = data.get(6)?.as_map()?;
    for (key, value) in metadata {
        if value_as_u64(key) == Some(0x01) {
            return value_as_display_name(value);
        }
    }
    None
}

pub fn propagation_announce_data_is_valid(app_data: &[u8]) -> bool {
    parse_propagation_announce_data(app_data).is_some()
}

pub fn propagation_announce_stamp_costs(app_data: &[u8]) -> Vec<u8> {
    parse_propagation_announce_data(app_data)
        .and_then(|items| {
            items.get(5).and_then(rmpv::Value::as_array).map(|costs| {
                costs
                    .iter()
                    .filter_map(value_as_u64)
                    .filter_map(|cost| u8::try_from(cost).ok())
                    .collect()
            })
        })
        .unwrap_or_default()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropagationStampValidation {
    pub transient_id: [u8; 32],
    pub lxm_data: Vec<u8>,
    pub stamp_value: u32,
    pub target_cost: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedPropagationStamp {
    pub transient_id: [u8; 32],
    pub stamp: Vec<u8>,
    pub stamp_value: u32,
    pub target_cost: u8,
    pub attempts: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeLxmfPropagationEnvelope {
    pub envelope: Vec<u8>,
    pub transient_id: [u8; 32],
    pub stamp: Option<GeneratedPropagationStamp>,
}

pub fn propagation_announce_target_stamp_cost(app_data: &[u8]) -> Option<u8> {
    propagation_announce_stamp_costs(app_data)
        .into_iter()
        .next()
}

#[cfg(feature = "native-rns-net")]
pub fn validate_propagation_stamp_any_cost(
    transient_data: &[u8],
    target_costs: &[u8],
) -> Option<PropagationStampValidation> {
    for target_cost in target_costs {
        let Some((transient_id, lxm_data, stamp_value)) =
            validate_propagation_stamp(transient_data, *target_cost)
        else {
            continue;
        };
        return Some(PropagationStampValidation {
            transient_id,
            lxm_data,
            stamp_value,
            target_cost: *target_cost,
        });
    }
    None
}

#[cfg(feature = "native-rns-net")]
pub fn encode_signed_propagation_envelope(
    outbound: &NativeLxmfOutbound,
    private_identity_bytes: &[u8],
    recipient_public_key: [u8; 64],
    target_stamp_cost: Option<u8>,
    max_stamp_attempts: u64,
) -> AppResult<NativeLxmfPropagationEnvelope> {
    let wire_bytes = encode_signed_wire_message(outbound, private_identity_bytes)?;
    let wire = lxmf::WireMessage::unpack(wire_bytes.as_slice()).map_err(|err| {
        AppError::Runtime(format!("LXMF wire decode before propagation failed: {err}"))
    })?;
    let recipient = lxmf::wire::identity::Identity::new_from_slices(
        &recipient_public_key[..32],
        &recipient_public_key[32..],
    );
    let timestamp = wire.payload.timestamp;
    let (lxm_data, transient_id) = wire
        .pack_propagation_transient_with_rng(&recipient, rand_core::OsRng)
        .map_err(|err| {
            AppError::Runtime(format!("LXMF propagation transient encode failed: {err}"))
        })?;
    let stamp = if let Some(target_cost) = target_stamp_cost {
        Some(generate_propagation_stamp_for_transient(
            &lxm_data,
            transient_id,
            target_cost,
            max_stamp_attempts,
        )?)
    } else {
        None
    };
    let envelope = lxmf::WireMessage::pack_propagation_envelope(
        timestamp,
        &lxm_data,
        stamp.as_ref().map(|stamp| stamp.stamp.as_slice()),
    )
    .map_err(|err| AppError::Runtime(format!("LXMF propagation envelope encode failed: {err}")))?;
    Ok(NativeLxmfPropagationEnvelope {
        envelope,
        transient_id,
        stamp,
    })
}

#[cfg(feature = "native-rns-net")]
pub fn generate_propagation_stamp_for_transient(
    lxm_data: &[u8],
    transient_id: [u8; 32],
    target_cost: u8,
    max_attempts: u64,
) -> AppResult<GeneratedPropagationStamp> {
    let workblock =
        rns_core::stamp::stamp_workblock(&transient_id, PROPAGATION_WORKBLOCK_EXPAND_ROUNDS);
    let mut stamp = vec![0u8; PROPAGATION_STAMP_SIZE];
    for attempt in 1..=max_attempts {
        rand_core::OsRng.fill_bytes(&mut stamp);
        if rns_core::stamp::stamp_valid(&stamp, target_cost, &workblock) {
            let stamp_value = rns_core::stamp::stamp_value(&workblock, &stamp);
            let mut transient_data = Vec::with_capacity(lxm_data.len() + stamp.len());
            transient_data.extend_from_slice(lxm_data);
            transient_data.extend_from_slice(&stamp);
            if validate_propagation_stamp(&transient_data, target_cost).is_none() {
                return Err(AppError::Runtime(
                    "generated LXMF propagation stamp failed self-validation".into(),
                ));
            }
            return Ok(GeneratedPropagationStamp {
                transient_id,
                stamp,
                stamp_value,
                target_cost,
                attempts: attempt,
            });
        }
    }
    Err(AppError::Runtime(format!(
        "LXMF propagation stamp generation did not find a cost {target_cost} stamp within {max_attempts} attempts"
    )))
}

#[cfg(feature = "native-rns-net")]
fn validate_propagation_stamp(
    transient_data: &[u8],
    target_cost: u8,
) -> Option<([u8; 32], Vec<u8>, u32)> {
    if transient_data.len() <= PROPAGATION_LXMF_OVERHEAD + PROPAGATION_STAMP_SIZE {
        return None;
    }
    let split = transient_data.len() - PROPAGATION_STAMP_SIZE;
    let lxm_data = &transient_data[..split];
    let stamp = &transient_data[split..];
    let digest = Sha256::digest(lxm_data);
    let mut transient_id = [0u8; 32];
    transient_id.copy_from_slice(&digest);
    let workblock =
        rns_core::stamp::stamp_workblock(&transient_id, PROPAGATION_WORKBLOCK_EXPAND_ROUNDS);
    if rns_core::stamp::stamp_valid(stamp, target_cost, &workblock) {
        let stamp_value = rns_core::stamp::stamp_value(&workblock, stamp);
        Some((transient_id, lxm_data.to_vec(), stamp_value))
    } else {
        None
    }
}

#[cfg(not(feature = "native-rns-net"))]
pub fn validate_propagation_stamp_any_cost(
    _transient_data: &[u8],
    _target_costs: &[u8],
) -> Option<PropagationStampValidation> {
    None
}

fn parse_propagation_announce_data(app_data: &[u8]) -> Option<Vec<rmpv::Value>> {
    let mut cursor = std::io::Cursor::new(app_data);
    let value = rmpv::decode::read_value(&mut cursor).ok()?;
    if cursor.position() != app_data.len() as u64 {
        return None;
    }
    let rmpv::Value::Array(items) = value else {
        return None;
    };
    if items.len() < 7 {
        return None;
    }
    value_as_u64(&items[1])?;
    items[2].as_bool()?;
    value_as_u64(&items[3])?;
    value_as_u64(&items[4])?;
    let costs = items[5].as_array()?;
    if costs.len() < 3 {
        return None;
    }
    value_as_u64(&costs[0])?;
    value_as_u64(&costs[1])?;
    value_as_u64(&costs[2])?;
    items[6].as_map()?;
    Some(items)
}

fn value_as_u64(value: &rmpv::Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|value| u64::try_from(value).ok()))
}

fn value_as_display_name(value: &rmpv::Value) -> Option<String> {
    match value {
        rmpv::Value::Binary(bytes) => String::from_utf8(bytes.clone())
            .ok()
            .and_then(|value| lxmf::wire::announce::normalize_display_name(&value)),
        rmpv::Value::String(value) => value
            .as_str()
            .and_then(lxmf::wire::announce::normalize_display_name),
        _ => None,
    }
}

pub fn build_outbound_message(
    envelope: &MessageEnvelope,
    source_hash_hex: &str,
) -> AppResult<NativeLxmfOutbound> {
    if envelope.include_ticket {
        return Err(AppError::Unsupported(
            "native LXMF include-ticket sending is not implemented yet; disable ticket for this send"
                .into(),
        ));
    }

    let destination = parse_lxmf_hash(&envelope.peer_hash)?;
    let source = parse_lxmf_hash(source_hash_hex)?;
    let desired_method = match envelope.delivery_mode {
        DeliveryMode::Direct => lxmf::TransportMethod::Direct,
        DeliveryMode::Propagated => lxmf::TransportMethod::Propagated,
    };
    let content_size = envelope.title.len() + envelope.body.len();
    let delivery = lxmf::decide_delivery(desired_method, false, content_size)
        .map_err(|err| AppError::Runtime(format!("LXMF delivery decision failed: {err}")))?;

    let mut message = lxmf::Message::new();
    message.destination_hash = Some(destination);
    message.source_hash = Some(source);
    message.set_title_from_string(&envelope.title);
    message.set_content_from_string(&envelope.body);
    message.set_state(lxmf::wire::message::State::Outbound);
    let (fields, attachments) = attachment_fields_from_paths(&envelope.attachments)?;
    message.fields = fields;

    Ok(NativeLxmfOutbound {
        message,
        delivery,
        include_ticket: envelope.include_ticket,
        attachments,
    })
}

pub fn encode_signed_wire_message(
    outbound: &NativeLxmfOutbound,
    private_identity_bytes: &[u8],
) -> AppResult<Vec<u8>> {
    let signer = PrivateIdentity::from_private_key_bytes(private_identity_bytes)
        .map_err(|_| AppError::Runtime("native Reticulum identity is invalid".into()))?;
    outbound
        .message
        .to_wire(Some(&signer))
        .map_err(|err| AppError::Runtime(format!("LXMF wire encode failed: {err}")))
}

pub fn decode_wire_message(bytes: &[u8]) -> AppResult<MessageSummary> {
    decode_wire_message_inner(bytes, None)
}

pub fn decode_wire_message_storing_attachments(
    bytes: &[u8],
    attachments_dir: &Path,
) -> AppResult<MessageSummary> {
    decode_wire_message_inner(bytes, Some(attachments_dir))
}

fn decode_wire_message_inner(
    bytes: &[u8],
    attachments_dir: Option<&Path>,
) -> AppResult<MessageSummary> {
    let (wire, message) = decode_wire_and_message(bytes)?;
    let peer_hash = hex16(&wire.source);
    let message_id = hex32(&wire.message_id());
    let attachments = if let Some(attachments_dir) = attachments_dir {
        store_attachments_from_fields(message.fields.as_ref(), attachments_dir, &message_id)?
    } else {
        attachment_summaries_from_fields(message.fields.as_ref())
    };

    Ok(MessageSummary {
        peer_hash: peer_hash.clone(),
        peer_label: peer_hash.chars().take(8).collect(),
        title: message.title_as_string().unwrap_or_default(),
        content: message.content_as_string().unwrap_or_default(),
        timestamp: message.timestamp.unwrap_or_default(),
        transport_method: AppTransportMethod::Unknown("lxmf_wire".into()),
        delivered: false,
        failed: false,
        incoming: true,
        unread: true,
        message_id: Some(message_id),
        fields: BTreeMap::new(),
        attachments,
    })
}

pub fn decode_propagated_lxmf_data(
    lxmf_data: &[u8],
    private_identity_bytes: &[u8],
) -> AppResult<MessageSummary> {
    decode_propagated_lxmf_data_inner(lxmf_data, private_identity_bytes, None)
}

pub fn decode_propagated_lxmf_data_storing_attachments(
    lxmf_data: &[u8],
    private_identity_bytes: &[u8],
    attachments_dir: &Path,
) -> AppResult<MessageSummary> {
    decode_propagated_lxmf_data_inner(lxmf_data, private_identity_bytes, Some(attachments_dir))
}

fn decode_propagated_lxmf_data_inner(
    lxmf_data: &[u8],
    private_identity_bytes: &[u8],
    attachments_dir: Option<&Path>,
) -> AppResult<MessageSummary> {
    if let Ok(message) = decode_wire_message_inner(lxmf_data, attachments_dir) {
        return Ok(message);
    }
    if lxmf_data.len() <= 16 {
        return Err(AppError::Runtime(
            "propagated LXMF data is shorter than destination hash".into(),
        ));
    }

    let identity = PrivateIdentity::from_private_key_bytes(private_identity_bytes)
        .map_err(|_| AppError::Runtime("native Reticulum identity is invalid".into()))?;
    let destination_hash = &lxmf_data[..16];
    let encrypted = &lxmf_data[16..];
    let decrypted = decrypt_with_identity(&identity, destination_hash, encrypted)
        .map_err(|_| AppError::Runtime("propagated LXMF decrypt failed".into()))?;
    let mut wire = Vec::with_capacity(16 + decrypted.len());
    wire.extend_from_slice(destination_hash);
    wire.extend_from_slice(&decrypted);
    let mut message = decode_wire_message_inner(&wire, attachments_dir)?;
    message.transport_method = AppTransportMethod::Propagated;
    message.fields.insert(
        "native_lxmf_delivery_source".into(),
        "propagation_sync".into(),
    );
    Ok(message)
}

pub fn propagation_envelope_entries(bytes: &[u8]) -> AppResult<Vec<Vec<u8>>> {
    let mut cursor = std::io::Cursor::new(bytes);
    let value = rmpv::decode::read_value(&mut cursor)
        .map_err(|_| AppError::Runtime("LXMF propagation envelope decode failed".into()))?;
    if cursor.position() != bytes.len() as u64 {
        return Err(AppError::Runtime(
            "LXMF propagation envelope had trailing data".into(),
        ));
    }
    let entries = propagation_entries_from_value(&value)?;
    Ok(entries)
}

fn decode_wire_and_message(bytes: &[u8]) -> AppResult<(lxmf::WireMessage, lxmf::Message)> {
    if let Ok(wire) = lxmf::WireMessage::unpack_storage(bytes) {
        let packed = wire.pack().map_err(|err| {
            AppError::Runtime(format!("LXMF wire re-pack failed during decode: {err}"))
        })?;
        let message = lxmf::Message::from_wire(&packed)
            .map_err(|err| AppError::Runtime(format!("LXMF message decode failed: {err}")))?;
        return Ok((wire, message));
    }
    let wire = lxmf::WireMessage::unpack(bytes)
        .map_err(|err| AppError::Runtime(format!("LXMF wire decode failed: {err}")))?;
    let message = lxmf::Message::from_wire(bytes)
        .map_err(|err| AppError::Runtime(format!("LXMF message decode failed: {err}")))?;
    Ok((wire, message))
}

fn attachment_fields_from_paths(
    paths: &[std::path::PathBuf],
) -> AppResult<(Option<rmpv::Value>, Vec<AttachmentSummary>)> {
    let mut field_entries = Vec::new();
    let mut summaries = Vec::new();

    for path in paths {
        if !path.is_file() {
            continue;
        }
        let data = std::fs::read(path)
            .map_err(|err| AppError::Runtime(format!("failed to read LXMF attachment: {err}")))?;
        let name = attachment_file_name(path);
        field_entries.push(rmpv::Value::Array(vec![
            rmpv::Value::String(name.clone().into()),
            rmpv::Value::Binary(data.clone()),
        ]));
        summaries.push(AttachmentSummary {
            name,
            size: data.len() as u64,
            path: Some(path.clone()),
        });
    }

    if field_entries.is_empty() {
        return Ok((None, summaries));
    }

    Ok((
        Some(rmpv::Value::Map(vec![(
            rmpv::Value::Integer(FIELD_FILE_ATTACHMENTS.into()),
            rmpv::Value::Array(field_entries),
        )])),
        summaries,
    ))
}

fn attachment_file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("attachment")
        .to_string()
}

fn attachment_summaries_from_fields(fields: Option<&rmpv::Value>) -> Vec<AttachmentSummary> {
    attachment_entries_from_fields(fields)
        .into_iter()
        .map(|entry| AttachmentSummary {
            name: entry.name,
            size: entry.data.len() as u64,
            path: None,
        })
        .collect()
}

fn store_attachments_from_fields(
    fields: Option<&rmpv::Value>,
    attachments_dir: &Path,
    message_id: &str,
) -> AppResult<Vec<AttachmentSummary>> {
    let entries = attachment_entries_from_fields(fields);
    if entries.is_empty() {
        return Ok(Vec::new());
    }

    let message_dir = attachments_dir.join(safe_path_component(message_id));
    std::fs::create_dir_all(&message_dir)?;
    let mut summaries = Vec::with_capacity(entries.len());
    for (index, entry) in entries.into_iter().enumerate() {
        let file_name = format!("{}_{}", index, safe_path_component(&entry.name));
        let stored_path = next_available_path(&message_dir, &file_name);
        std::fs::write(&stored_path, &entry.data)?;
        summaries.push(AttachmentSummary {
            name: entry.name,
            size: entry.data.len() as u64,
            path: Some(stored_path),
        });
    }
    Ok(summaries)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AttachmentEntry {
    name: String,
    data: Vec<u8>,
}

fn attachment_entries_from_fields(fields: Option<&rmpv::Value>) -> Vec<AttachmentEntry> {
    let Some(rmpv::Value::Map(entries)) = fields else {
        return Vec::new();
    };
    let Some(value) = entries.iter().find_map(|(key, value)| {
        if field_key_matches_file_attachments(key) {
            Some(value)
        } else {
            None
        }
    }) else {
        return Vec::new();
    };
    let rmpv::Value::Array(items) = value else {
        return Vec::new();
    };

    items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| attachment_entry_from_value(index, item))
        .collect()
}

fn field_key_matches_file_attachments(key: &rmpv::Value) -> bool {
    match key {
        rmpv::Value::Integer(value) => value.as_i64() == Some(FIELD_FILE_ATTACHMENTS),
        rmpv::Value::String(value) => value.as_str() == Some("5"),
        _ => false,
    }
}

fn attachment_entry_from_value(index: usize, value: &rmpv::Value) -> Option<AttachmentEntry> {
    let rmpv::Value::Array(items) = value else {
        return None;
    };
    if items.len() < 2 {
        return None;
    }
    let name = items[0]
        .as_str()
        .map(|name| {
            Path::new(name)
                .file_name()
                .and_then(|file_name| file_name.to_str())
                .unwrap_or(name)
                .to_string()
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| format!("attachment_{index}"));
    let data = attachment_bytes_from_value(&items[1])?;
    Some(AttachmentEntry { name, data })
}

fn attachment_bytes_from_value(value: &rmpv::Value) -> Option<Vec<u8>> {
    match value {
        rmpv::Value::Binary(bytes) => Some(bytes.clone()),
        rmpv::Value::Array(bytes) => {
            let mut out = Vec::with_capacity(bytes.len());
            for byte in bytes {
                let value = byte
                    .as_u64()
                    .filter(|value| *value <= u8::MAX as u64)
                    .map(|value| value as u8)
                    .or_else(|| byte.as_i64().and_then(|value| u8::try_from(value).ok()))?;
                out.push(value);
            }
            Some(out)
        }
        _ => None,
    }
}

fn safe_path_component(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    let sanitized = sanitized.trim_matches('.');
    if sanitized.is_empty() {
        "attachment".into()
    } else {
        sanitized.to_string()
    }
}

fn next_available_path(dir: &Path, file_name: &str) -> std::path::PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }

    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("attachment");
    let extension = path.extension().and_then(|extension| extension.to_str());
    for index in 1.. {
        let name = if let Some(extension) = extension {
            format!("{stem}-{index}.{extension}")
        } else {
            format!("{stem}-{index}")
        };
        let candidate = dir.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("unbounded attachment path search")
}

fn propagation_entries_from_value(value: &rmpv::Value) -> AppResult<Vec<Vec<u8>>> {
    let rmpv::Value::Array(items) = value else {
        return Err(AppError::Runtime(
            "LXMF propagation envelope was not an array".into(),
        ));
    };
    if items.len() != 2 {
        return Err(AppError::Runtime(
            "LXMF propagation envelope did not contain timestamp and entries".into(),
        ));
    }
    let rmpv::Value::Array(entries) = &items[1] else {
        return Err(AppError::Runtime(
            "LXMF propagation envelope entries were not an array".into(),
        ));
    };
    entries
        .iter()
        .map(|entry| match entry {
            rmpv::Value::Binary(bytes) => Ok(bytes.clone()),
            _ => Err(AppError::Runtime(
                "LXMF propagation envelope entry was not binary".into(),
            )),
        })
        .collect()
}

pub fn app_transport_method(method: lxmf::TransportMethod) -> AppTransportMethod {
    match method {
        lxmf::TransportMethod::Direct | lxmf::TransportMethod::Opportunistic => {
            AppTransportMethod::Direct
        }
        lxmf::TransportMethod::Propagated => AppTransportMethod::Propagated,
        lxmf::TransportMethod::Paper => AppTransportMethod::Unknown("paper".into()),
    }
}

pub fn parse_lxmf_hash(value: &str) -> AppResult<[u8; 16]> {
    let value = value.trim();
    if value.len() != 32 {
        return Err(AppError::Runtime(
            "LXMF destination hash must be 32 hex characters".into(),
        ));
    }

    let mut out = [0u8; 16];
    for (index, byte) in out.iter_mut().enumerate() {
        let offset = index * 2;
        *byte = u8::from_str_radix(&value[offset..offset + 2], 16)
            .map_err(|_| AppError::Runtime("LXMF destination hash is not valid hex".into()))?;
    }
    Ok(out)
}

fn hex16(bytes: &[u8; 16]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hex32(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::IdentityMaterialProvider;
    use crate::messaging::DeliveryMode;
    use crate::runtime::native::identity::NativeReticulumIdentityProvider;

    const DEST: &str = "00112233445566778899aabbccddeeff";
    const SRC: &str = "ffeeddccbbaa99887766554433221100";

    #[test]
    fn lxmf_umbrella_wire_types_are_available_without_sdk_assumptions() {
        let api = native_lxmf_wire_api();

        assert!(api.message_type.contains("Message"));
        assert!(api.payload_type.contains("Payload"));
        assert!(api.wire_message_type.contains("WireMessage"));
    }

    #[test]
    fn native_lxmf_parity_is_explicit_about_ticket_and_stamp_support() {
        let parity = native_lxmf_parity();

        assert!(parity.payload_stamps_supported);
        assert!(parity.stamp_validation_supported);
        assert!(parity.stamp_generation_supported);
        assert!(!parity.include_ticket_supported);
    }

    #[test]
    fn lxmf_wire_payload_round_trips_msgpack() {
        let payload = lxmf::Payload::new(
            1.25,
            Some(b"body".to_vec()),
            Some(b"title".to_vec()),
            None,
            None,
        );

        let encoded = payload.to_msgpack().expect("encode payload");
        let decoded = lxmf::Payload::from_msgpack(&encoded).expect("decode payload");

        assert_eq!(decoded.timestamp, 1.25);
        assert_eq!(
            decoded.title.as_ref().map(|bytes| bytes.as_ref()),
            Some(&b"title"[..])
        );
        assert_eq!(
            decoded.content.as_ref().map(|bytes| bytes.as_ref()),
            Some(&b"body"[..])
        );
    }

    #[test]
    fn delivery_announce_app_data_extracts_display_name() {
        let encoded = encode_delivery_display_name_app_data("Alice Relay")
            .expect("encoded delivery app data");

        let display_name =
            delivery_display_name_from_app_data(encoded.as_slice()).expect("display name");

        assert_eq!(display_name, "Alice Relay");
    }

    #[test]
    fn propagation_announce_display_name_parser_reuses_python_metadata_shape() {
        let value = rmpv::Value::Array(vec![
            rmpv::Value::Nil,
            rmpv::Value::from(1_700_000_000_u64),
            rmpv::Value::Boolean(true),
            rmpv::Value::from(256_u64),
            rmpv::Value::from(10_240_u64),
            rmpv::Value::Array(vec![
                rmpv::Value::from(16_u64),
                rmpv::Value::from(3_u64),
                rmpv::Value::from(18_u64),
            ]),
            rmpv::Value::Map(vec![(
                rmpv::Value::from(0x01_u64),
                rmpv::Value::Binary(b"Relay Node".to_vec()),
            )]),
        ]);
        let mut encoded = Vec::new();
        rmpv::encode::write_value(&mut encoded, &value).expect("encode propagation announce");

        assert!(propagation_announce_data_is_valid(&encoded));
        assert_eq!(
            propagation_display_name_from_app_data(&encoded).as_deref(),
            Some("Relay Node")
        );
        assert_eq!(propagation_announce_stamp_costs(&encoded), vec![16, 3, 18]);
        assert_eq!(propagation_announce_target_stamp_cost(&encoded), Some(16));
        assert_eq!(propagation_display_name_from_app_data(b"not-msgpack"), None);
    }

    #[test]
    fn propagation_stamp_validation_strips_appended_stamp() {
        let mut lxm_data = vec![0x42; 180];
        lxm_data[0] = 0x91;
        let mut transient_data = lxm_data.clone();
        transient_data.extend_from_slice(&[0u8; 32]);

        let stamp = validate_propagation_stamp_any_cost(&transient_data, &[0])
            .expect("cost zero stamp validates");

        assert_eq!(stamp.lxm_data, lxm_data);
        assert_eq!(stamp.target_cost, 0);
        assert_eq!(stamp.transient_id.len(), 32);
    }

    #[test]
    fn signed_propagation_envelope_uses_encrypted_transient_and_generated_stamp() {
        let sender_provider = NativeReticulumIdentityProvider;
        let sender_private = sender_provider
            .create_identity_material("propagation-sender")
            .expect("sender identity");
        let sender = PrivateIdentity::from_private_key_bytes(&sender_private).expect("sender");
        let receiver = PrivateIdentity::new_from_rand(rand_core::OsRng);
        let receiver_identity = receiver.as_identity();
        let mut receiver_public = [0u8; 64];
        receiver_public[..32].copy_from_slice(receiver_identity.public_key_bytes());
        receiver_public[32..].copy_from_slice(receiver_identity.verifying_key_bytes());
        let envelope = MessageEnvelope {
            peer_hash: receiver.address_hash().to_hex_string(),
            title: "Propagated".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Propagated,
            include_ticket: false,
            attachments: Vec::new(),
        };
        let outbound = build_outbound_message(&envelope, &sender.address_hash().to_hex_string())
            .expect("outbound");

        let package = encode_signed_propagation_envelope(
            &outbound,
            &sender_private,
            receiver_public,
            Some(0),
            1,
        )
        .expect("propagation envelope");
        let entries = propagation_envelope_entries(package.envelope.as_slice()).expect("entries");
        let stamped = validate_propagation_stamp_any_cost(entries[0].as_slice(), &[0])
            .expect("generated stamp validates");
        let summary = decode_propagated_lxmf_data(
            stamped.lxm_data.as_slice(),
            &receiver.to_private_key_bytes(),
        )
        .expect("decode propagated data");

        assert_eq!(entries.len(), 1);
        assert_eq!(
            package.stamp.as_ref().map(|stamp| stamp.target_cost),
            Some(0)
        );
        assert_eq!(stamped.transient_id, package.transient_id);
        assert_eq!(summary.peer_hash, sender.address_hash().to_hex_string());
        assert_eq!(summary.title, "Propagated");
        assert_eq!(summary.content, "Body");
    }

    #[test]
    fn outbound_envelope_maps_to_lxmf_wire_message() {
        let envelope = MessageEnvelope {
            peer_hash: DEST.into(),
            title: "Subject".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            attachments: Vec::new(),
        };

        let outbound = build_outbound_message(&envelope, SRC).expect("outbound");

        assert_eq!(
            outbound.message.destination_hash,
            Some(parse_lxmf_hash(DEST).unwrap())
        );
        assert_eq!(
            outbound.message.source_hash,
            Some(parse_lxmf_hash(SRC).unwrap())
        );
        assert_eq!(
            outbound.message.title_as_string().as_deref(),
            Some("Subject")
        );
        assert_eq!(
            outbound.message.content_as_string().as_deref(),
            Some("Body")
        );
        assert_eq!(outbound.delivery.method, lxmf::TransportMethod::Direct);
    }

    #[test]
    fn propagated_delivery_maps_to_lxmf_propagated_without_ticket() {
        let envelope = MessageEnvelope {
            peer_hash: DEST.into(),
            title: "Subject".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Propagated,
            include_ticket: false,
            attachments: Vec::new(),
        };

        let outbound = build_outbound_message(&envelope, SRC).expect("outbound");

        assert_eq!(outbound.delivery.method, lxmf::TransportMethod::Propagated);
        assert!(!outbound.include_ticket);
    }

    #[test]
    fn include_ticket_send_fails_clearly_until_native_ticket_api_is_verified() {
        let envelope = MessageEnvelope {
            peer_hash: DEST.into(),
            title: "Subject".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: true,
            attachments: Vec::new(),
        };

        let error = build_outbound_message(&envelope, SRC).expect_err("ticket unsupported");

        assert!(error.to_string().contains("include-ticket"));
    }

    #[test]
    fn signed_wire_message_decodes_to_message_summary() {
        let provider = NativeReticulumIdentityProvider;
        let private = provider
            .create_identity_material("sender")
            .expect("native identity");
        let signer = PrivateIdentity::from_private_key_bytes(&private).expect("signer");
        let source = signer.address_hash().to_hex_string();
        let envelope = MessageEnvelope {
            peer_hash: DEST.into(),
            title: "Subject".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            attachments: Vec::new(),
        };
        let outbound = build_outbound_message(&envelope, &source).expect("outbound");

        let wire = encode_signed_wire_message(&outbound, &private).expect("encode");
        let summary = decode_wire_message(&wire).expect("decode");

        assert_eq!(summary.peer_hash, source);
        assert_eq!(summary.title, "Subject");
        assert_eq!(summary.content, "Body");
        assert!(summary.incoming);
        assert!(summary.unread);
        assert!(summary.message_id.is_some());
    }

    #[test]
    fn outbound_envelope_encodes_python_style_file_attachments() {
        let provider = NativeReticulumIdentityProvider;
        let private = provider
            .create_identity_material("sender")
            .expect("native identity");
        let signer = PrivateIdentity::from_private_key_bytes(&private).expect("signer");
        let source = signer.address_hash().to_hex_string();
        let attachment_dir = unique_test_path("omenbrowser-lxmf-attachment");
        std::fs::create_dir_all(&attachment_dir).expect("create attachment dir");
        let attachment = attachment_dir.join("omenbrowser-lxmf-attachment.bin");
        std::fs::write(&attachment, b"attached bytes").expect("write attachment");
        let envelope = MessageEnvelope {
            peer_hash: DEST.into(),
            title: "Subject".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            attachments: vec![attachment.clone()],
        };

        let outbound = build_outbound_message(&envelope, &source).expect("outbound");
        let fields = outbound.message.fields.as_ref().expect("attachment fields");
        let summaries = attachment_summaries_from_fields(Some(fields));
        let wire = encode_signed_wire_message(&outbound, &private).expect("encode");
        let decoded = decode_wire_message(&wire).expect("decode");
        let stored_dir = unique_test_path("omenbrowser-lxmf-stored-attachments");
        let stored =
            decode_wire_message_storing_attachments(&wire, &stored_dir).expect("decode stored");

        assert_eq!(outbound.attachments.len(), 1);
        assert_eq!(
            outbound.attachments[0].name,
            "omenbrowser-lxmf-attachment.bin"
        );
        assert_eq!(outbound.attachments[0].size, 14);
        assert_eq!(summaries[0].name, "omenbrowser-lxmf-attachment.bin");
        assert_eq!(summaries[0].size, 14);
        assert_eq!(
            decoded.attachments[0].name,
            "omenbrowser-lxmf-attachment.bin"
        );
        assert_eq!(decoded.attachments[0].size, 14);
        assert_eq!(decoded.attachments[0].path, None);
        assert_eq!(
            stored.attachments[0].name,
            "omenbrowser-lxmf-attachment.bin"
        );
        assert_eq!(stored.attachments[0].size, 14);
        let stored_path = stored.attachments[0].path.as_ref().expect("stored path");
        assert_eq!(
            std::fs::read(stored_path).expect("stored bytes"),
            b"attached bytes"
        );
        assert!(stored_path.starts_with(&stored_dir));

        let _ = std::fs::remove_file(attachment);
        let _ = std::fs::remove_dir(attachment_dir);
        let _ = std::fs::remove_dir_all(stored_dir);
    }

    #[test]
    fn outbound_attachment_encoding_skips_missing_paths_like_python_adapter() {
        let envelope = MessageEnvelope {
            peer_hash: DEST.into(),
            title: "Subject".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            attachments: vec![unique_test_path("omenbrowser-missing-attachment.bin")],
        };

        let outbound = build_outbound_message(&envelope, SRC).expect("outbound");

        assert!(outbound.message.fields.is_none());
        assert!(outbound.attachments.is_empty());
    }

    #[test]
    fn propagated_lxmf_data_decrypts_to_message_summary() {
        let sender = PrivateIdentity::new_from_rand(rand_core::OsRng);
        let receiver = PrivateIdentity::new_from_rand(rand_core::OsRng);
        let receiver_hash =
            parse_lxmf_hash(&receiver.address_hash().to_hex_string()).expect("receiver hash");
        let sender_hash =
            parse_lxmf_hash(&sender.address_hash().to_hex_string()).expect("sender hash");
        let payload = lxmf::Payload::new(
            42.0,
            Some(b"Body".to_vec()),
            Some(b"Subject".to_vec()),
            None,
            None,
        );
        let mut wire = lxmf::WireMessage::new(receiver_hash, sender_hash, payload);
        wire.sign(&sender).expect("sign");
        let (lxmf_data, _transient_id) = wire
            .pack_propagation_transient_with_rng(receiver.as_identity(), rand_core::OsRng)
            .expect("propagation data");

        let summary = decode_propagated_lxmf_data(
            lxmf_data.as_slice(),
            receiver.to_private_key_bytes().as_slice(),
        )
        .expect("decode propagated");

        assert_eq!(summary.peer_hash, sender.address_hash().to_hex_string());
        assert_eq!(summary.title, "Subject");
        assert_eq!(summary.content, "Body");
        assert_eq!(summary.transport_method, AppTransportMethod::Propagated);
        assert_eq!(
            summary
                .fields
                .get("native_lxmf_delivery_source")
                .map(String::as_str),
            Some("propagation_sync")
        );
    }

    #[test]
    fn propagation_envelope_entries_extract_binary_payloads() {
        let envelope =
            lxmf::WireMessage::pack_propagation_envelope(42.0, b"lxmf-data", Some(&[0xAB; 32]))
                .expect("envelope");

        let entries = propagation_envelope_entries(envelope.as_slice()).expect("entries");

        assert_eq!(entries.len(), 1);
        assert!(entries[0].starts_with(b"lxmf-data"));
        assert!(entries[0].ends_with(&[0xAB; 32]));
    }

    #[test]
    fn invalid_destination_is_rejected_before_wire_encoding() {
        let envelope = MessageEnvelope {
            peer_hash: "not-a-hash".into(),
            title: "Subject".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            attachments: Vec::new(),
        };

        let error = build_outbound_message(&envelope, SRC).expect_err("invalid hash");

        assert!(error.to_string().contains("32 hex"));
    }

    fn unique_test_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ))
    }
}
