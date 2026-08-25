use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use rand_core::RngCore;
use reticulum_rs::core::identity::{Identity, PrivateIdentity};
use reticulum_rs::core::ratchets::decrypt_with_identity;
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};
use crate::messaging::{
    AttachmentSummary, DeliveryMode, MessageEnvelope, MessageSummary, NativeLxmfReplyTicket,
    TransportMethod as AppTransportMethod, LXMF_SOURCE_AUTHENTICATED_FIELD,
};
use crate::storage::files::atomic_replace;

const FIELD_EMBEDDED_LXMS: i64 = 0x01;
const FIELD_TELEMETRY: i64 = 0x02;
const FIELD_TELEMETRY_STREAM: i64 = 0x03;
const FIELD_ICON_APPEARANCE: i64 = 0x04;
const FIELD_FILE_ATTACHMENTS: i64 = 0x05;
const FIELD_IMAGE: i64 = 0x06;
const FIELD_AUDIO: i64 = 0x07;
const FIELD_THREAD: i64 = 0x08;
const FIELD_COMMANDS: i64 = 0x09;
const FIELD_RESULTS: i64 = 0x0A;
const FIELD_GROUP: i64 = 0x0B;
const FIELD_TICKET: i64 = 0x0C;
const FIELD_EVENT: i64 = 0x0D;
const FIELD_RNR_REFS: i64 = 0x0E;
const FIELD_RENDERER: i64 = 0x0F;
const FIELD_CUSTOM_TYPE: i64 = 0xFB;
const FIELD_CUSTOM_DATA: i64 = 0xFC;
const FIELD_CUSTOM_META: i64 = 0xFD;
const FIELD_NON_SPECIFIC: i64 = 0xFE;
const FIELD_DEBUG: i64 = 0xFF;

const RENDERER_PLAIN: u64 = 0x00;
const RENDERER_MICRON: u64 = 0x01;
const RENDERER_MARKDOWN: u64 = 0x02;
const RENDERER_BBCODE: u64 = 0x03;
const LXMF_TICKET_LENGTH: usize = 16;
const LXMF_TICKET_EXPIRY_SECONDS: f64 = 21.0 * 24.0 * 60.0 * 60.0;
const LXMF_STAMP_SIZE: usize = 32;
const PROPAGATION_LXMF_OVERHEAD: usize = 112;
const DIRECT_WORKBLOCK_EXPAND_ROUNDS: u32 = 3000;
const PROPAGATION_WORKBLOCK_EXPAND_ROUNDS: u32 = 1000;
pub const DEFAULT_PROPAGATION_STAMP_TARGET_COST: u8 = 16;
pub const DEFAULT_PROPAGATION_STAMP_MAX_ATTEMPTS: u64 = 1 << 22;
pub const DEFAULT_DIRECT_STAMP_MAX_ATTEMPTS: u64 = 1 << 22;
pub const CLEAN_DIRECT_STAMP_MAX_COST: u8 = 8;
pub const CLEAN_DIRECT_STAMP_MAX_ATTEMPTS: u64 = 1 << 16;
const MAX_LXMF_ANNOUNCE_BYTES: usize = 4 * 1024;
const MAX_LXMF_ANNOUNCE_CONTAINER_ITEMS: usize = 64;
const MAX_LXMF_ANNOUNCE_TOTAL_VALUES: usize = 256;
const MAX_LXMF_ANNOUNCE_DEPTH: usize = 8;
pub const MAX_LXMF_PROPAGATION_ENVELOPE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_LXMF_PROPAGATION_ENTRY_BYTES: usize = 8 * 1024 * 1024;
const MAX_LXMF_PROPAGATION_CONTAINER_ITEMS: usize = 256;
const MAX_LXMF_PROPAGATION_TOTAL_VALUES: usize = 512;
const MAX_LXMF_PROPAGATION_DEPTH: usize = 4;
const MAX_LXMF_WIRE_BYTES: usize = 16 * 1024 * 1024;
const MAX_LXMF_WIRE_SCALAR_BYTES: usize = 8 * 1024 * 1024;
const MAX_LXMF_WIRE_CONTAINER_ITEMS: usize = 4096;
const MAX_LXMF_WIRE_TOTAL_VALUES: usize = 8192;
const MAX_LXMF_WIRE_DEPTH: usize = 16;
const LXMF_RAW_WIRE_HEADER_BYTES: usize = 16 + 16 + 64;
const LXMF_STORAGE_MAGIC: &[u8; 8] = b"LXMFSTR0";
pub const MAX_LXMF_ATTACHMENT_ITEMS: usize = 64;
pub const MAX_LXMF_ATTACHMENT_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_LXMF_ATTACHMENT_TOTAL_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_LXMF_ATTACHMENT_NAME_BYTES: usize = 4 * 1024;
const MAX_LXMF_ATTACHMENT_PATH_COMPONENT_BYTES: usize = 200;
static LXMF_ATTACHMENT_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

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
    pub reply_ticket_used: bool,
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
        stamp_validation_supported: true,
        stamp_generation_supported: true,
        include_ticket_supported: true,
    }
}

pub fn native_delivery_type_name() -> &'static str {
    "lxmf::Message"
}

pub fn delivery_display_name_from_app_data(app_data: &[u8]) -> Option<String> {
    validate_lxmf_announce_msgpack(app_data).ok()?;
    lxmf::wire::announce::display_name_from_delivery_app_data(app_data)
        .ok()
        .flatten()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectStampPolicy {
    Unknown,
    NotRequired,
    Required { cost: u8 },
    TicketAccepted,
    Unsupported,
}

pub fn delivery_announce_direct_stamp_policy(
    app_data: Option<&[u8]>,
    valid_reply_ticket: bool,
) -> DirectStampPolicy {
    if valid_reply_ticket {
        return DirectStampPolicy::TicketAccepted;
    }
    let Some(app_data) = app_data.filter(|data| !data.is_empty()) else {
        return DirectStampPolicy::Unknown;
    };
    if validate_lxmf_announce_msgpack(app_data).is_err() {
        return DirectStampPolicy::Unknown;
    }
    let mut cursor = std::io::Cursor::new(app_data);
    let Ok(rmpv::Value::Array(items)) = rmpv::decode::read_value(&mut cursor) else {
        return DirectStampPolicy::Unknown;
    };
    if cursor.position() != app_data.len() as u64 {
        return DirectStampPolicy::Unsupported;
    }
    match items.get(1) {
        Some(rmpv::Value::Nil) => DirectStampPolicy::NotRequired,
        Some(value) => match value_as_u64(value) {
            Some(cost @ 1..=254) => DirectStampPolicy::Required { cost: cost as u8 },
            _ => DirectStampPolicy::Unsupported,
        },
        None => DirectStampPolicy::Unknown,
    }
}

pub fn delivery_announce_stamp_cost(app_data: &[u8]) -> Option<u8> {
    match delivery_announce_direct_stamp_policy(Some(app_data), false) {
        DirectStampPolicy::Required { cost } => Some(cost),
        DirectStampPolicy::Unknown
        | DirectStampPolicy::NotRequired
        | DirectStampPolicy::TicketAccepted
        | DirectStampPolicy::Unsupported => None,
    }
}

pub fn encode_delivery_display_name_app_data(display_name: &str) -> AppResult<Vec<u8>> {
    lxmf::wire::announce::encode_delivery_display_name_app_data(display_name).map_err(|error| {
        AppError::Runtime(format!("LXMF delivery announce app-data failed: {error}"))
    })
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
pub struct GeneratedDirectStamp {
    pub message_id: [u8; 32],
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
    let timestamp = wire.payload.timestamp;
    let (lxm_data, transient_id) =
        pack_identity_salted_propagation_transient(&wire, recipient_public_key)?;
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

fn pack_identity_salted_propagation_transient(
    wire: &lxmf::WireMessage,
    recipient_public_key: [u8; 64],
) -> AppResult<(Vec<u8>, [u8; 32])> {
    let recipient =
        Identity::try_new_from_slices(&recipient_public_key[..32], &recipient_public_key[32..])
            .map_err(|err| {
                AppError::Runtime(format!("LXMF recipient identity is invalid: {err}"))
            })?;
    wire.pack_propagation_transient_with_rng(&recipient, rand_core::OsRng)
        .map_err(|err| {
            AppError::Runtime(format!("LXMF propagation transient encode failed: {err}"))
        })
}

pub fn generate_propagation_stamp_for_transient(
    lxm_data: &[u8],
    transient_id: [u8; 32],
    target_cost: u8,
    max_attempts: u64,
) -> AppResult<GeneratedPropagationStamp> {
    let workblock = stamp_workblock(&transient_id, PROPAGATION_WORKBLOCK_EXPAND_ROUNDS)?;
    let mut stamp = vec![0u8; LXMF_STAMP_SIZE];
    for attempt in 1..=max_attempts {
        rand_core::OsRng.fill_bytes(&mut stamp);
        if stamp_valid(&stamp, target_cost, &workblock) {
            let stamp_value = stamp_value(&workblock, &stamp);
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

fn validate_propagation_stamp(
    transient_data: &[u8],
    target_cost: u8,
) -> Option<([u8; 32], Vec<u8>, u32)> {
    if transient_data.len() <= PROPAGATION_LXMF_OVERHEAD + LXMF_STAMP_SIZE {
        return None;
    }
    let split = transient_data.len() - LXMF_STAMP_SIZE;
    let lxm_data = &transient_data[..split];
    let stamp = &transient_data[split..];
    let digest = Sha256::digest(lxm_data);
    let mut transient_id = [0u8; 32];
    transient_id.copy_from_slice(&digest);
    let workblock = stamp_workblock(&transient_id, PROPAGATION_WORKBLOCK_EXPAND_ROUNDS).ok()?;
    if stamp_valid(stamp, target_cost, &workblock) {
        let stamp_value = stamp_value(&workblock, stamp);
        Some((transient_id, lxm_data.to_vec(), stamp_value))
    } else {
        None
    }
}

fn stamp_workblock(material: &[u8], expand_rounds: u32) -> AppResult<Vec<u8>> {
    let mut workblock = Vec::with_capacity(expand_rounds as usize * 256);
    for n in 0..expand_rounds {
        let mut packed_round = Vec::new();
        rmpv::encode::write_value(&mut packed_round, &rmpv::Value::Integer(n.into()))
            .map_err(|err| AppError::Runtime(format!("LXMF stamp round encode failed: {err}")))?;
        let salt = Sha256::digest([material, packed_round.as_slice()].concat());
        let hkdf = hkdf::Hkdf::<Sha256>::new(Some(salt.as_slice()), material);
        let mut block = [0u8; 256];
        hkdf.expand(&[], &mut block)
            .map_err(|_| AppError::Runtime("LXMF stamp workblock HKDF expand failed".into()))?;
        workblock.extend_from_slice(&block);
    }
    Ok(workblock)
}

fn stamp_value(workblock: &[u8], stamp: &[u8]) -> u32 {
    let digest = Sha256::digest([workblock, stamp].concat());
    let mut value = 0u32;
    for byte in digest {
        let leading = byte.leading_zeros().min(8);
        value += leading;
        if leading < 8 {
            break;
        }
    }
    value
}

fn stamp_valid(stamp: &[u8], target_cost: u8, workblock: &[u8]) -> bool {
    if target_cost == 0 {
        return true;
    }
    stamp_value(workblock, stamp) >= u32::from(target_cost)
}

fn parse_propagation_announce_data(app_data: &[u8]) -> Option<Vec<rmpv::Value>> {
    validate_lxmf_announce_msgpack(app_data).ok()?;
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

fn validate_lxmf_announce_msgpack(app_data: &[u8]) -> AppResult<()> {
    crate::msgpack::validate_msgpack_with_limits(
        app_data,
        MAX_LXMF_ANNOUNCE_BYTES,
        MAX_LXMF_ANNOUNCE_BYTES,
        MAX_LXMF_ANNOUNCE_CONTAINER_ITEMS,
        MAX_LXMF_ANNOUNCE_TOTAL_VALUES,
        MAX_LXMF_ANNOUNCE_DEPTH,
    )
    .map_err(|error| AppError::Runtime(format!("LXMF announce decode rejected: {error}")))
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
    let mut fields = fields;
    if envelope.include_ticket {
        insert_lxmf_field(&mut fields, FIELD_TICKET, generate_lxmf_ticket_field());
    }
    message.fields = fields;
    let mut reply_ticket_used = false;
    if let Some(ticket) = envelope.native_reply_ticket.as_ref() {
        if ticket.expires > current_unix_secs_f64() {
            apply_reply_ticket_stamp(&mut message, ticket)?;
            reply_ticket_used = true;
        }
    }

    Ok(NativeLxmfOutbound {
        message,
        delivery,
        include_ticket: envelope.include_ticket,
        reply_ticket_used,
        attachments,
    })
}

fn apply_reply_ticket_stamp(
    message: &mut lxmf::Message,
    ticket: &NativeLxmfReplyTicket,
) -> AppResult<()> {
    let message_id = lxmf_message_id(message)?;
    let stamp = ticket_stamp_for_message(&ticket.ticket, &message_id)?;
    message.set_stamp_from_bytes(&stamp);
    Ok(())
}

pub fn apply_direct_stamp_if_needed(
    outbound: &mut NativeLxmfOutbound,
    target_cost: Option<u8>,
    max_attempts: u64,
) -> AppResult<Option<GeneratedDirectStamp>> {
    if outbound.reply_ticket_used || outbound.message.stamp.is_some() {
        return Ok(None);
    }
    let Some(target_cost) = target_cost else {
        return Ok(None);
    };
    if target_cost == 0 {
        return Ok(None);
    }
    let message_id = lxmf_message_id(&mut outbound.message)?;
    let generated = generate_direct_stamp_for_message(message_id, target_cost, max_attempts)?;
    outbound.message.set_stamp_from_bytes(&generated.stamp);
    Ok(Some(generated))
}

pub fn generate_direct_stamp_for_message(
    message_id: [u8; 32],
    target_cost: u8,
    max_attempts: u64,
) -> AppResult<GeneratedDirectStamp> {
    generate_direct_stamp_for_message_cancellable(message_id, target_cost, max_attempts, || false)
}

pub fn generate_direct_stamp_for_message_cancellable(
    message_id: [u8; 32],
    target_cost: u8,
    max_attempts: u64,
    mut cancelled: impl FnMut() -> bool,
) -> AppResult<GeneratedDirectStamp> {
    if cancelled() {
        return Err(AppError::Runtime(
            "LXMF direct stamp generation cancelled before work".into(),
        ));
    }
    let workblock = stamp_workblock(&message_id, DIRECT_WORKBLOCK_EXPAND_ROUNDS)?;
    let mut stamp = vec![0u8; LXMF_STAMP_SIZE];
    for attempt in 1..=max_attempts {
        if cancelled() {
            return Err(AppError::Runtime(format!(
                "LXMF direct stamp generation cancelled after {} attempts",
                attempt - 1
            )));
        }
        rand_core::OsRng.fill_bytes(&mut stamp);
        if stamp_valid(&stamp, target_cost, &workblock) {
            let stamp_value = stamp_value(&workblock, &stamp);
            return Ok(GeneratedDirectStamp {
                message_id,
                stamp,
                stamp_value,
                target_cost,
                attempts: attempt,
            });
        }
    }
    Err(AppError::Runtime(format!(
        "LXMF direct stamp generation did not find a cost {target_cost} stamp within {max_attempts} attempts"
    )))
}

pub fn validate_direct_stamp(message_id: &[u8; 32], stamp: &[u8], target_cost: u8) -> Option<u32> {
    let workblock = stamp_workblock(message_id, DIRECT_WORKBLOCK_EXPAND_ROUNDS).ok()?;
    if stamp_valid(stamp, target_cost, &workblock) {
        Some(stamp_value(&workblock, stamp))
    } else {
        None
    }
}

fn lxmf_message_id(message: &mut lxmf::Message) -> AppResult<[u8; 32]> {
    let destination = message
        .destination_hash
        .ok_or_else(|| AppError::Runtime("LXMF stamp requires destination hash".into()))?;
    let source = message
        .source_hash
        .ok_or_else(|| AppError::Runtime("LXMF stamp requires source hash".into()))?;
    let timestamp = message.timestamp.unwrap_or_else(current_unix_secs_f64);
    message.timestamp = Some(timestamp);
    let payload = lxmf::Payload::new(
        timestamp,
        Some(message.content.clone()),
        Some(message.title.clone()),
        message.fields.clone(),
        None,
    );
    let wire = lxmf::WireMessage::new(destination, source, payload);
    wire.try_message_id()
        .map_err(|err| AppError::Runtime(format!("LXMF message ID encode failed: {err}")))
}

pub fn ticket_entry_from_fields(fields: Option<&rmpv::Value>) -> Option<(f64, Vec<u8>)> {
    let rmpv::Value::Map(entries) = fields? else {
        return None;
    };
    let value = entries.iter().find_map(|(key, value)| {
        if field_key_matches_i64(key, FIELD_TICKET) {
            Some(value)
        } else {
            None
        }
    })?;
    let rmpv::Value::Array(items) = value else {
        return None;
    };
    if items.len() < 2 {
        return None;
    }
    let expires = items[0]
        .as_f64()
        .or_else(|| items[0].as_u64().map(|value| value as f64))
        .or_else(|| items[0].as_i64().map(|value| value as f64))?;
    let ticket = match &items[1] {
        rmpv::Value::Binary(bytes) => bytes.clone(),
        _ => return None,
    };
    Some((expires, ticket))
}

pub fn ticket_stamp_for_message(ticket: &[u8], message_id: &[u8; 32]) -> AppResult<[u8; 16]> {
    if ticket.len() != LXMF_TICKET_LENGTH {
        return Err(AppError::Runtime(format!(
            "LXMF ticket must be {LXMF_TICKET_LENGTH} bytes"
        )));
    }
    let mut hasher = Sha256::new();
    hasher.update(ticket);
    hasher.update(message_id);
    let digest = hasher.finalize();
    let mut stamp = [0u8; 16];
    stamp.copy_from_slice(&digest[..16]);
    Ok(stamp)
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
    decode_wire_message_inner(bytes, None, None)
}

pub fn decode_wire_message_storing_attachments(
    bytes: &[u8],
    attachments_dir: &Path,
) -> AppResult<MessageSummary> {
    decode_wire_message_inner(bytes, Some(attachments_dir), None)
}

pub fn decode_verified_wire_message(
    bytes: &[u8],
    source_identity: &Identity,
) -> AppResult<MessageSummary> {
    decode_wire_message_inner(bytes, None, Some(source_identity))
}

pub fn decode_verified_wire_message_storing_attachments(
    bytes: &[u8],
    source_identity: &Identity,
    attachments_dir: &Path,
) -> AppResult<MessageSummary> {
    decode_wire_message_inner(bytes, Some(attachments_dir), Some(source_identity))
}

pub fn wire_source_hash(bytes: &[u8]) -> AppResult<[u8; 16]> {
    let (wire, _) = decode_wire_and_message(bytes)?;
    Ok(wire.source)
}

fn decode_wire_message_inner(
    bytes: &[u8],
    attachments_dir: Option<&Path>,
    source_identity: Option<&Identity>,
) -> AppResult<MessageSummary> {
    let (wire, message) = decode_wire_and_message(bytes)?;
    let mut source_authenticated = false;
    if let Some(source_identity) = source_identity {
        let expected_source = lxmf_delivery_destination_hash(source_identity);
        if wire.source != expected_source {
            return Err(AppError::Runtime(
                "LXMF source identity does not match signed source destination".into(),
            ));
        }
        if !wire.verify(source_identity).map_err(|err| {
            AppError::Runtime(format!("LXMF signature verification failed: {err}"))
        })? {
            return Err(AppError::Runtime(
                "LXMF signature is missing or invalid".into(),
            ));
        }
        source_authenticated = true;
    }
    let peer_hash = hex16(&wire.source);
    let message_id = hex32(&wire.try_message_id().map_err(|err| {
        AppError::Runtime(format!("LXMF decoded message ID encode failed: {err}"))
    })?);
    let attachments = if let Some(attachments_dir) = attachments_dir {
        store_attachments_from_fields(message.fields.as_ref(), attachments_dir, &message_id)?
    } else {
        attachment_summaries_from_fields(message.fields.as_ref())?
    };
    let mut fields = native_lxmf_summary_fields_from_message_fields(message.fields.as_ref());
    if source_authenticated {
        fields.insert(LXMF_SOURCE_AUTHENTICATED_FIELD.into(), "true".into());
    }

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
        fields,
        attachments,
    })
}

fn lxmf_delivery_destination_hash(identity: &Identity) -> [u8; 16] {
    let destination = reticulum_rs::core::destination::Destination::<
        Identity,
        reticulum_rs::core::destination::Output,
        reticulum_rs::core::destination::Single,
    >::new(
        *identity,
        reticulum_rs::core::destination::DestinationName::new("lxmf", "delivery"),
    );
    let mut destination_hash = [0u8; 16];
    destination_hash.copy_from_slice(destination.desc.address_hash.as_slice());
    destination_hash
}

fn native_lxmf_summary_fields_from_message_fields(
    message_fields: Option<&rmpv::Value>,
) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    annotate_lxmf_field_presence(&mut fields, message_fields);
    if let Some((expires, ticket)) = ticket_entry_from_fields(message_fields) {
        fields.insert("native_lxmf_reply_ticket".into(), hex_bytes(&ticket));
        fields.insert(
            "native_lxmf_reply_ticket_expires".into(),
            format!("{expires:.3}"),
        );
        fields.insert(
            "native_lxmf_reply_ticket_state".into(),
            if expires > current_unix_secs_f64() {
                "valid"
            } else {
                "expired"
            }
            .into(),
        );
    }
    fields
}

fn annotate_lxmf_field_presence(
    fields: &mut BTreeMap<String, String>,
    message_fields: Option<&rmpv::Value>,
) {
    let Some(rmpv::Value::Map(entries)) = message_fields else {
        return;
    };
    let mut official = Vec::new();
    let mut custom = Vec::new();

    for (key, value) in entries {
        let Some(field_id) = field_key_as_i64(key) else {
            continue;
        };
        if let Some(name) = lxmf_field_name(field_id) {
            official.push(name);
            fields.insert(
                format!("native_lxmf_field_{name}"),
                lxmf_field_value_summary(value),
            );
            if field_id == FIELD_RENDERER {
                if let Some((renderer_id, renderer_name)) = lxmf_renderer_name(value) {
                    fields.insert("native_lxmf_renderer".into(), renderer_name.into());
                    fields.insert("native_lxmf_renderer_id".into(), renderer_id.to_string());
                }
            }
            if field_id == FIELD_THREAD {
                annotate_lxmf_thread_field(fields, value);
            }
        } else if field_id >= FIELD_CUSTOM_TYPE {
            custom.push(format!("0x{field_id:02x}"));
        }
    }

    if !official.is_empty() {
        official.sort_unstable();
        official.dedup();
        fields.insert("native_lxmf_fields".into(), official.join(","));
    }
    if !custom.is_empty() {
        custom.sort_unstable();
        custom.dedup();
        fields.insert("native_lxmf_custom_fields".into(), custom.join(","));
    }
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

pub fn lxmf_delivery_destination_hash_from_private_identity_bytes(
    private_identity_bytes: &[u8],
) -> AppResult<[u8; 16]> {
    let identity = PrivateIdentity::from_private_key_bytes(private_identity_bytes)
        .map_err(|_| AppError::Runtime("native Reticulum identity is invalid".into()))?;
    let destination = reticulum_rs::core::destination::Destination::<
        PrivateIdentity,
        reticulum_rs::core::destination::Input,
        reticulum_rs::core::destination::Single,
    >::new(
        identity,
        reticulum_rs::core::destination::DestinationName::new("lxmf", "delivery"),
    );
    let mut destination_hash = [0u8; 16];
    destination_hash.copy_from_slice(destination.desc.address_hash.as_slice());
    Ok(destination_hash)
}

pub fn propagated_lxmf_destination_hash(lxmf_data: &[u8]) -> Option<[u8; 16]> {
    if lxmf_data.len() <= 16 {
        return None;
    }
    let mut destination = [0u8; 16];
    destination.copy_from_slice(&lxmf_data[..16]);
    Some(destination)
}

fn decode_propagated_lxmf_data_inner(
    lxmf_data: &[u8],
    private_identity_bytes: &[u8],
    attachments_dir: Option<&Path>,
) -> AppResult<MessageSummary> {
    if let Ok(message) = decode_wire_message_inner(lxmf_data, attachments_dir, None) {
        return Ok(message);
    }
    let wire = unpack_propagated_lxmf_wire(lxmf_data, private_identity_bytes)?;
    let message = decode_wire_message_inner(&wire, attachments_dir, None)?;
    Ok(mark_message_propagated(message))
}

pub fn unpack_propagated_lxmf_wire(
    lxmf_data: &[u8],
    private_identity_bytes: &[u8],
) -> AppResult<Vec<u8>> {
    if decode_wire_and_message(lxmf_data).is_ok() {
        return Ok(lxmf_data.to_vec());
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
    let decrypted = decrypt_with_identity(&identity, identity.address_hash().as_slice(), encrypted)
        .map_err(|_| AppError::Runtime("propagated LXMF decrypt failed".into()))?;
    let mut wire = Vec::with_capacity(16 + decrypted.len());
    wire.extend_from_slice(destination_hash);
    wire.extend_from_slice(&decrypted);
    preflight_lxmf_wire(&wire)?;
    Ok(wire)
}

pub fn decode_verified_propagated_wire_message_storing_attachments(
    wire: &[u8],
    source_identity: &Identity,
    attachments_dir: &Path,
) -> AppResult<MessageSummary> {
    let message = decode_wire_message_inner(wire, Some(attachments_dir), Some(source_identity))?;
    Ok(mark_message_propagated(message))
}

fn mark_message_propagated(mut message: MessageSummary) -> MessageSummary {
    message.transport_method = AppTransportMethod::Propagated;
    message.fields.insert(
        "native_lxmf_delivery_source".into(),
        "propagation_sync".into(),
    );
    message
}

pub fn propagation_envelope_entries(bytes: &[u8]) -> AppResult<Vec<Vec<u8>>> {
    crate::msgpack::validate_msgpack_with_limits(
        bytes,
        MAX_LXMF_PROPAGATION_ENVELOPE_BYTES,
        MAX_LXMF_PROPAGATION_ENTRY_BYTES,
        MAX_LXMF_PROPAGATION_CONTAINER_ITEMS,
        MAX_LXMF_PROPAGATION_TOTAL_VALUES,
        MAX_LXMF_PROPAGATION_DEPTH,
    )
    .map_err(|error| AppError::Runtime(format!("LXMF propagation envelope rejected: {error}")))?;
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
    preflight_lxmf_wire(bytes)?;
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

fn preflight_lxmf_wire(bytes: &[u8]) -> AppResult<()> {
    if bytes.len() > MAX_LXMF_WIRE_BYTES {
        return Err(AppError::Runtime("LXMF wire exceeds byte limit".into()));
    }
    if bytes.starts_with(LXMF_STORAGE_MAGIC) {
        if bytes.len() < 10 + 32 {
            return Err(AppError::Runtime("LXMF storage wire is truncated".into()));
        }
        let signature_bytes = if bytes[9] & 0x01 != 0 { 64 } else { 0 };
        let payload_start = 10usize.saturating_add(32).saturating_add(signature_bytes);
        let payload = bytes
            .get(payload_start..)
            .ok_or_else(|| AppError::Runtime("LXMF storage payload is truncated".into()))?;
        return validate_lxmf_payload_msgpack(payload);
    }

    if crate::msgpack::validate_msgpack_with_limits(
        bytes,
        MAX_LXMF_WIRE_BYTES,
        MAX_LXMF_WIRE_BYTES,
        32,
        64,
        4,
    )
    .is_ok()
    {
        let mut cursor = std::io::Cursor::new(bytes);
        if let Ok(rmpv::Value::Map(fields)) = rmpv::decode::read_value(&mut cursor) {
            if let Some(inner) = fields.iter().find_map(|(key, value)| {
                matches!(key, rmpv::Value::String(key) if key.as_str() == Some("lxmf_bytes"))
                    .then_some(value)
            }) {
                let rmpv::Value::Binary(inner) = inner else {
                    return Err(AppError::Runtime(
                        "LXMF Python storage payload was not binary".into(),
                    ));
                };
                return preflight_raw_lxmf_wire(inner);
            }
        }
    }
    preflight_raw_lxmf_wire(bytes)
}

fn preflight_raw_lxmf_wire(bytes: &[u8]) -> AppResult<()> {
    let payload = bytes
        .get(LXMF_RAW_WIRE_HEADER_BYTES..)
        .ok_or_else(|| AppError::Runtime("LXMF raw wire is truncated".into()))?;
    validate_lxmf_payload_msgpack(payload)
}

fn validate_lxmf_payload_msgpack(payload: &[u8]) -> AppResult<()> {
    crate::msgpack::validate_msgpack_with_limits(
        payload,
        MAX_LXMF_WIRE_BYTES,
        MAX_LXMF_WIRE_SCALAR_BYTES,
        MAX_LXMF_WIRE_CONTAINER_ITEMS,
        MAX_LXMF_WIRE_TOTAL_VALUES,
        MAX_LXMF_WIRE_DEPTH,
    )
    .map_err(|error| AppError::Runtime(format!("LXMF payload rejected: {error}")))
}

pub(crate) fn attachment_fields_from_paths(
    paths: &[std::path::PathBuf],
) -> AppResult<(Option<rmpv::Value>, Vec<AttachmentSummary>)> {
    if paths.len() > MAX_LXMF_ATTACHMENT_ITEMS {
        return Err(AppError::Runtime(format!(
            "LXMF attachments exceed the {MAX_LXMF_ATTACHMENT_ITEMS} item limit"
        )));
    }
    let mut field_entries = Vec::new();
    let mut summaries = Vec::new();
    let mut total_bytes = 0_u64;

    for path in paths {
        let Some(data) = read_lxmf_attachment(path)? else {
            continue;
        };
        let size = data.len() as u64;
        let name = attachment_file_name(path);
        if name.len() > MAX_LXMF_ATTACHMENT_NAME_BYTES {
            return Err(AppError::Runtime(format!(
                "LXMF attachment name exceeds the {MAX_LXMF_ATTACHMENT_NAME_BYTES} byte limit"
            )));
        }
        total_bytes = total_bytes.checked_add(size).ok_or_else(|| {
            AppError::Runtime("LXMF attachment aggregate byte count overflow".into())
        })?;
        if total_bytes > MAX_LXMF_ATTACHMENT_TOTAL_BYTES {
            return Err(AppError::Runtime(format!(
                "LXMF attachments exceed the {MAX_LXMF_ATTACHMENT_TOTAL_BYTES} byte aggregate limit"
            )));
        }
        field_entries.push(rmpv::Value::Array(vec![
            rmpv::Value::String(name.clone().into()),
            rmpv::Value::Binary(data),
        ]));
        summaries.push(AttachmentSummary {
            name,
            size,
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

fn read_lxmf_attachment(path: &Path) -> AppResult<Option<Vec<u8>>> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError::Runtime(
            "LXMF attachment must be a regular non-symlink file".into(),
        ));
    }
    if metadata.len() > MAX_LXMF_ATTACHMENT_BYTES {
        return Err(AppError::Runtime(format!(
            "LXMF attachment exceeds the {MAX_LXMF_ATTACHMENT_BYTES} byte limit"
        )));
    }

    let mut file = File::open(path)?;
    let opened = file.metadata()?;
    if !opened.is_file() || opened.len() > MAX_LXMF_ATTACHMENT_BYTES {
        return Err(AppError::Runtime(
            "LXMF attachment changed or exceeded its byte limit during admission".into(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.dev() != opened.dev() || metadata.ino() != opened.ino() {
            return Err(AppError::Runtime(
                "LXMF attachment changed during admission".into(),
            ));
        }
    }

    let mut data = Vec::with_capacity(opened.len() as usize);
    Read::by_ref(&mut file)
        .take(MAX_LXMF_ATTACHMENT_BYTES + 1)
        .read_to_end(&mut data)?;
    if data.len() as u64 > MAX_LXMF_ATTACHMENT_BYTES {
        return Err(AppError::Runtime(format!(
            "LXMF attachment exceeds the {MAX_LXMF_ATTACHMENT_BYTES} byte limit"
        )));
    }
    Ok(Some(data))
}

fn insert_lxmf_field(fields: &mut Option<rmpv::Value>, field: i64, value: rmpv::Value) {
    match fields {
        Some(rmpv::Value::Map(entries)) => {
            entries.retain(|(key, _)| !field_key_matches_i64(key, field));
            entries.push((rmpv::Value::Integer(field.into()), value));
        }
        _ => {
            *fields = Some(rmpv::Value::Map(vec![(
                rmpv::Value::Integer(field.into()),
                value,
            )]));
        }
    }
}

fn generate_lxmf_ticket_field() -> rmpv::Value {
    let mut ticket = vec![0u8; LXMF_TICKET_LENGTH];
    rand_core::OsRng.fill_bytes(&mut ticket);
    let expires = current_unix_secs_f64() + LXMF_TICKET_EXPIRY_SECONDS;
    rmpv::Value::Array(vec![rmpv::Value::F64(expires), rmpv::Value::Binary(ticket)])
}

fn current_unix_secs_f64() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or_default()
}

fn attachment_file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("attachment")
        .to_string()
}

fn attachment_summaries_from_fields(
    fields: Option<&rmpv::Value>,
) -> AppResult<Vec<AttachmentSummary>> {
    Ok(attachment_entries_from_fields(fields)?
        .into_iter()
        .map(|entry| AttachmentSummary {
            name: entry.name,
            size: entry.data.len() as u64,
            path: None,
        })
        .collect())
}

fn store_attachments_from_fields(
    fields: Option<&rmpv::Value>,
    attachments_dir: &Path,
    message_id: &str,
) -> AppResult<Vec<AttachmentSummary>> {
    let entries = attachment_entries_from_fields(fields)?;
    if entries.is_empty() {
        return Ok(Vec::new());
    }

    ensure_private_attachment_dir(attachments_dir)?;
    let message_dir = attachments_dir.join(safe_path_component(message_id));
    ensure_private_attachment_dir(&message_dir)?;
    let mut summaries = Vec::with_capacity(entries.len());
    for (index, entry) in entries.into_iter().enumerate() {
        let file_name = format!("{}_{}", index, safe_path_component(&entry.name));
        let stored_path = message_dir.join(file_name);
        write_private_attachment(&stored_path, &entry.data)?;
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

fn attachment_entries_from_fields(fields: Option<&rmpv::Value>) -> AppResult<Vec<AttachmentEntry>> {
    let Some(rmpv::Value::Map(entries)) = fields else {
        return Ok(Vec::new());
    };
    let Some(value) = entries.iter().find_map(|(key, value)| {
        if field_key_matches_file_attachments(key) {
            Some(value)
        } else {
            None
        }
    }) else {
        return Ok(Vec::new());
    };
    let rmpv::Value::Array(items) = value else {
        return Ok(Vec::new());
    };
    if items.len() > MAX_LXMF_ATTACHMENT_ITEMS {
        return Err(AppError::Runtime(format!(
            "LXMF attachments exceed the {MAX_LXMF_ATTACHMENT_ITEMS} item limit"
        )));
    }

    let mut total_bytes = 0_u64;
    let mut retained = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let Some(entry) = attachment_entry_from_value(index, item) else {
            continue;
        };
        if entry.name.len() > MAX_LXMF_ATTACHMENT_NAME_BYTES
            || entry.data.len() as u64 > MAX_LXMF_ATTACHMENT_BYTES
        {
            return Err(AppError::Runtime(
                "LXMF attachment exceeds its name or byte limit".into(),
            ));
        }
        total_bytes = total_bytes
            .checked_add(entry.data.len() as u64)
            .ok_or_else(|| AppError::Runtime("LXMF attachment byte count overflow".into()))?;
        if total_bytes > MAX_LXMF_ATTACHMENT_TOTAL_BYTES {
            return Err(AppError::Runtime(format!(
                "LXMF attachments exceed the {MAX_LXMF_ATTACHMENT_TOTAL_BYTES} byte aggregate limit"
            )));
        }
        retained.push(entry);
    }
    Ok(retained)
}

fn field_key_matches_file_attachments(key: &rmpv::Value) -> bool {
    field_key_matches_i64(key, FIELD_FILE_ATTACHMENTS)
}

fn field_key_matches_i64(key: &rmpv::Value, expected: i64) -> bool {
    field_key_as_i64(key) == Some(expected)
}

fn field_key_as_i64(key: &rmpv::Value) -> Option<i64> {
    match key {
        rmpv::Value::Integer(value) => value
            .as_i64()
            .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok())),
        rmpv::Value::String(value) => value.as_str().and_then(|value| {
            value
                .parse::<i64>()
                .ok()
                .or_else(|| i64::from_str_radix(value.trim_start_matches("0x"), 16).ok())
        }),
        _ => None,
    }
}

fn lxmf_field_name(field_id: i64) -> Option<&'static str> {
    Some(match field_id {
        FIELD_EMBEDDED_LXMS => "embedded_lxms",
        FIELD_TELEMETRY => "telemetry",
        FIELD_TELEMETRY_STREAM => "telemetry_stream",
        FIELD_ICON_APPEARANCE => "icon_appearance",
        FIELD_FILE_ATTACHMENTS => "file_attachments",
        FIELD_IMAGE => "image",
        FIELD_AUDIO => "audio",
        FIELD_THREAD => "thread",
        FIELD_COMMANDS => "commands",
        FIELD_RESULTS => "results",
        FIELD_GROUP => "group",
        FIELD_TICKET => "ticket",
        FIELD_EVENT => "event",
        FIELD_RNR_REFS => "rnr_refs",
        FIELD_RENDERER => "renderer",
        FIELD_CUSTOM_TYPE => "custom_type",
        FIELD_CUSTOM_DATA => "custom_data",
        FIELD_CUSTOM_META => "custom_meta",
        FIELD_NON_SPECIFIC => "non_specific",
        FIELD_DEBUG => "debug",
        _ => return None,
    })
}

fn lxmf_field_value_summary(value: &rmpv::Value) -> String {
    match value {
        rmpv::Value::Nil => "nil".into(),
        rmpv::Value::Boolean(_) => "bool".into(),
        rmpv::Value::Integer(_) => "integer".into(),
        rmpv::Value::F32(_) | rmpv::Value::F64(_) => "float".into(),
        rmpv::Value::String(value) => {
            let len = value.as_str().map(str::len).unwrap_or_default();
            format!("string:{len}")
        }
        rmpv::Value::Binary(bytes) => format!("binary:{}B", bytes.len()),
        rmpv::Value::Array(items) => format!("array:{}", items.len()),
        rmpv::Value::Map(entries) => format!("map:{}", entries.len()),
        rmpv::Value::Ext(_, bytes) => format!("ext:{}B", bytes.len()),
    }
}

fn lxmf_renderer_name(value: &rmpv::Value) -> Option<(u64, &'static str)> {
    let renderer_id = value_as_u64(value)?;
    let name = match renderer_id {
        RENDERER_PLAIN => "plain",
        RENDERER_MICRON => "micron",
        RENDERER_MARKDOWN => "markdown",
        RENDERER_BBCODE => "bbcode",
        _ => "unknown",
    };
    Some((renderer_id, name))
}

fn annotate_lxmf_thread_field(fields: &mut BTreeMap<String, String>, value: &rmpv::Value) {
    if let Some(thread_id) = lxmf_thread_scalar(value) {
        fields.insert("native_lxmf_thread_id".into(), thread_id);
        return;
    }

    match value {
        rmpv::Value::Array(items) => {
            if let Some(thread_id) = items.first().and_then(lxmf_thread_scalar) {
                fields.insert("native_lxmf_thread_id".into(), thread_id);
            }
            if let Some(parent_id) = items.get(1).and_then(lxmf_thread_scalar) {
                fields.insert("native_lxmf_thread_parent_id".into(), parent_id);
            }
        }
        rmpv::Value::Map(entries) => {
            for (key, value) in entries {
                let Some(key) = lxmf_thread_map_key(key) else {
                    continue;
                };
                let Some(value) = lxmf_thread_scalar(value) else {
                    continue;
                };
                match key.as_str() {
                    "id" | "thread" | "thread_id" => {
                        fields.insert("native_lxmf_thread_id".into(), value);
                    }
                    "parent" | "parent_id" | "reply_to" | "reply" => {
                        fields.insert("native_lxmf_thread_parent_id".into(), value);
                    }
                    "root" | "root_id" => {
                        fields.insert("native_lxmf_thread_root_id".into(), value);
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

fn lxmf_thread_scalar(value: &rmpv::Value) -> Option<String> {
    match value {
        rmpv::Value::String(value) => value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(compact_lxmf_thread_value),
        rmpv::Value::Binary(bytes) => {
            if bytes.is_empty() {
                None
            } else if let Ok(value) = String::from_utf8(bytes.clone()) {
                let value = value.trim();
                if value.is_empty() {
                    None
                } else {
                    Some(compact_lxmf_thread_value(value))
                }
            } else {
                Some(hex_bytes(bytes))
            }
        }
        rmpv::Value::Integer(value) => value
            .as_i64()
            .map(|value| value.to_string())
            .or_else(|| value.as_u64().map(|value| value.to_string())),
        _ => None,
    }
}

fn lxmf_thread_map_key(value: &rmpv::Value) -> Option<String> {
    match value {
        rmpv::Value::String(value) => value.as_str().map(|value| value.to_ascii_lowercase()),
        rmpv::Value::Binary(bytes) => String::from_utf8(bytes.clone())
            .ok()
            .map(|value| value.to_ascii_lowercase()),
        _ => None,
    }
}

fn compact_lxmf_thread_value(value: &str) -> String {
    const MAX_THREAD_VALUE_CHARS: usize = 96;
    if value.chars().count() <= MAX_THREAD_VALUE_CHARS {
        return value.to_string();
    }
    value.chars().take(MAX_THREAD_VALUE_CHARS).collect()
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
    let sanitized = if sanitized.is_empty() {
        "attachment".into()
    } else {
        sanitized.to_string()
    };
    if sanitized.len() <= MAX_LXMF_ATTACHMENT_PATH_COMPONENT_BYTES {
        return sanitized;
    }
    let digest = Sha256::digest(value.as_bytes());
    let suffix = hex_bytes(&digest[..8]);
    let prefix_bytes = MAX_LXMF_ATTACHMENT_PATH_COMPONENT_BYTES - suffix.len() - 1;
    format!("{}-{suffix}", &sanitized[..prefix_bytes])
}

fn ensure_private_attachment_dir(path: &Path) -> AppResult<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(AppError::Runtime(
                    "LXMF attachment storage path must be a real directory".into(),
                ));
            }
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            let mut builder = std::fs::DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(path)?;
        }
        Err(error) => return Err(error.into()),
    }
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::Runtime(
            "LXMF attachment storage path must be a real directory".into(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn write_private_attachment(path: &Path, bytes: &[u8]) -> AppResult<()> {
    write_private_attachment_with(path, bytes, || Ok(()))
}

fn write_private_attachment_with(
    path: &Path,
    bytes: &[u8],
    before_commit: impl FnOnce() -> std::io::Result<()>,
) -> AppResult<()> {
    if bytes.len() as u64 > MAX_LXMF_ATTACHMENT_BYTES {
        return Err(AppError::Runtime(format!(
            "LXMF attachment exceeds the {MAX_LXMF_ATTACHMENT_BYTES} byte limit"
        )));
    }
    let parent = path.parent().ok_or_else(|| {
        AppError::Runtime("LXMF attachment destination has no parent directory".into())
    })?;
    ensure_private_attachment_dir(parent)?;
    validate_attachment_destination(path)?;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::Runtime("LXMF attachment destination is invalid".into()))?;
    let sequence = LXMF_ATTACHMENT_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        sequence
    ));
    let result = (|| -> std::io::Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        before_commit()?;
        validate_attachment_destination_io(path)?;
        atomic_replace(&temporary, path)?;
        #[cfg(unix)]
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(Into::into)
}

fn validate_attachment_destination(path: &Path) -> AppResult<()> {
    validate_attachment_destination_io(path).map_err(|error| {
        AppError::Runtime(format!("LXMF attachment destination rejected: {error}"))
    })
}

fn validate_attachment_destination_io(path: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(std::io::Error::other("must be a regular non-symlink file"))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
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

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::path::{Path, PathBuf};
    use std::process::{Child, ChildStdout, Command, Stdio};
    use std::sync::{mpsc, Arc};
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use omen_ifac_tcp::IfacTcpClient;
    use rns_transport::delivery::{await_link_activation, send_on_link_observed, LinkSendResult};
    use rns_transport::hash::AddressHash;
    use rns_transport::identity::PrivateIdentity as TransportPrivateIdentity;
    use rns_transport::transport::{DeliveryReceipt, ReceiptHandler, Transport, TransportConfig};

    use super::*;
    use crate::identity::IdentityMaterialProvider;
    use crate::messaging::DeliveryMode;
    use crate::runtime::native::identity::NativeReticulumIdentityProvider;

    const DEST: &str = "00112233445566778899aabbccddeeff";
    const SRC: &str = "ffeeddccbbaa99887766554433221100";

    struct CurrentLxmfReceiptCapture {
        sender: tokio::sync::mpsc::Sender<[u8; 32]>,
    }

    impl ReceiptHandler for CurrentLxmfReceiptCapture {
        fn on_receipt(&self, receipt: &DeliveryReceipt) {
            let _ = self.sender.try_send(receipt.message_id);
        }
    }

    struct CurrentLxmfRoot(PathBuf);

    impl CurrentLxmfRoot {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "omen-current-python-lxmf-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("create isolated current LXMF root");
            Self(path)
        }
    }

    impl Drop for CurrentLxmfRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    struct CurrentPythonLxmfPeer {
        child: Child,
        json_lines: mpsc::Receiver<serde_json::Value>,
        reader: Option<JoinHandle<()>>,
        ready: serde_json::Value,
    }

    impl CurrentPythonLxmfPeer {
        fn spawn(root: &Path, port: u16, source: &str) -> Self {
            let source_path = std::env::var_os("OMEN_PYTHON_RNS_SOURCE")
                .map(PathBuf::from)
                .expect("OMEN_PYTHON_RNS_SOURCE must name current Python site-packages");
            let script = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src/server/crates/omen-ifac-tcp/tests/fixtures/current_python_lxmf_peer.py");
            let mut child = Command::new("python3")
                .arg(script)
                .arg("--rns-source")
                .arg(source_path)
                .arg("--root")
                .arg(root)
                .arg("--port")
                .arg(port.to_string())
                .arg("--source")
                .arg(source)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("spawn current Python LXMF peer");
            let (json_lines, reader) =
                current_lxmf_json_reader(child.stdout.take().expect("current Python peer stdout"));
            let ready = current_lxmf_json_line(
                &json_lines,
                Duration::from_secs(8),
                "current Python LXMF readiness",
            );
            assert_eq!(ready["ready"], true);
            assert_eq!(ready["port"], port);
            assert_eq!(ready["rns"], "1.4.2");
            assert_eq!(ready["lxmf"], "1.1.1");
            Self {
                child,
                json_lines,
                reader: Some(reader),
                ready,
            }
        }

        fn wait_for_source_announce(&self) {
            let announced = current_lxmf_json_line(
                &self.json_lines,
                Duration::from_secs(10),
                "current Python LXMF source announce",
            );
            assert_eq!(announced["source_announced"], true);
        }

        fn finish(mut self) -> serde_json::Value {
            let result = current_lxmf_json_line(
                &self.json_lines,
                Duration::from_secs(22),
                "current Python LXMF delivery result",
            );
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if let Some(status) = self.child.try_wait().expect("poll current Python peer") {
                    assert!(status.success(), "current Python LXMF peer exited {status}");
                    self.join_reader();
                    return result;
                }
                if Instant::now() >= deadline {
                    let _ = self.child.kill();
                    panic!("current Python LXMF peer did not exit within bounded shutdown");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        fn join_reader(&mut self) {
            if let Some(reader) = self.reader.take() {
                reader.join().expect("current Python stdout reader join");
            }
        }
    }

    impl Drop for CurrentPythonLxmfPeer {
        fn drop(&mut self) {
            if self.child.try_wait().ok().flatten().is_none() {
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
            self.join_reader();
        }
    }

    struct CurrentPythonLxmfSender {
        child: Child,
        json_lines: mpsc::Receiver<serde_json::Value>,
        reader: Option<JoinHandle<()>>,
        ready: serde_json::Value,
    }

    impl CurrentPythonLxmfSender {
        fn spawn(root: &Path, port: u16, destination: &str) -> Self {
            let source_path = std::env::var_os("OMEN_PYTHON_RNS_SOURCE")
                .map(PathBuf::from)
                .expect("OMEN_PYTHON_RNS_SOURCE must name current Python site-packages");
            let script = Path::new(env!("CARGO_MANIFEST_DIR")).join(
                "src/server/crates/omen-ifac-tcp/tests/fixtures/current_python_lxmf_sender.py",
            );
            let mut child = Command::new("python3")
                .arg(script)
                .arg("--rns-source")
                .arg(source_path)
                .arg("--root")
                .arg(root)
                .arg("--port")
                .arg(port.to_string())
                .arg("--destination")
                .arg(destination)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("spawn current Python LXMF sender");
            let (json_lines, reader) = current_lxmf_json_reader(
                child.stdout.take().expect("current Python sender stdout"),
            );
            let ready = current_lxmf_json_line(
                &json_lines,
                Duration::from_secs(8),
                "current Python LXMF sender readiness",
            );
            assert_eq!(ready["ready"], true);
            assert_eq!(ready["port"], port);
            assert_eq!(ready["rns"], "1.4.2");
            assert_eq!(ready["lxmf"], "1.1.1");
            Self {
                child,
                json_lines,
                reader: Some(reader),
                ready,
            }
        }

        fn finish(mut self) -> serde_json::Value {
            let result = current_lxmf_json_line(
                &self.json_lines,
                Duration::from_secs(22),
                "current Python LXMF sender result",
            );
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if let Some(status) = self.child.try_wait().expect("poll current Python sender") {
                    assert!(
                        status.success(),
                        "current Python LXMF sender exited {status}"
                    );
                    self.join_reader();
                    return result;
                }
                if Instant::now() >= deadline {
                    let _ = self.child.kill();
                    panic!("current Python LXMF sender did not exit within bounded shutdown");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        fn join_reader(&mut self) {
            if let Some(reader) = self.reader.take() {
                reader
                    .join()
                    .expect("current Python sender stdout reader join");
            }
        }
    }

    impl Drop for CurrentPythonLxmfSender {
        fn drop(&mut self) {
            if self.child.try_wait().ok().flatten().is_none() {
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
            self.join_reader();
        }
    }

    struct PythonLxmfTicketRoundtripPeer {
        child: Child,
        json_lines: mpsc::Receiver<serde_json::Value>,
        reader: Option<JoinHandle<()>>,
        ready: serde_json::Value,
    }

    impl PythonLxmfTicketRoundtripPeer {
        fn spawn(
            root: &Path,
            port: u16,
            rust_source: &str,
            rns_source_env: &str,
            lxmf_source_env: Option<&str>,
            expected_rns: &str,
            expected_lxmf: &str,
        ) -> Self {
            let rns_source = std::env::var_os(rns_source_env)
                .map(PathBuf::from)
                .unwrap_or_else(|| panic!("{rns_source_env} must name a Python RNS source"));
            let script = Path::new(env!("CARGO_MANIFEST_DIR")).join(
                "src/server/crates/omen-ifac-tcp/tests/fixtures/python_lxmf_ticket_roundtrip_peer.py",
            );
            let mut command = Command::new("python3");
            command.arg(script).arg("--rns-source").arg(rns_source);
            if let Some(lxmf_source_env) = lxmf_source_env {
                let lxmf_source = std::env::var_os(lxmf_source_env)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| panic!("{lxmf_source_env} must name a Python LXMF source"));
                command.arg("--lxmf-source").arg(lxmf_source);
            }
            let mut child = command
                .arg("--expected-rns")
                .arg(expected_rns)
                .arg("--expected-lxmf")
                .arg(expected_lxmf)
                .arg("--root")
                .arg(root)
                .arg("--port")
                .arg(port.to_string())
                .arg("--rust-source")
                .arg(rust_source)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("spawn Python LXMF ticket round-trip peer");
            let (json_lines, reader) = current_lxmf_json_reader(
                child
                    .stdout
                    .take()
                    .expect("Python ticket round-trip stdout"),
            );
            let ready = current_lxmf_json_line(
                &json_lines,
                Duration::from_secs(8),
                "Python LXMF ticket round-trip readiness",
            );
            assert_eq!(ready["ready"], true);
            assert_eq!(ready["port"], port);
            assert_eq!(ready["rns"], expected_rns);
            assert_eq!(ready["lxmf"], expected_lxmf);
            Self {
                child,
                json_lines,
                reader: Some(reader),
                ready,
            }
        }

        fn wait_for_source_announce(&self) {
            let announced = current_lxmf_json_line(
                &self.json_lines,
                Duration::from_secs(12),
                "Python LXMF ticket source announce",
            );
            assert_eq!(announced["source_announced"], true);
        }

        fn finish(mut self) -> serde_json::Value {
            let result = current_lxmf_json_line(
                &self.json_lines,
                Duration::from_secs(28),
                "Python LXMF ticket round-trip result",
            );
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if let Some(status) = self.child.try_wait().expect("poll Python ticket peer") {
                    assert!(status.success(), "Python LXMF ticket peer exited {status}");
                    self.join_reader();
                    return result;
                }
                if Instant::now() >= deadline {
                    let _ = self.child.kill();
                    panic!("Python LXMF ticket peer did not exit within bounded shutdown");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        fn join_reader(&mut self) {
            if let Some(reader) = self.reader.take() {
                reader.join().expect("Python ticket stdout reader join");
            }
        }
    }

    impl Drop for PythonLxmfTicketRoundtripPeer {
        fn drop(&mut self) {
            if self.child.try_wait().ok().flatten().is_none() {
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
            self.join_reader();
        }
    }

    struct PythonLxmfDirectStampPeer {
        child: Child,
        json_lines: mpsc::Receiver<serde_json::Value>,
        reader: Option<JoinHandle<()>>,
        ready: serde_json::Value,
    }

    impl PythonLxmfDirectStampPeer {
        fn spawn(
            root: &Path,
            port: u16,
            rust_source: &str,
            rns_source_env: &str,
            lxmf_source_env: Option<&str>,
            expected_rns: &str,
            expected_lxmf: &str,
        ) -> Self {
            let rns_source = std::env::var_os(rns_source_env)
                .map(PathBuf::from)
                .unwrap_or_else(|| panic!("{rns_source_env} must name a Python RNS source"));
            let script = Path::new(env!("CARGO_MANIFEST_DIR")).join(
                "src/server/crates/omen-ifac-tcp/tests/fixtures/python_lxmf_direct_stamp_peer.py",
            );
            let mut command = Command::new("python3");
            command.arg(script).arg("--rns-source").arg(rns_source);
            if let Some(lxmf_source_env) = lxmf_source_env {
                let lxmf_source = std::env::var_os(lxmf_source_env)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| panic!("{lxmf_source_env} must name a Python LXMF source"));
                command.arg("--lxmf-source").arg(lxmf_source);
            }
            let mut child = command
                .arg("--expected-rns")
                .arg(expected_rns)
                .arg("--expected-lxmf")
                .arg(expected_lxmf)
                .arg("--root")
                .arg(root)
                .arg("--port")
                .arg(port.to_string())
                .arg("--rust-source")
                .arg(rust_source)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("spawn Python LXMF direct-stamp peer");
            let (json_lines, reader) =
                current_lxmf_json_reader(child.stdout.take().expect("Python direct-stamp stdout"));
            let ready = current_lxmf_json_line(
                &json_lines,
                Duration::from_secs(8),
                "Python LXMF direct-stamp readiness",
            );
            assert_eq!(ready["ready"], true);
            assert_eq!(ready["port"], port);
            assert_eq!(ready["rns"], expected_rns);
            assert_eq!(ready["lxmf"], expected_lxmf);
            Self {
                child,
                json_lines,
                reader: Some(reader),
                ready,
            }
        }

        fn wait_for_source_announce(&self) {
            let announced = current_lxmf_json_line(
                &self.json_lines,
                Duration::from_secs(12),
                "Python LXMF direct-stamp source announce",
            );
            assert_eq!(announced["source_announced"], true);
        }

        fn finish(mut self) -> serde_json::Value {
            let result = current_lxmf_json_line(
                &self.json_lines,
                Duration::from_secs(28),
                "Python LXMF direct-stamp result",
            );
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if let Some(status) = self.child.try_wait().expect("poll Python stamp peer") {
                    assert!(status.success(), "Python LXMF stamp peer exited {status}");
                    self.join_reader();
                    return result;
                }
                if Instant::now() >= deadline {
                    let _ = self.child.kill();
                    panic!("Python LXMF stamp peer did not exit within bounded shutdown");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        fn join_reader(&mut self) {
            if let Some(reader) = self.reader.take() {
                reader.join().expect("Python stamp stdout reader join");
            }
        }
    }

    impl Drop for PythonLxmfDirectStampPeer {
        fn drop(&mut self) {
            if self.child.try_wait().ok().flatten().is_none() {
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
            self.join_reader();
        }
    }

    fn current_lxmf_json_reader(
        stdout: ChildStdout,
    ) -> (mpsc::Receiver<serde_json::Value>, JoinHandle<()>) {
        let (sender, receiver) = mpsc::sync_channel(4);
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                    if sender.send(value).is_err() {
                        break;
                    }
                }
            }
        });
        (receiver, reader)
    }

    fn current_lxmf_json_line(
        lines: &mpsc::Receiver<serde_json::Value>,
        timeout: Duration,
        description: &str,
    ) -> serde_json::Value {
        lines
            .recv_timeout(timeout)
            .unwrap_or_else(|error| panic!("{description}: {error}"))
    }

    fn current_lxmf_port() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").expect("reserve LXMF loopback port");
        listener.local_addr().expect("reserved address").port()
    }

    fn lxmf_delivery_hash(identity: &PrivateIdentity) -> [u8; 16] {
        lxmf_delivery_destination_hash_from_private_identity_bytes(
            identity.to_private_key_bytes().as_slice(),
        )
        .expect("lxmf delivery hash")
    }

    fn hex16_bytes(bytes: &[u8; 16]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

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
        assert!(parity.include_ticket_supported);
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
    fn lxmf_wire_preflight_rejects_unbounded_raw_and_storage_payloads() {
        let oversized_payload = [0xc6, 0x00, 0x80, 0x00, 0x01];
        let mut raw = vec![0; LXMF_RAW_WIRE_HEADER_BYTES];
        raw.extend(oversized_payload);
        assert!(preflight_lxmf_wire(&raw).is_err());

        let mut fixed_storage = LXMF_STORAGE_MAGIC.to_vec();
        fixed_storage.extend([1, 0]);
        fixed_storage.extend([0; 32]);
        fixed_storage.extend(oversized_payload);
        assert!(preflight_lxmf_wire(&fixed_storage).is_err());

        let mut python_storage = Vec::new();
        rmpv::encode::write_value(
            &mut python_storage,
            &rmpv::Value::Map(vec![(
                rmpv::Value::String("lxmf_bytes".into()),
                rmpv::Value::Binary(raw),
            )]),
        )
        .expect("encode Python storage container");
        assert!(preflight_lxmf_wire(&python_storage).is_err());

        let mut deep = vec![0x91; MAX_LXMF_WIRE_DEPTH + 2];
        deep.push(0xc0);
        let mut raw_deep = vec![0; LXMF_RAW_WIRE_HEADER_BYTES];
        raw_deep.extend(deep);
        assert!(preflight_lxmf_wire(&raw_deep).is_err());
    }

    #[test]
    fn delivery_announce_app_data_extracts_display_name() {
        let encoded = encode_delivery_display_name_app_data("Alice Relay")
            .expect("encoded delivery app data");

        let display_name =
            delivery_display_name_from_app_data(encoded.as_slice()).expect("display name");

        assert_eq!(display_name, "Alice Relay");
        assert_eq!(delivery_announce_stamp_cost(encoded.as_slice()), None);
    }

    #[test]
    fn delivery_announce_app_data_extracts_stamp_cost() {
        let value = rmpv::Value::Array(vec![
            rmpv::Value::Binary(b"Alice Relay".to_vec()),
            rmpv::Value::from(8_u64),
        ]);
        let mut encoded = Vec::new();
        rmpv::encode::write_value(&mut encoded, &value).expect("encode delivery announce");

        assert_eq!(
            delivery_display_name_from_app_data(encoded.as_slice()).as_deref(),
            Some("Alice Relay")
        );
        assert_eq!(delivery_announce_stamp_cost(encoded.as_slice()), Some(8));
    }

    #[test]
    fn delivery_stamp_policy_distinguishes_unknown_required_ticket_and_unsupported() {
        fn encode(values: Vec<rmpv::Value>) -> Vec<u8> {
            let mut encoded = Vec::new();
            rmpv::encode::write_value(&mut encoded, &rmpv::Value::Array(values))
                .expect("encode delivery announce");
            encoded
        }

        let required = encode(vec![
            rmpv::Value::Binary(b"Peer".to_vec()),
            rmpv::Value::from(8_u64),
        ]);
        let not_required = encode(vec![
            rmpv::Value::Binary(b"Peer".to_vec()),
            rmpv::Value::Nil,
        ]);
        let unsupported_zero = encode(vec![
            rmpv::Value::Binary(b"Peer".to_vec()),
            rmpv::Value::from(0_u64),
        ]);
        let unsupported_high = encode(vec![
            rmpv::Value::Binary(b"Peer".to_vec()),
            rmpv::Value::from(255_u64),
        ]);
        let unsupported_type = encode(vec![
            rmpv::Value::Binary(b"Peer".to_vec()),
            rmpv::Value::String("expensive".into()),
        ]);

        assert_eq!(
            delivery_announce_direct_stamp_policy(Some(&required), false),
            DirectStampPolicy::Required { cost: 8 }
        );
        assert_eq!(
            delivery_announce_direct_stamp_policy(Some(&not_required), false),
            DirectStampPolicy::NotRequired
        );
        assert_eq!(
            delivery_announce_direct_stamp_policy(Some(&required), true),
            DirectStampPolicy::TicketAccepted
        );
        assert_eq!(
            delivery_announce_direct_stamp_policy(None, false),
            DirectStampPolicy::Unknown
        );
        assert_eq!(
            delivery_announce_direct_stamp_policy(Some(b"legacy peer"), false),
            DirectStampPolicy::Unknown
        );
        assert_eq!(
            delivery_announce_direct_stamp_policy(Some(&unsupported_zero), false),
            DirectStampPolicy::Unsupported
        );
        assert_eq!(
            delivery_announce_direct_stamp_policy(Some(&unsupported_high), false),
            DirectStampPolicy::Unsupported
        );
        assert_eq!(
            delivery_announce_direct_stamp_policy(Some(&unsupported_type), false),
            DirectStampPolicy::Unsupported
        );
    }

    #[test]
    fn delivery_stamp_cost_parser_matches_upstream_09_for_admitted_costs() {
        for cost in [1_u64, 8, 16, 254] {
            let value = rmpv::Value::Array(vec![
                rmpv::Value::Binary(b"Peer".to_vec()),
                rmpv::Value::from(cost),
            ]);
            let mut encoded = Vec::new();
            rmpv::encode::write_value(&mut encoded, &value).expect("encode delivery announce");

            assert_eq!(
                delivery_announce_stamp_cost(&encoded).map(i64::from),
                lxmf::wire::announce::stamp_cost_from_app_data(Some(&encoded))
            );
        }
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
    fn lxmf_announce_parsers_reject_unbounded_or_trailing_msgpack() {
        let mut valid_delivery = Vec::new();
        rmpv::encode::write_value(
            &mut valid_delivery,
            &rmpv::Value::Array(vec![
                rmpv::Value::Binary(b"Relay".to_vec()),
                rmpv::Value::from(8_u64),
            ]),
        )
        .expect("encode delivery announce");
        valid_delivery.push(0xc0);
        assert_eq!(delivery_display_name_from_app_data(&valid_delivery), None);
        assert_eq!(delivery_announce_stamp_cost(&valid_delivery), None);

        let oversized_scalar = [0xdb, 0x00, 0x00, 0x10, 0x01];
        assert_eq!(delivery_announce_stamp_cost(&oversized_scalar), None);
        assert!(!propagation_announce_data_is_valid(&oversized_scalar));

        let mut deep = vec![0x91; MAX_LXMF_ANNOUNCE_DEPTH + 2];
        deep.push(0xc0);
        assert!(!propagation_announce_data_is_valid(&deep));
        assert!(!propagation_announce_data_is_valid(&vec![
            0xc0;
            MAX_LXMF_ANNOUNCE_BYTES
                + 1
        ]));
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
    fn propagation_stamp_generation_validates_without_legacy_rns_net_stack() {
        let mut lxm_data = vec![0x33; 180];
        lxm_data[0] = 0x92;
        let digest = Sha256::digest(&lxm_data);
        let mut transient_id = [0u8; 32];
        transient_id.copy_from_slice(&digest);

        let stamp = generate_propagation_stamp_for_transient(&lxm_data, transient_id, 1, 512)
            .expect("propagation stamp");
        let mut transient_data = lxm_data.clone();
        transient_data.extend_from_slice(&stamp.stamp);
        let validated = validate_propagation_stamp_any_cost(&transient_data, &[1])
            .expect("generated propagation stamp validates");

        assert_eq!(validated.lxm_data, lxm_data);
        assert_eq!(validated.transient_id, transient_id);
        assert_eq!(validated.stamp_value, stamp.stamp_value);
    }

    #[test]
    fn direct_stamp_generation_validates_without_legacy_rns_net_stack() {
        let message_id = [0x55; 32];

        let stamp = generate_direct_stamp_for_message(message_id, 1, 512).expect("direct stamp");

        assert_eq!(
            validate_direct_stamp(&message_id, &stamp.stamp, stamp.target_cost),
            Some(stamp.stamp_value)
        );
    }

    #[test]
    fn direct_stamp_generation_observes_cooperative_cancellation() {
        let mut checks = 0_u8;
        let error = generate_direct_stamp_for_message_cancellable(
            [0x44; 32],
            254,
            CLEAN_DIRECT_STAMP_MAX_ATTEMPTS,
            || {
                checks = checks.saturating_add(1);
                checks > 3
            },
        )
        .expect_err("cancelled stamp work");

        assert!(error.to_string().contains("cancelled after"));
        assert_eq!(checks, 4);
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
        let receiver_lxmf_hash = lxmf_delivery_hash(&receiver);
        let sender_lxmf_hash = lxmf_delivery_hash(&sender);
        let mut receiver_public = [0u8; 64];
        receiver_public[..32].copy_from_slice(receiver_identity.public_key_bytes());
        receiver_public[32..].copy_from_slice(receiver_identity.verifying_key_bytes());
        let envelope = MessageEnvelope {
            peer_hash: hex16_bytes(&receiver_lxmf_hash),
            title: "Propagated".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Propagated,
            include_ticket: false,
            native_reply_ticket: None,
            operation: None,
            attachments: Vec::new(),
        };
        let outbound =
            build_outbound_message(&envelope, &hex16_bytes(&sender_lxmf_hash)).expect("outbound");

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
        assert_eq!(summary.peer_hash, hex16_bytes(&sender_lxmf_hash));
        assert_eq!(summary.title, "Propagated");
        assert_eq!(summary.content, "Body");
    }

    #[test]
    fn direct_stamp_generation_applies_valid_lxmf_message_stamp() {
        let envelope = MessageEnvelope {
            peer_hash: DEST.into(),
            title: "Subject".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: None,
            operation: None,
            attachments: Vec::new(),
        };
        let mut outbound = build_outbound_message(&envelope, SRC).expect("outbound");

        let stamp = apply_direct_stamp_if_needed(&mut outbound, Some(1), 512)
            .expect("direct stamp")
            .expect("stamp generated");

        assert_eq!(stamp.target_cost, 1);
        assert!(stamp.attempts <= 512);
        assert_eq!(
            validate_direct_stamp(&stamp.message_id, &stamp.stamp, stamp.target_cost),
            Some(stamp.stamp_value)
        );
        assert_eq!(
            outbound.message.stamp.as_deref(),
            Some(stamp.stamp.as_slice())
        );
    }

    #[test]
    fn direct_stamp_generation_keeps_valid_reply_ticket_stamp() {
        let ticket = NativeLxmfReplyTicket {
            ticket: vec![0x42; 16],
            expires: current_unix_secs_f64() + 60.0,
        };
        let envelope = MessageEnvelope {
            peer_hash: DEST.into(),
            title: "Subject".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: Some(ticket),
            operation: None,
            attachments: Vec::new(),
        };
        let mut outbound = build_outbound_message(&envelope, SRC).expect("outbound");
        let ticket_stamp = outbound.message.stamp.clone().expect("ticket stamp");

        let generated = apply_direct_stamp_if_needed(&mut outbound, Some(1), 512)
            .expect("direct stamp skipped");

        assert!(generated.is_none());
        assert!(outbound.reply_ticket_used);
        assert_eq!(outbound.message.stamp.as_ref(), Some(&ticket_stamp));
    }

    #[test]
    fn outbound_envelope_maps_to_lxmf_wire_message() {
        let envelope = MessageEnvelope {
            peer_hash: DEST.into(),
            title: "Subject".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: None,
            operation: None,
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
            native_reply_ticket: None,
            operation: None,
            attachments: Vec::new(),
        };

        let outbound = build_outbound_message(&envelope, SRC).expect("outbound");

        assert_eq!(outbound.delivery.method, lxmf::TransportMethod::Propagated);
        assert!(!outbound.include_ticket);
    }

    #[test]
    fn include_ticket_send_adds_lxmf_ticket_field() {
        let envelope = MessageEnvelope {
            peer_hash: DEST.into(),
            title: "Subject".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: true,
            native_reply_ticket: None,
            operation: None,
            attachments: Vec::new(),
        };

        let outbound = build_outbound_message(&envelope, SRC).expect("ticketed outbound");
        let (expires, ticket) =
            ticket_entry_from_fields(outbound.message.fields.as_ref()).expect("ticket field");

        assert!(outbound.include_ticket);
        assert_eq!(ticket.len(), LXMF_TICKET_LENGTH);
        assert!(expires > 0.0);
    }

    #[test]
    fn signed_ticketed_wire_message_decodes_reply_ticket_metadata() {
        let provider = NativeReticulumIdentityProvider;
        let private = provider
            .create_identity_material("sender")
            .expect("native identity");
        let signer = PrivateIdentity::from_private_key_bytes(&private).expect("signer");
        let source = hex16_bytes(&lxmf_delivery_destination_hash(signer.as_identity()));
        let envelope = MessageEnvelope {
            peer_hash: DEST.into(),
            title: "Ticketed".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: true,
            native_reply_ticket: None,
            operation: None,
            attachments: Vec::new(),
        };
        let outbound = build_outbound_message(&envelope, &source).expect("outbound");
        let (_, ticket) =
            ticket_entry_from_fields(outbound.message.fields.as_ref()).expect("ticket field");

        let wire = encode_signed_wire_message(&outbound, &private).expect("encode");
        let summary = decode_wire_message(&wire).expect("decode");
        let expected_ticket = hex_bytes(&ticket);

        assert_eq!(
            summary
                .fields
                .get("native_lxmf_reply_ticket")
                .map(String::as_str),
            Some(expected_ticket.as_str())
        );
        assert_eq!(
            summary
                .fields
                .get("native_lxmf_reply_ticket_state")
                .map(String::as_str),
            Some("valid")
        );
        assert!(summary
            .fields
            .contains_key("native_lxmf_reply_ticket_expires"));
    }

    #[test]
    fn clean_include_ticket_send_adds_lxmf_ticket_field() {
        let envelope = MessageEnvelope {
            peer_hash: DEST.into(),
            title: "Subject".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: true,
            native_reply_ticket: None,
            operation: None,
            attachments: Vec::new(),
        };

        let outbound = build_outbound_message(&envelope, SRC).expect("ticketed outbound");
        let (expires, ticket) =
            ticket_entry_from_fields(outbound.message.fields.as_ref()).expect("ticket field");

        assert!(outbound.include_ticket);
        assert_eq!(ticket.len(), LXMF_TICKET_LENGTH);
        assert!(expires > current_unix_secs_f64());
    }

    #[test]
    fn ticket_stamp_uses_ticket_and_message_id_truncated_hash() {
        let ticket = [0x11u8; LXMF_TICKET_LENGTH];
        let message_id = [0x22u8; 32];

        let stamp = ticket_stamp_for_message(&ticket, &message_id).expect("ticket stamp");
        let repeat = ticket_stamp_for_message(&ticket, &message_id).expect("repeat stamp");

        assert_eq!(stamp.len(), 16);
        assert_eq!(stamp, repeat);
    }

    #[test]
    fn outbound_reply_ticket_sets_lxmf_ticket_stamp() {
        let ticket = NativeLxmfReplyTicket {
            ticket: vec![0x42; LXMF_TICKET_LENGTH],
            expires: current_unix_secs_f64() + 60.0,
        };
        let envelope = MessageEnvelope {
            peer_hash: DEST.into(),
            title: "Subject".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: Some(ticket.clone()),
            operation: None,
            attachments: Vec::new(),
        };

        let outbound = build_outbound_message(&envelope, SRC).expect("outbound");
        let payload = lxmf::Payload::new(
            outbound.message.timestamp.expect("timestamp"),
            Some(outbound.message.content.clone()),
            Some(outbound.message.title.clone()),
            outbound.message.fields.clone(),
            None,
        );
        let wire = lxmf::WireMessage::new(
            outbound.message.destination_hash.expect("destination"),
            outbound.message.source_hash.expect("source"),
            payload,
        );
        let expected =
            ticket_stamp_for_message(&ticket.ticket, &wire.message_id()).expect("expected stamp");

        assert!(outbound.reply_ticket_used);
        assert_eq!(outbound.message.stamp.as_deref(), Some(expected.as_slice()));
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
            native_reply_ticket: None,
            operation: None,
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
        assert!(!summary.fields.contains_key(LXMF_SOURCE_AUTHENTICATED_FIELD));
    }

    #[test]
    fn verified_signed_wire_message_requires_matching_source_identity() {
        let provider = NativeReticulumIdentityProvider;
        let private = provider
            .create_identity_material("verified-sender")
            .expect("native identity");
        let signer = PrivateIdentity::from_private_key_bytes(&private).expect("signer");
        let source = hex16_bytes(&lxmf_delivery_destination_hash(signer.as_identity()));
        let envelope = MessageEnvelope {
            peer_hash: DEST.into(),
            title: "Verified".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: None,
            operation: None,
            attachments: Vec::new(),
        };
        let outbound = build_outbound_message(&envelope, &source).expect("outbound");
        let wire = encode_signed_wire_message(&outbound, &private).expect("encode");

        let summary = decode_verified_wire_message(&wire, signer.as_identity())
            .expect("matching identity verifies");

        assert_eq!(summary.peer_hash, source);
        assert_eq!(summary.title, "Verified");
        assert_eq!(
            summary
                .fields
                .get(LXMF_SOURCE_AUTHENTICATED_FIELD)
                .map(String::as_str),
            Some("true")
        );
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn signed_native_invitation_wire_enters_preview_without_history_or_action() {
        let provider = NativeReticulumIdentityProvider;
        let private = provider
            .create_identity_material("invitation-sender")
            .expect("native identity");
        let signer = PrivateIdentity::from_private_key_bytes(&private).expect("signer");
        let source = hex16_bytes(&lxmf_delivery_destination_hash(signer.as_identity()));
        let payload = crate::chat::handoff::OmenChatInvitePayload::new(
            DEST,
            "lobby",
            "Lobby",
            "Verified inviter",
            &source,
        );
        let envelope = MessageEnvelope {
            peer_hash: DEST.into(),
            title: crate::chat::handoff::OMENCHAT_INVITE_PROTOCOL.into(),
            body: String::from_utf8(payload.encode().expect("encode invitation"))
                .expect("JSON UTF-8"),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: None,
            operation: None,
            attachments: Vec::new(),
        };
        let outbound = build_outbound_message(&envelope, &source).expect("outbound invitation");
        let wire = encode_signed_wire_message(&outbound, &private).expect("signed invitation");
        let summary = decode_verified_wire_message(&wire, signer.as_identity())
            .expect("production invitation verification");

        let root = CurrentLxmfRoot::new();
        let paths = crate::config::AppPaths::from_root(root.0.clone());
        paths.ensure().expect("isolated app paths");
        let mut app = crate::app::App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        assert!(
            app.enqueue_runtime_event(crate::runtime::RuntimeBusEvent::MessageReceived(summary,))
        );
        assert_eq!(app.drain_internal_events(), 1);

        let preview = app
            .omenchat_lxmf_invitation_preview
            .pending()
            .expect("authenticated invitation preview");
        assert_eq!(preview.payload.server_destination, DEST);
        assert_eq!(preview.payload.inviter_destination, source);
        assert_eq!(
            preview.sender_evidence,
            crate::chat::handoff::OmenChatInviteSenderEvidence::AuthenticatedMatch
        );
        assert!(app
            .messaging_service
            .conversation(&source)
            .expect("empty control-message thread")
            .messages
            .is_empty());
        assert!(app.status.task.contains("no connection has been opened"));
    }

    #[test]
    fn verified_signed_wire_message_rejects_forged_signature_and_identity_mismatch() {
        let sender = PrivateIdentity::new_from_rand(rand_core::OsRng);
        let other = PrivateIdentity::new_from_rand(rand_core::OsRng);
        let source = hex16_bytes(&lxmf_delivery_destination_hash(sender.as_identity()));
        let envelope = MessageEnvelope {
            peer_hash: DEST.into(),
            title: "Verified".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: None,
            operation: None,
            attachments: Vec::new(),
        };
        let outbound = build_outbound_message(&envelope, &source).expect("outbound");
        let mut wire = outbound
            .message
            .to_wire(Some(&sender))
            .expect("signed wire");

        let mismatch = decode_verified_wire_message(&wire, other.as_identity())
            .expect_err("different source identity must be rejected");
        assert!(mismatch.to_string().contains("does not match"));

        wire[32] ^= 0x01;
        let forged = decode_verified_wire_message(&wire, sender.as_identity())
            .expect_err("mutated signature must be rejected");
        assert!(forged.to_string().contains("missing or invalid"));
    }

    #[test]
    fn forged_signed_attachment_is_rejected_before_filesystem_write() {
        let sender = PrivateIdentity::new_from_rand(rand_core::OsRng);
        let source = hex16_bytes(&lxmf_delivery_destination_hash(sender.as_identity()));
        let source_dir = unique_test_path("omenbrowser-lxmf-forged-source");
        let stored_dir = unique_test_path("omenbrowser-lxmf-forged-stored");
        std::fs::create_dir_all(&source_dir).expect("create source dir");
        let attachment = source_dir.join("forged.bin");
        std::fs::write(&attachment, b"must not be stored").expect("write source attachment");
        let envelope = MessageEnvelope {
            peer_hash: DEST.into(),
            title: "Forged".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: None,
            operation: None,
            attachments: vec![attachment],
        };
        let outbound = build_outbound_message(&envelope, &source).expect("outbound");
        let mut wire = outbound
            .message
            .to_wire(Some(&sender))
            .expect("signed wire");
        wire[32] ^= 0x01;

        decode_verified_wire_message_storing_attachments(&wire, sender.as_identity(), &stored_dir)
            .expect_err("forged attachment message must be rejected");

        assert!(!stored_dir.exists());
        let _ = std::fs::remove_dir_all(source_dir);
        let _ = std::fs::remove_dir_all(stored_dir);
    }

    #[test]
    fn signed_wire_message_preserves_known_lxmf_field_metadata() {
        let provider = NativeReticulumIdentityProvider;
        let private = provider
            .create_identity_material("fielded-sender")
            .expect("native identity");
        let signer = PrivateIdentity::from_private_key_bytes(&private).expect("signer");
        let source = signer.address_hash().to_hex_string();
        let envelope = MessageEnvelope {
            peer_hash: DEST.into(),
            title: "Subject".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: None,
            operation: None,
            attachments: Vec::new(),
        };
        let mut outbound = build_outbound_message(&envelope, &source).expect("outbound");
        outbound.message.fields = Some(rmpv::Value::Map(vec![
            (
                rmpv::Value::Integer(FIELD_RENDERER.into()),
                rmpv::Value::Integer((RENDERER_MICRON as i64).into()),
            ),
            (
                rmpv::Value::Integer(FIELD_THREAD.into()),
                rmpv::Value::Map(vec![
                    (
                        rmpv::Value::String("thread_id".into()),
                        rmpv::Value::String("thread-1".into()),
                    ),
                    (
                        rmpv::Value::String("reply_to".into()),
                        rmpv::Value::String("parent-1".into()),
                    ),
                ]),
            ),
            (
                rmpv::Value::Integer(FIELD_COMMANDS.into()),
                rmpv::Value::Array(Vec::new()),
            ),
            (
                rmpv::Value::Integer(FIELD_CUSTOM_META.into()),
                rmpv::Value::Map(Vec::new()),
            ),
        ]));

        let wire = encode_signed_wire_message(&outbound, &private).expect("encode");
        let summary = decode_wire_message(&wire).expect("decode");

        assert_eq!(
            summary
                .fields
                .get("native_lxmf_renderer")
                .map(String::as_str),
            Some("micron")
        );
        assert_eq!(
            summary
                .fields
                .get("native_lxmf_field_thread")
                .map(String::as_str),
            Some("map:2")
        );
        assert_eq!(
            summary
                .fields
                .get("native_lxmf_thread_id")
                .map(String::as_str),
            Some("thread-1")
        );
        assert_eq!(
            summary
                .fields
                .get("native_lxmf_thread_parent_id")
                .map(String::as_str),
            Some("parent-1")
        );
        assert_eq!(
            summary
                .fields
                .get("native_lxmf_field_commands")
                .map(String::as_str),
            Some("array:0")
        );
        assert_eq!(
            summary
                .fields
                .get("native_lxmf_field_custom_meta")
                .map(String::as_str),
            Some("map:0")
        );
        assert_eq!(
            summary.fields.get("native_lxmf_fields").map(String::as_str),
            Some("commands,custom_meta,renderer,thread")
        );
    }

    #[test]
    fn signed_wire_message_extracts_array_lxmf_thread_metadata() {
        let provider = NativeReticulumIdentityProvider;
        let private = provider
            .create_identity_material("threaded-sender")
            .expect("native identity");
        let signer = PrivateIdentity::from_private_key_bytes(&private).expect("signer");
        let source = signer.address_hash().to_hex_string();
        let envelope = MessageEnvelope {
            peer_hash: DEST.into(),
            title: "Subject".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: None,
            operation: None,
            attachments: Vec::new(),
        };
        let mut outbound = build_outbound_message(&envelope, &source).expect("outbound");
        outbound.message.fields = Some(rmpv::Value::Map(vec![(
            rmpv::Value::Integer(FIELD_THREAD.into()),
            rmpv::Value::Array(vec![
                rmpv::Value::String("thread-array".into()),
                rmpv::Value::String("parent-array".into()),
            ]),
        )]));

        let wire = encode_signed_wire_message(&outbound, &private).expect("encode");
        let summary = decode_wire_message(&wire).expect("decode");

        assert_eq!(
            summary
                .fields
                .get("native_lxmf_thread_id")
                .map(String::as_str),
            Some("thread-array")
        );
        assert_eq!(
            summary
                .fields
                .get("native_lxmf_thread_parent_id")
                .map(String::as_str),
            Some("parent-array")
        );
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
            native_reply_ticket: None,
            operation: None,
            attachments: vec![attachment.clone()],
        };

        let outbound = build_outbound_message(&envelope, &source).expect("outbound");
        let fields = outbound.message.fields.as_ref().expect("attachment fields");
        let summaries = attachment_summaries_from_fields(Some(fields)).expect("summaries");
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
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            assert_eq!(
                std::fs::metadata(stored_path)
                    .expect("stored metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            assert_eq!(
                std::fs::metadata(stored_path.parent().expect("message directory"))
                    .expect("message directory metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
        let replayed =
            decode_wire_message_storing_attachments(&wire, &stored_dir).expect("decode replay");
        assert_eq!(replayed.attachments[0].path.as_ref(), Some(stored_path));
        assert_eq!(
            std::fs::read_dir(stored_path.parent().expect("message directory"))
                .expect("list stored attachments")
                .count(),
            1
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            let referent = stored_dir.join("outside-referent.bin");
            std::fs::write(&referent, b"outside").expect("referent");
            std::fs::remove_file(stored_path).expect("remove stored file");
            symlink(&referent, stored_path).expect("stored-path symlink");
            let error = decode_wire_message_storing_attachments(&wire, &stored_dir)
                .expect_err("unsafe stored path");
            assert!(error.to_string().contains("non-symlink"));
            assert_eq!(
                std::fs::read(&referent).expect("referent remains"),
                b"outside"
            );
        }

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
            native_reply_ticket: None,
            operation: None,
            attachments: vec![unique_test_path("omenbrowser-missing-attachment.bin")],
        };

        let outbound = build_outbound_message(&envelope, SRC).expect("outbound");

        assert!(outbound.message.fields.is_none());
        assert!(outbound.attachments.is_empty());
    }

    #[test]
    fn outbound_attachment_rejects_sparse_next_byte_file_before_reading() {
        let root = unique_test_path("omenbrowser-lxmf-oversized-attachment");
        std::fs::create_dir_all(&root).expect("fixture root");
        let path = root.join("oversized.bin");
        File::create(&path)
            .and_then(|file| file.set_len(MAX_LXMF_ATTACHMENT_BYTES + 1))
            .expect("sparse oversized attachment");
        let envelope = MessageEnvelope {
            peer_hash: DEST.into(),
            title: "Subject".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: None,
            operation: None,
            attachments: vec![path.clone()],
        };

        let error = build_outbound_message(&envelope, SRC).expect_err("oversized attachment");

        assert!(error.to_string().contains("byte limit"));
        assert_eq!(
            std::fs::metadata(path).expect("source metadata").len(),
            MAX_LXMF_ATTACHMENT_BYTES + 1
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn outbound_attachments_accept_exact_per_file_and_aggregate_limits() {
        let root = unique_test_path("omenbrowser-lxmf-exact-attachments");
        std::fs::create_dir_all(&root).expect("fixture root");
        let first = root.join("first.bin");
        let second = root.join("second.bin");
        for path in [&first, &second] {
            File::create(path)
                .and_then(|file| file.set_len(MAX_LXMF_ATTACHMENT_BYTES))
                .expect("sparse exact attachment");
        }
        let envelope = MessageEnvelope {
            peer_hash: DEST.into(),
            title: "Subject".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: None,
            operation: None,
            attachments: vec![first, second],
        };

        let outbound = build_outbound_message(&envelope, SRC).expect("exact attachment limits");

        assert_eq!(outbound.attachments.len(), 2);
        assert_eq!(
            outbound
                .attachments
                .iter()
                .map(|item| item.size)
                .sum::<u64>(),
            MAX_LXMF_ATTACHMENT_TOTAL_BYTES
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn outbound_attachment_item_limit_rejects_before_file_access() {
        let envelope = MessageEnvelope {
            peer_hash: DEST.into(),
            title: "Subject".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: None,
            operation: None,
            attachments: (0..=MAX_LXMF_ATTACHMENT_ITEMS)
                .map(|index| unique_test_path(&format!("missing-{index}")))
                .collect(),
        };

        let error = build_outbound_message(&envelope, SRC).expect_err("attachment item limit");

        assert!(error.to_string().contains("item limit"));
    }

    #[cfg(unix)]
    #[test]
    fn outbound_attachment_symlink_is_rejected_without_reading_referent() {
        use std::os::unix::fs::symlink;

        let root = unique_test_path("omenbrowser-lxmf-symlink-attachment");
        std::fs::create_dir_all(&root).expect("fixture root");
        let referent = root.join("referent.bin");
        let link = root.join("linked.bin");
        std::fs::write(&referent, b"private referent").expect("referent");
        symlink(&referent, &link).expect("attachment symlink");
        let envelope = MessageEnvelope {
            peer_hash: DEST.into(),
            title: "Subject".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: None,
            operation: None,
            attachments: vec![link],
        };

        let error = build_outbound_message(&envelope, SRC).expect_err("symlink rejected");

        assert!(error.to_string().contains("non-symlink"));
        assert_eq!(
            std::fs::read(referent).expect("referent remains"),
            b"private referent"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn inbound_attachment_collection_limits_reject_atomically() {
        let excessive_items = rmpv::Value::Map(vec![(
            rmpv::Value::Integer(FIELD_FILE_ATTACHMENTS.into()),
            rmpv::Value::Array(
                (0..=MAX_LXMF_ATTACHMENT_ITEMS)
                    .map(|index| {
                        rmpv::Value::Array(vec![
                            rmpv::Value::String(format!("{index}.bin").into()),
                            rmpv::Value::Binary(Vec::new()),
                        ])
                    })
                    .collect(),
            ),
        )]);
        let error = attachment_entries_from_fields(Some(&excessive_items))
            .expect_err("excessive attachment items");
        assert!(error.to_string().contains("item limit"));

        let excessive_bytes = rmpv::Value::Map(vec![(
            rmpv::Value::Integer(FIELD_FILE_ATTACHMENTS.into()),
            rmpv::Value::Array(vec![
                rmpv::Value::Array(vec![
                    rmpv::Value::String("first.bin".into()),
                    rmpv::Value::Binary(vec![0; MAX_LXMF_ATTACHMENT_BYTES as usize]),
                ]),
                rmpv::Value::Array(vec![
                    rmpv::Value::String("second.bin".into()),
                    rmpv::Value::Binary(vec![0; MAX_LXMF_ATTACHMENT_BYTES as usize]),
                ]),
                rmpv::Value::Array(vec![
                    rmpv::Value::String("next.bin".into()),
                    rmpv::Value::Binary(vec![0]),
                ]),
            ]),
        )]);
        let error = attachment_entries_from_fields(Some(&excessive_bytes))
            .expect_err("excessive attachment bytes");
        assert!(error.to_string().contains("aggregate limit"));
    }

    #[test]
    fn attachment_storage_component_is_bounded_and_collision_resistant() {
        let first = format!("{}a", "long-name-".repeat(500));
        let second = format!("{}b", "long-name-".repeat(500));

        let first_component = safe_path_component(&first);
        let second_component = safe_path_component(&second);

        assert!(first_component.len() <= MAX_LXMF_ATTACHMENT_PATH_COMPONENT_BYTES);
        assert!(second_component.len() <= MAX_LXMF_ATTACHMENT_PATH_COMPONENT_BYTES);
        assert_ne!(first_component, second_component);
        assert_eq!(first_component, safe_path_component(&first));
    }

    #[test]
    fn private_attachment_fault_preserves_previous_file_and_removes_stage() {
        let root = unique_test_path("omenbrowser-lxmf-attachment-fault");
        std::fs::create_dir_all(&root).expect("fixture root");
        let path = root.join("attachment.bin");
        std::fs::write(&path, b"previous").expect("previous attachment");

        let result = write_private_attachment_with(&path, b"replacement", || {
            Err(std::io::Error::other("injected pre-commit failure"))
        });

        assert!(result.is_err());
        assert_eq!(std::fs::read(&path).expect("previous remains"), b"previous");
        assert_eq!(std::fs::read_dir(&root).expect("list fixture").count(), 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn propagation_transient_rejects_invalid_recipient_identity_without_panicking() {
        let wire = lxmf::WireMessage::new(
            [0x11; 16],
            [0x22; 16],
            lxmf::Payload::new(42.0, Some(b"body".to_vec()), None, None, None),
        );
        let invalid_verifying_key = (0_u8..=u8::MAX)
            .map(|byte| [byte; 32])
            .find(|bytes| Identity::try_new_from_slices(&[0x11; 32], bytes).is_err())
            .expect("at least one compressed point encoding must be invalid");
        let mut invalid_recipient = [0u8; 64];
        invalid_recipient[..32].fill(0x11);
        invalid_recipient[32..].copy_from_slice(&invalid_verifying_key);

        let error = pack_identity_salted_propagation_transient(&wire, invalid_recipient)
            .expect_err("invalid verifying key must fail closed");

        assert!(error.to_string().contains("recipient identity is invalid"));
    }

    #[test]
    fn propagated_lxmf_data_decrypts_to_message_summary() {
        let sender = PrivateIdentity::new_from_rand(rand_core::OsRng);
        let receiver = PrivateIdentity::new_from_rand(rand_core::OsRng);
        let receiver_hash = lxmf_delivery_hash(&receiver);
        let sender_hash = lxmf_delivery_hash(&sender);
        let payload = lxmf::Payload::new(
            42.0,
            Some(b"Body".to_vec()),
            Some(b"Subject".to_vec()),
            None,
            None,
        );
        let mut wire = lxmf::WireMessage::new(receiver_hash, sender_hash, payload);
        wire.sign(&sender).expect("sign");
        let receiver_identity = receiver.as_identity();
        let mut receiver_public = [0u8; 64];
        receiver_public[..32].copy_from_slice(receiver_identity.public_key_bytes());
        receiver_public[32..].copy_from_slice(receiver_identity.verifying_key_bytes());
        let (lxmf_data, _transient_id) =
            pack_identity_salted_propagation_transient(&wire, receiver_public)
                .expect("propagation data");

        let summary = decode_propagated_lxmf_data(
            lxmf_data.as_slice(),
            receiver.to_private_key_bytes().as_slice(),
        )
        .expect("decode propagated");

        assert_eq!(summary.peer_hash, hex16_bytes(&sender_hash));
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
    fn clean_wire_propagated_lxmf_data_decrypts_to_message_summary() {
        let sender = PrivateIdentity::new_from_rand(rand_core::OsRng);
        let receiver = PrivateIdentity::new_from_rand(rand_core::OsRng);
        let receiver_hash = lxmf_delivery_hash(&receiver);
        let sender_hash = lxmf_delivery_hash(&sender);
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
            .expect("pack clean propagation transient");

        let summary = decode_propagated_lxmf_data(
            lxmf_data.as_slice(),
            receiver.to_private_key_bytes().as_slice(),
        )
        .expect("decode propagated");

        assert_eq!(summary.peer_hash, hex16_bytes(&sender_hash));
        assert_eq!(summary.title, "Subject");
        assert_eq!(summary.content, "Body");
        assert_eq!(summary.transport_method, AppTransportMethod::Propagated);
    }

    #[test]
    fn verified_propagated_wire_accepts_matching_announced_sender() {
        let sender = PrivateIdentity::new_from_rand(rand_core::OsRng);
        let receiver = PrivateIdentity::new_from_rand(rand_core::OsRng);
        let sender_hash = lxmf_delivery_hash(&sender);
        let payload = lxmf::Payload::new(
            42.0,
            Some(b"Body".to_vec()),
            Some(b"Verified propagated".to_vec()),
            None,
            None,
        );
        let mut message =
            lxmf::WireMessage::new(lxmf_delivery_hash(&receiver), sender_hash, payload);
        message.sign(&sender).expect("sign");
        let (encrypted, _) = message
            .pack_propagation_transient_with_rng(receiver.as_identity(), rand_core::OsRng)
            .expect("pack propagated");
        let wire =
            unpack_propagated_lxmf_wire(&encrypted, receiver.to_private_key_bytes().as_slice())
                .expect("decrypt propagated wire");
        let attachments_dir = unique_test_path("verified-propagated");

        let summary = decode_verified_propagated_wire_message_storing_attachments(
            &wire,
            sender.as_identity(),
            &attachments_dir,
        )
        .expect("verified propagated message");

        assert_eq!(summary.peer_hash, hex16_bytes(&sender_hash));
        assert_eq!(summary.title, "Verified propagated");
        assert_eq!(summary.transport_method, AppTransportMethod::Propagated);
        assert_eq!(
            summary
                .fields
                .get(LXMF_SOURCE_AUTHENTICATED_FIELD)
                .map(String::as_str),
            Some("true")
        );
        assert_eq!(
            summary
                .fields
                .get("native_lxmf_delivery_source")
                .map(String::as_str),
            Some("propagation_sync")
        );
        let _ = std::fs::remove_dir_all(attachments_dir);
    }

    #[test]
    fn verified_propagated_wire_rejects_forgery_and_sender_mismatch() {
        let sender = PrivateIdentity::new_from_rand(rand_core::OsRng);
        let other = PrivateIdentity::new_from_rand(rand_core::OsRng);
        let receiver = PrivateIdentity::new_from_rand(rand_core::OsRng);
        let payload = lxmf::Payload::new(
            42.0,
            Some(b"Body".to_vec()),
            Some(b"Rejected propagated".to_vec()),
            None,
            None,
        );
        let mut message = lxmf::WireMessage::new(
            lxmf_delivery_hash(&receiver),
            lxmf_delivery_hash(&sender),
            payload,
        );
        message.sign(&sender).expect("sign");
        let (encrypted, _) = message
            .pack_propagation_transient_with_rng(receiver.as_identity(), rand_core::OsRng)
            .expect("pack propagated");
        let wire =
            unpack_propagated_lxmf_wire(&encrypted, receiver.to_private_key_bytes().as_slice())
                .expect("decrypt propagated wire");
        let attachments_dir = unique_test_path("rejected-propagated");

        let mismatch = decode_verified_propagated_wire_message_storing_attachments(
            &wire,
            other.as_identity(),
            &attachments_dir,
        )
        .expect_err("wrong sender identity must be rejected");
        assert!(mismatch.to_string().contains("does not match"));

        message.signature.as_mut().expect("signature")[0] ^= 0x01;
        let (forged_encrypted, _) = message
            .pack_propagation_transient_with_rng(receiver.as_identity(), rand_core::OsRng)
            .expect("pack forged propagated");
        let forged_wire = unpack_propagated_lxmf_wire(
            &forged_encrypted,
            receiver.to_private_key_bytes().as_slice(),
        )
        .expect("decrypt forged wire");
        let forged = decode_verified_propagated_wire_message_storing_attachments(
            &forged_wire,
            sender.as_identity(),
            &attachments_dir,
        )
        .expect_err("forged propagated signature must be rejected");
        assert!(forged.to_string().contains("missing or invalid"));
        assert!(!attachments_dir.exists());
    }

    #[test]
    fn forged_propagated_attachment_is_rejected_before_filesystem_write() {
        let sender = PrivateIdentity::new_from_rand(rand_core::OsRng);
        let receiver = PrivateIdentity::new_from_rand(rand_core::OsRng);
        let source_dir = unique_test_path("forged-propagated-source");
        let stored_dir = unique_test_path("forged-propagated-stored");
        std::fs::create_dir_all(&source_dir).expect("source dir");
        let attachment = source_dir.join("forged-propagated.bin");
        std::fs::write(&attachment, b"must not be stored").expect("attachment fixture");
        let envelope = MessageEnvelope {
            peer_hash: hex16_bytes(&lxmf_delivery_hash(&receiver)),
            title: "Forged propagated attachment".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Propagated,
            include_ticket: false,
            native_reply_ticket: None,
            operation: None,
            attachments: vec![attachment],
        };
        let outbound =
            build_outbound_message(&envelope, &hex16_bytes(&lxmf_delivery_hash(&sender)))
                .expect("outbound");
        let payload = lxmf::Payload::new(
            outbound.message.timestamp.unwrap_or(42.0),
            Some(outbound.message.content.clone()),
            Some(outbound.message.title.clone()),
            outbound.message.fields.clone(),
            None,
        );
        let mut message = lxmf::WireMessage::new(
            outbound.message.destination_hash.expect("destination"),
            outbound.message.source_hash.expect("source"),
            payload,
        );
        message.sign(&sender).expect("sign");
        message.signature.as_mut().expect("signature")[0] ^= 0x01;
        let (encrypted, _) = message
            .pack_propagation_transient_with_rng(receiver.as_identity(), rand_core::OsRng)
            .expect("pack forged propagated");
        let wire =
            unpack_propagated_lxmf_wire(&encrypted, receiver.to_private_key_bytes().as_slice())
                .expect("decrypt forged wire");

        decode_verified_propagated_wire_message_storing_attachments(
            &wire,
            sender.as_identity(),
            &stored_dir,
        )
        .expect_err("forged propagated attachment must be rejected");

        assert!(!stored_dir.exists());
        let _ = std::fs::remove_dir_all(source_dir);
        let _ = std::fs::remove_dir_all(stored_dir);
    }

    #[test]
    fn propagated_lxmf_destination_helpers_match_wire_destination() {
        let receiver = PrivateIdentity::new_from_rand(rand_core::OsRng);
        let receiver_hash = lxmf_delivery_hash(&receiver);
        let helper_hash = lxmf_delivery_destination_hash_from_private_identity_bytes(
            receiver.to_private_key_bytes().as_slice(),
        )
        .expect("helper hash");
        let mut lxm_data = receiver_hash.to_vec();
        lxm_data.extend_from_slice(b"encrypted-placeholder");

        assert_eq!(helper_hash, receiver_hash);
        assert_eq!(
            propagated_lxmf_destination_hash(lxm_data.as_slice()),
            Some(receiver_hash)
        );
        assert_eq!(propagated_lxmf_destination_hash(&[1u8; 16]), None);
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
    fn propagation_envelope_rejects_unbounded_or_trailing_msgpack() {
        let mut trailing = lxmf::WireMessage::pack_propagation_envelope(42.0, b"lxmf-data", None)
            .expect("envelope");
        trailing.push(0xc0);
        assert!(propagation_envelope_entries(&trailing).is_err());

        let oversized_scalar = [0xc6, 0x00, 0x80, 0x00, 0x01];
        assert!(propagation_envelope_entries(&oversized_scalar).is_err());

        let oversized_container = [0xdd, 0x00, 0x00, 0x01, 0x01];
        assert!(propagation_envelope_entries(&oversized_container).is_err());

        let mut deep = vec![0x91; MAX_LXMF_PROPAGATION_DEPTH + 2];
        deep.push(0xc0);
        assert!(propagation_envelope_entries(&deep).is_err());
        assert!(
            propagation_envelope_entries(&vec![0xc0; MAX_LXMF_PROPAGATION_ENVELOPE_BYTES + 1])
                .is_err()
        );
    }

    #[test]
    fn invalid_destination_is_rejected_before_wire_encoding() {
        let envelope = MessageEnvelope {
            peer_hash: "not-a-hash".into(),
            title: "Subject".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: None,
            operation: None,
            attachments: Vec::new(),
        };

        let error = build_outbound_message(&envelope, SRC).expect_err("invalid hash");

        assert!(error.to_string().contains("32 hex"));
    }

    #[test]
    #[ignore = "explicit current-Python LXMF direct-delivery interoperability test"]
    fn current_python_lxmf_router_accepts_rust_direct_signed_message() {
        const TITLE: &str = "OMEN Rust direct LXMF";
        const CONTENT: &str = "current Python LXMF 1.1.1 received this signed message";

        let root = CurrentLxmfRoot::new();
        let port = current_lxmf_port();
        let transport_identity =
            TransportPrivateIdentity::new_from_name("omen-current-python-lxmf-client");
        let local_identity =
            PrivateIdentity::from_private_key_bytes(&transport_identity.to_private_key_bytes())
                .expect("matching core signing identity");
        let local_lxmf_hash = lxmf_delivery_hash(&local_identity);
        let local_lxmf_hash_hex = hex16_bytes(&local_lxmf_hash);
        let peer = CurrentPythonLxmfPeer::spawn(&root.0, port, &local_lxmf_hash_hex);
        let destination = AddressHash::new_from_hex_string(
            peer.ready["destination"]
                .as_str()
                .expect("current Python delivery destination"),
        )
        .expect("valid current Python destination hash");

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("current LXMF Tokio runtime");
        let packet_hash = runtime.block_on(async {
            let (receipt_tx, mut receipt_rx) = tokio::sync::mpsc::channel(2);
            let mut transport = Transport::new(TransportConfig::new(
                "current-python-lxmf-interop",
                &transport_identity,
                true,
            ));
            let local_delivery = transport
                .add_destination(
                    transport_identity.clone(),
                    rns_transport::destination::DestinationName::new("lxmf", "delivery"),
                )
                .await;
            assert_eq!(
                local_delivery
                    .lock()
                    .await
                    .desc
                    .address_hash
                    .to_hex_string(),
                local_lxmf_hash_hex
            );
            transport
                .set_receipt_handler(Box::new(CurrentLxmfReceiptCapture { sender: receipt_tx }))
                .await;
            let transport = Arc::new(transport);
            let mut announces = transport.recv_announces().await;

            let (iface_address, iface_task) = {
                let manager = transport.iface_manager();
                let mut manager = manager.lock().await;
                let client = IfacTcpClient::new(
                    format!("127.0.0.1:{port}"),
                    Some("omen-ifac-vector".into()),
                    Some("public-test-fixture".into()),
                    16,
                )
                .expect("current LXMF IFAC client");
                let context = manager.new_context(client);
                let address = *context.channel.address();
                (address, tokio::spawn(IfacTcpClient::spawn(context)))
            };

            assert!(
                transport
                    .await_path(&destination, Duration::from_secs(8), Some(iface_address))
                    .await,
                "Rust path request did not yield current Python LXMF announce"
            );
            let event = tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    let event = announces
                        .recv()
                        .await
                        .expect("current Python announce stream remains open");
                    if event.destination.lock().await.desc.address_hash == destination {
                        return event;
                    }
                }
            })
            .await
            .expect("matching current Python LXMF announce");
            let local_announce = local_delivery
                .lock()
                .await
                .announce(rand_core::OsRng, None)
                .expect("current LXMF source announce");
            let announce_dispatch = transport
                .send_packet_broadcast_with_trace(local_announce)
                .await
                .dispatch;
            assert!(
                announce_dispatch.sent_ifaces > 0 || announce_dispatch.queued_ifaces > 0,
                "current LXMF source announce was not dispatched"
            );
            peer.wait_for_source_announce();
            let destination_desc = event.destination.lock().await.desc;
            let link = transport.link(destination_desc).await;
            await_link_activation(&transport, &link, Duration::from_secs(8))
                .await
                .expect("Rust-to-current-Python LXMF link activation");

            let envelope = MessageEnvelope {
                peer_hash: destination.to_hex_string(),
                title: TITLE.into(),
                body: CONTENT.into(),
                delivery_mode: DeliveryMode::Direct,
                include_ticket: false,
                native_reply_ticket: None,
                operation: None,
                attachments: Vec::new(),
            };
            let outbound = build_outbound_message(&envelope, &hex16_bytes(&local_lxmf_hash))
                .expect("build current Python LXMF message");
            let wire = encode_signed_wire_message(
                &outbound,
                local_identity.to_private_key_bytes().as_slice(),
            )
            .expect("encode signed current Python LXMF message");
            let sent = send_on_link_observed(
                &transport,
                &link,
                wire.as_slice(),
                |_| {},
                |_| panic!("small current LXMF fixture unexpectedly used a Resource"),
            )
            .await
            .expect("send current Python LXMF direct message");
            let packet_hash = match sent {
                LinkSendResult::Packet(packet) => packet.hash(),
                LinkSendResult::Resource(_) => {
                    panic!("small current LXMF fixture unexpectedly used a Resource")
                }
            };
            let receipt = tokio::time::timeout(Duration::from_secs(4), receipt_rx.recv())
                .await
                .expect("current Python LXMF packet proof timeout")
                .expect("current LXMF receipt channel remains open");
            assert_eq!(receipt, packet_hash.to_bytes());

            assert_eq!(transport.detach_interfaces().await, 1);
            tokio::time::timeout(Duration::from_secs(1), iface_task)
                .await
                .expect("current LXMF IFAC task shutdown")
                .expect("current LXMF IFAC task join");
            packet_hash
        });

        let result = peer.finish();
        assert_eq!(result["received"], true);
        assert_eq!(result["title"], TITLE);
        assert_eq!(result["content"], CONTENT);
        assert_eq!(result["source_hash"], hex16_bytes(&local_lxmf_hash));
        assert_eq!(result["destination_hash"], destination.to_hex_string());
        assert_eq!(result["signature_validated"], true);
        assert_eq!(result["method"], result["direct_method"]);
        assert_ne!(packet_hash.to_bytes(), [0u8; 32]);
    }

    #[test]
    #[ignore = "explicit current-Python-to-Rust LXMF direct-delivery interoperability test"]
    fn rust_accepts_current_python_lxmf_router_direct_signed_message() {
        let root = CurrentLxmfRoot::new();
        let port = current_lxmf_port();
        let transport_identity =
            TransportPrivateIdentity::new_from_name("omen-current-python-lxmf-receiver");
        let local_identity =
            PrivateIdentity::from_private_key_bytes(&transport_identity.to_private_key_bytes())
                .expect("matching core receiver identity");
        let local_lxmf_hash = lxmf_delivery_hash(&local_identity);
        let local_lxmf_hash_hex = hex16_bytes(&local_lxmf_hash);
        let sender = CurrentPythonLxmfSender::spawn(&root.0, port, &local_lxmf_hash_hex);
        let python_source = AddressHash::new_from_hex_string(
            sender.ready["source"]
                .as_str()
                .expect("current Python LXMF source destination"),
        )
        .expect("valid current Python LXMF source hash");
        let expected_title = sender.ready["title"]
            .as_str()
            .expect("current Python LXMF title")
            .to_string();
        let expected_content = sender.ready["content"]
            .as_str()
            .expect("current Python LXMF content")
            .to_string();

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("current Python sender Tokio runtime");
        let (message, signature_valid, wire_message_id) = runtime.block_on(async {
            let transport = Transport::new(TransportConfig::new(
                "current-python-lxmf-receiver",
                &transport_identity,
                true,
            ));
            let local_delivery = transport
                .add_destination(
                    transport_identity.clone(),
                    rns_transport::destination::DestinationName::new("lxmf", "delivery"),
                )
                .await;
            let transport = Arc::new(transport);
            let mut announces = transport.recv_announces().await;
            let mut inbound_links = transport.in_link_events();

            let (iface_address, iface_task) = {
                let manager = transport.iface_manager();
                let mut manager = manager.lock().await;
                let client = IfacTcpClient::new(
                    format!("127.0.0.1:{port}"),
                    Some("omen-ifac-vector".into()),
                    Some("public-test-fixture".into()),
                    16,
                )
                .expect("current Python sender IFAC client");
                let context = manager.new_context(client);
                let address = *context.channel.address();
                (address, tokio::spawn(IfacTcpClient::spawn(context)))
            };

            assert!(
                transport
                    .await_path(&python_source, Duration::from_secs(8), Some(iface_address))
                    .await,
                "Rust path request did not yield current Python sender announce"
            );
            let source_event = tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    let event = announces
                        .recv()
                        .await
                        .expect("current Python sender announce stream remains open");
                    if event.destination.lock().await.desc.address_hash == python_source {
                        return event;
                    }
                }
            })
            .await
            .expect("matching current Python sender announce");
            let source_identity = source_event.destination.lock().await.desc.identity;
            let source_identity = Identity::new_from_slices(
                source_identity.public_key_bytes(),
                source_identity.verifying_key_bytes(),
            );

            let local_announce = local_delivery
                .lock()
                .await
                .announce(rand_core::OsRng, None)
                .expect("current Rust receiver LXMF announce");
            let announce_dispatch = transport
                .send_packet_broadcast_with_trace(local_announce)
                .await
                .dispatch;
            assert!(
                announce_dispatch.sent_ifaces > 0 || announce_dispatch.queued_ifaces > 0,
                "current Rust receiver LXMF announce was not dispatched"
            );

            let wire_bytes = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let event = inbound_links
                        .recv()
                        .await
                        .expect("current Python inbound link stream remains open");
                    if let rns_transport::destination::link::LinkEvent::Data(payload) = event.event
                    {
                        return payload.as_slice().to_vec();
                    }
                }
            })
            .await
            .expect("current Python direct LXMF payload");
            let wire = lxmf::WireMessage::unpack(&wire_bytes)
                .expect("current Python wire parses with lxmf-wire 0.9.5");
            let signature_valid = wire
                .verify(&source_identity)
                .expect("current Python LXMF signature verification");
            let wire_message_id = hex32(&wire.message_id());
            let message = decode_verified_wire_message(&wire_bytes, &source_identity)
                .expect("production verifier accepts current Python LXMF wire");

            assert_eq!(transport.detach_interfaces().await, 1);
            tokio::time::timeout(Duration::from_secs(1), iface_task)
                .await
                .expect("current Python sender IFAC task shutdown")
                .expect("current Python sender IFAC task join");
            (message, signature_valid, wire_message_id)
        });

        let result = sender.finish();
        assert_eq!(result["delivered"], true);
        assert_eq!(result["failed"], false);
        assert_eq!(result["source_hash"], python_source.to_hex_string());
        assert_eq!(result["destination_hash"], local_lxmf_hash_hex);
        assert_eq!(result["method"], result["direct_method"]);
        assert_eq!(result["message_id"], wire_message_id);
        assert_eq!(message.peer_hash, python_source.to_hex_string());
        assert_eq!(message.title, expected_title);
        assert_eq!(message.content, expected_content);
        assert_eq!(
            message.message_id.as_deref(),
            Some(wire_message_id.as_str())
        );
        assert!(message.incoming);
        assert!(signature_valid);
    }

    #[test]
    #[ignore = "explicit current-Python live LXMF ticket round-trip interoperability test"]
    fn current_python_lxmf_live_ticket_roundtrip_uses_rust_issued_ticket() {
        run_python_lxmf_live_ticket_roundtrip(
            "current-python-lxmf-live-ticket",
            "OMEN_PYTHON_RNS_SOURCE",
            None,
            "1.4.2",
            "1.1.1",
        );
    }

    #[test]
    #[ignore = "explicit pinned-Python live LXMF ticket round-trip interoperability test"]
    fn pinned_python_lxmf_live_ticket_roundtrip_uses_rust_issued_ticket() {
        run_python_lxmf_live_ticket_roundtrip(
            "pinned-python-lxmf-live-ticket",
            "OMEN_PINNED_RNS_SOURCE",
            Some("OMEN_PINNED_LXMF_SOURCE"),
            "1.5.0",
            "0.9.6",
        );
    }

    fn run_python_lxmf_live_ticket_roundtrip(
        case: &str,
        rns_source_env: &str,
        lxmf_source_env: Option<&str>,
        expected_rns: &str,
        expected_lxmf: &str,
    ) {
        const TITLE: &str = "OMEN Rust ticket issue";
        const CONTENT: &str = "Python must use this ticket for its direct reply";

        let root = CurrentLxmfRoot::new();
        let port = current_lxmf_port();
        let transport_identity = TransportPrivateIdentity::new_from_name(case);
        let local_identity =
            PrivateIdentity::from_private_key_bytes(&transport_identity.to_private_key_bytes())
                .expect("matching core ticket issuer identity");
        let local_lxmf_hash = lxmf_delivery_hash(&local_identity);
        let local_lxmf_hash_hex = hex16_bytes(&local_lxmf_hash);
        let peer = PythonLxmfTicketRoundtripPeer::spawn(
            &root.0,
            port,
            &local_lxmf_hash_hex,
            rns_source_env,
            lxmf_source_env,
            expected_rns,
            expected_lxmf,
        );
        let python_destination = AddressHash::new_from_hex_string(
            peer.ready["destination"]
                .as_str()
                .expect("Python ticket delivery destination"),
        )
        .expect("valid Python ticket destination");
        let expected_reply_title = peer.ready["reply_title"]
            .as_str()
            .expect("Python ticket reply title")
            .to_string();
        let expected_reply_content = peer.ready["reply_content"]
            .as_str()
            .expect("Python ticket reply content")
            .to_string();

        let tokio = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("ticket round-trip Tokio runtime");
        let (reply, reply_message_id) = tokio.block_on(async {
            let transport = Transport::new(TransportConfig::new(
                format!("{case}-transport"),
                &transport_identity,
                true,
            ));
            let local_delivery = transport
                .add_destination(
                    transport_identity.clone(),
                    rns_transport::destination::DestinationName::new("lxmf", "delivery"),
                )
                .await;
            let transport = Arc::new(transport);
            let mut announces = transport.recv_announces().await;
            let mut inbound_links = transport.in_link_events();

            let (iface_address, iface_task) = {
                let manager = transport.iface_manager();
                let mut manager = manager.lock().await;
                let client = IfacTcpClient::new(
                    format!("127.0.0.1:{port}"),
                    Some("omen-ifac-vector".into()),
                    Some("public-test-fixture".into()),
                    16,
                )
                .expect("ticket round-trip IFAC client");
                let context = manager.new_context(client);
                let address = *context.channel.address();
                (address, tokio::spawn(IfacTcpClient::spawn(context)))
            };

            assert!(
                transport
                    .await_path(
                        &python_destination,
                        Duration::from_secs(8),
                        Some(iface_address),
                    )
                    .await,
                "Rust path request did not yield Python ticket-peer announce"
            );
            let python_event = tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    let event = announces
                        .recv()
                        .await
                        .expect("Python ticket announce stream remains open");
                    if event.destination.lock().await.desc.address_hash == python_destination {
                        return event;
                    }
                }
            })
            .await
            .expect("matching Python ticket-peer announce");
            let python_identity = python_event.destination.lock().await.desc.identity;
            let python_identity = Identity::new_from_slices(
                python_identity.public_key_bytes(),
                python_identity.verifying_key_bytes(),
            );

            let local_announce = local_delivery
                .lock()
                .await
                .announce(rand_core::OsRng, None)
                .expect("Rust ticket issuer announce");
            let dispatch = transport
                .send_packet_broadcast_with_trace(local_announce)
                .await
                .dispatch;
            assert!(
                dispatch.sent_ifaces > 0 || dispatch.queued_ifaces > 0,
                "Rust ticket issuer announce was not dispatched"
            );
            peer.wait_for_source_announce();

            let link = transport
                .link(python_event.destination.lock().await.desc)
                .await;
            await_link_activation(&transport, &link, Duration::from_secs(8))
                .await
                .expect("Rust-to-Python ticket link activation");
            let envelope = MessageEnvelope {
                peer_hash: python_destination.to_hex_string(),
                title: TITLE.into(),
                body: CONTENT.into(),
                delivery_mode: DeliveryMode::Direct,
                include_ticket: true,
                native_reply_ticket: None,
                operation: None,
                attachments: Vec::new(),
            };
            let outbound = build_outbound_message(&envelope, &local_lxmf_hash_hex)
                .expect("build ticket-issuing outbound message");
            let (_, issued_ticket) = ticket_entry_from_fields(outbound.message.fields.as_ref())
                .expect("production outbound contains reply ticket");
            let wire = encode_signed_wire_message(
                &outbound,
                local_identity.to_private_key_bytes().as_slice(),
            )
            .expect("encode signed ticket-issuing message");
            send_on_link_observed(
                &transport,
                &link,
                wire.as_slice(),
                |_| {},
                |_| panic!("small ticket fixture unexpectedly used a Resource"),
            )
            .await
            .expect("send ticket-issuing direct message");

            let reply_bytes = tokio::time::timeout(Duration::from_secs(15), async {
                loop {
                    let event = inbound_links
                        .recv()
                        .await
                        .expect("ticket reply link stream remains open");
                    if let rns_transport::destination::link::LinkEvent::Data(payload) = event.event
                    {
                        return payload.as_slice().to_vec();
                    }
                }
            })
            .await
            .expect("Python ticket-stamped direct reply");
            let reply_wire = lxmf::WireMessage::unpack(&reply_bytes)
                .expect("Python ticket reply parses with lxmf-wire");
            assert_eq!(reply_wire.destination, local_lxmf_hash);
            assert_eq!(
                reply_wire.source,
                parse_lxmf_hash(&python_destination.to_hex_string())
                    .expect("parse Python ticket source hash"),
            );
            assert!(
                reply_wire
                    .verify(&python_identity)
                    .expect("verify Python ticket reply signature"),
                "Python ticket reply signature was invalid"
            );
            let reply_message_id = reply_wire.message_id();
            let expected_stamp = ticket_stamp_for_message(&issued_ticket, &reply_message_id)
                .expect("calculate expected Python ticket stamp");
            assert_eq!(
                reply_wire.payload.stamp.as_ref().map(AsRef::as_ref),
                Some(expected_stamp.as_slice()),
                "Python reply did not use the exact Rust-issued ticket"
            );
            let reply = decode_verified_wire_message(&reply_bytes, &python_identity)
                .expect("production verifier accepts Python ticket reply");

            tokio::time::sleep(Duration::from_millis(150)).await;
            assert_eq!(transport.detach_interfaces().await, 1);
            tokio::time::timeout(Duration::from_secs(1), iface_task)
                .await
                .expect("ticket IFAC task shutdown")
                .expect("ticket IFAC task join");
            (reply, hex32(&reply_message_id))
        });

        let result = peer.finish();
        assert_eq!(result["passed"], true);
        assert_eq!(result["received"], true);
        assert_eq!(result["received_signature_validated"], true);
        assert_eq!(result["received_source"], local_lxmf_hash_hex);
        assert_eq!(result["ticket_shape_valid"], true);
        assert_eq!(result["ticket_remembered"], true);
        assert_eq!(result["reply_ticket_applied"], true);
        assert_eq!(result["reply_ticket_cost"], true);
        assert_eq!(result["reply_stamp_matches"], true);
        assert_eq!(result["reply_delivered"], true);
        assert_eq!(result["reply_failed"], false);
        assert_eq!(result["reply_message_id"], reply_message_id);
        assert_eq!(reply.peer_hash, python_destination.to_hex_string());
        assert_eq!(reply.title, expected_reply_title);
        assert_eq!(reply.content, expected_reply_content);
        assert_eq!(reply.message_id.as_deref(), Some(reply_message_id.as_str()));
        assert!(reply.incoming);
    }

    #[test]
    #[ignore = "explicit current-Python live LXMF direct-stamp admission interoperability test"]
    fn current_python_lxmf_live_direct_stamp_accepts_stamped_and_rejects_unstamped() {
        run_python_lxmf_live_direct_stamp_admission(
            "current-python-lxmf-live-direct-stamp",
            "OMEN_PYTHON_RNS_SOURCE",
            None,
            "1.4.2",
            "1.1.1",
        );
    }

    #[test]
    #[ignore = "explicit pinned-Python live LXMF direct-stamp admission interoperability test"]
    fn pinned_python_lxmf_live_direct_stamp_accepts_stamped_and_rejects_unstamped() {
        run_python_lxmf_live_direct_stamp_admission(
            "pinned-python-lxmf-live-direct-stamp",
            "OMEN_PINNED_RNS_SOURCE",
            Some("OMEN_PINNED_LXMF_SOURCE"),
            "1.5.0",
            "0.9.6",
        );
    }

    fn run_python_lxmf_live_direct_stamp_admission(
        case: &str,
        rns_source_env: &str,
        lxmf_source_env: Option<&str>,
        expected_rns: &str,
        expected_lxmf: &str,
    ) {
        const STAMPED_TITLE: &str = "OMEN Rust stamped direct LXMF";
        const UNSTAMPED_TITLE: &str = "OMEN Rust unstamped direct LXMF";

        let root = CurrentLxmfRoot::new();
        let port = current_lxmf_port();
        let transport_identity = TransportPrivateIdentity::new_from_name(case);
        let local_identity =
            PrivateIdentity::from_private_key_bytes(&transport_identity.to_private_key_bytes())
                .expect("matching core direct-stamp identity");
        let local_lxmf_hash = lxmf_delivery_hash(&local_identity);
        let local_lxmf_hash_hex = hex16_bytes(&local_lxmf_hash);
        let peer = PythonLxmfDirectStampPeer::spawn(
            &root.0,
            port,
            &local_lxmf_hash_hex,
            rns_source_env,
            lxmf_source_env,
            expected_rns,
            expected_lxmf,
        );
        assert_eq!(peer.ready["stamp_cost"], 1);
        let python_destination = AddressHash::new_from_hex_string(
            peer.ready["destination"]
                .as_str()
                .expect("Python direct-stamp delivery destination"),
        )
        .expect("valid Python direct-stamp destination");

        let tokio = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("direct-stamp Tokio runtime");
        let (stamp_value, stamp_attempts, elapsed) = tokio.block_on(async {
            let started = Instant::now();
            let transport = Transport::new(TransportConfig::new(
                format!("{case}-transport"),
                &transport_identity,
                true,
            ));
            let local_delivery = transport
                .add_destination(
                    transport_identity.clone(),
                    rns_transport::destination::DestinationName::new("lxmf", "delivery"),
                )
                .await;
            let transport = Arc::new(transport);
            let mut announces = transport.recv_announces().await;

            let (iface_address, iface_task) = {
                let manager = transport.iface_manager();
                let mut manager = manager.lock().await;
                let client = IfacTcpClient::new(
                    format!("127.0.0.1:{port}"),
                    Some("omen-ifac-vector".into()),
                    Some("public-test-fixture".into()),
                    16,
                )
                .expect("direct-stamp IFAC client");
                let context = manager.new_context(client);
                let address = *context.channel.address();
                (address, tokio::spawn(IfacTcpClient::spawn(context)))
            };

            assert!(
                transport
                    .await_path(
                        &python_destination,
                        Duration::from_secs(8),
                        Some(iface_address),
                    )
                    .await,
                "Rust path request did not yield Python direct-stamp announce"
            );
            let python_event = tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    let event = announces
                        .recv()
                        .await
                        .expect("Python direct-stamp announce stream remains open");
                    if event.destination.lock().await.desc.address_hash == python_destination {
                        return event;
                    }
                }
            })
            .await
            .expect("matching Python direct-stamp announce");
            assert_eq!(
                delivery_announce_stamp_cost(python_event.app_data.as_slice()),
                Some(1),
                "Rust did not parse the Python peer's authenticated direct-stamp cost"
            );

            let local_announce = local_delivery
                .lock()
                .await
                .announce(rand_core::OsRng, None)
                .expect("Rust direct-stamp source announce");
            let dispatch = transport
                .send_packet_broadcast_with_trace(local_announce)
                .await
                .dispatch;
            assert!(
                dispatch.sent_ifaces > 0 || dispatch.queued_ifaces > 0,
                "Rust direct-stamp source announce was not dispatched"
            );
            peer.wait_for_source_announce();

            let link = transport
                .link(python_event.destination.lock().await.desc)
                .await;
            await_link_activation(&transport, &link, Duration::from_secs(8))
                .await
                .expect("Rust-to-Python direct-stamp link activation");

            let stamped_envelope = MessageEnvelope {
                peer_hash: python_destination.to_hex_string(),
                title: STAMPED_TITLE.into(),
                body: "Python must accept this bounded direct proof".into(),
                delivery_mode: DeliveryMode::Direct,
                include_ticket: false,
                native_reply_ticket: None,
                operation: None,
                attachments: Vec::new(),
            };
            let stamped = crate::runtime::native_lxmf::client::build_sdk_wire_delivery_from_envelope_with_policy(
                &stamped_envelope,
                &local_lxmf_hash_hex,
                local_identity.to_private_key_bytes().as_slice(),
                Some(1),
                None,
                Some(1),
                || false,
            )
            .expect("production SDK builder creates bounded direct stamp");
            let direct_stamp = stamped
                .direct_stamp
                .as_ref()
                .expect("bounded direct stamp metadata");
            assert_eq!(direct_stamp.target_cost, 1);
            assert!(direct_stamp.attempts <= CLEAN_DIRECT_STAMP_MAX_ATTEMPTS);
            send_on_link_observed(
                &transport,
                &link,
                stamped.wire_bytes.as_slice(),
                |_| {},
                |_| panic!("small stamped fixture unexpectedly used a Resource"),
            )
            .await
            .expect("send stamped direct message");

            let unstamped_envelope = MessageEnvelope {
                peer_hash: python_destination.to_hex_string(),
                title: UNSTAMPED_TITLE.into(),
                body: "Python must reject this unstamped control".into(),
                delivery_mode: DeliveryMode::Direct,
                include_ticket: false,
                native_reply_ticket: None,
                operation: None,
                attachments: Vec::new(),
            };
            let unstamped = crate::runtime::native_lxmf::client::build_sdk_wire_delivery_from_envelope(
                &unstamped_envelope,
                &local_lxmf_hash_hex,
                local_identity.to_private_key_bytes().as_slice(),
                Some(1),
            )
            .expect("production SDK builder creates unstamped control");
            assert!(unstamped.direct_stamp.is_none());
            send_on_link_observed(
                &transport,
                &link,
                unstamped.wire_bytes.as_slice(),
                |_| {},
                |_| panic!("small unstamped fixture unexpectedly used a Resource"),
            )
            .await
            .expect("send unstamped direct control");

            tokio::time::sleep(Duration::from_millis(150)).await;
            assert_eq!(transport.detach_interfaces().await, 1);
            tokio::time::timeout(Duration::from_secs(1), iface_task)
                .await
                .expect("direct-stamp IFAC task shutdown")
                .expect("direct-stamp IFAC task join");
            (direct_stamp.stamp_value, direct_stamp.attempts, started.elapsed())
        });

        let result = peer.finish();
        assert_eq!(result["passed"], true);
        assert_eq!(result["received_count"], 1);
        assert_eq!(result["stamped_accepted"], true);
        assert_eq!(result["unstamped_rejected"], true);
        eprintln!(
            "direct-stamp admission interoperated: cost=1 value={stamp_value} attempts={stamp_attempts} elapsed_ms={}",
            elapsed.as_millis()
        );
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
