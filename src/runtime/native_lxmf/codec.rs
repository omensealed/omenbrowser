use std::collections::BTreeMap;
use std::path::Path;

#[cfg(feature = "native-rns-net")]
use rand_core::RngCore;
use reticulum_rs::core::identity::PrivateIdentity;
use reticulum_rs::core::ratchets::{decrypt_with_identity, encrypt_for_public_key};
#[cfg(feature = "native-rns-net")]
use sha2::{Digest, Sha256};
#[cfg(feature = "native-rns-net")]
use x25519_dalek::PublicKey;

use crate::error::{AppError, AppResult};
use crate::messaging::{
    AttachmentSummary, DeliveryMode, MessageEnvelope, MessageSummary, NativeLxmfReplyTicket,
    TransportMethod as AppTransportMethod,
};

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
#[cfg(feature = "native-rns-net")]
const LXMF_TICKET_LENGTH: usize = 16;
#[cfg(feature = "native-rns-net")]
const LXMF_TICKET_EXPIRY_SECONDS: f64 = 21.0 * 24.0 * 60.0 * 60.0;
#[cfg(feature = "native-rns-net")]
const LXMF_STAMP_SIZE: usize = 32;
#[cfg(feature = "native-rns-net")]
const PROPAGATION_LXMF_OVERHEAD: usize = 112;
#[cfg(feature = "native-rns-net")]
const DIRECT_WORKBLOCK_EXPAND_ROUNDS: u32 = 3000;
#[cfg(feature = "native-rns-net")]
const PROPAGATION_WORKBLOCK_EXPAND_ROUNDS: u32 = 1000;
#[cfg(feature = "native-rns-net")]
pub const DEFAULT_PROPAGATION_STAMP_MAX_ATTEMPTS: u64 = 1 << 22;
#[cfg(feature = "native-rns-net")]
pub const DEFAULT_DIRECT_STAMP_MAX_ATTEMPTS: u64 = 1 << 22;

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
        stamp_validation_supported: cfg!(feature = "native-rns-net"),
        stamp_generation_supported: cfg!(feature = "native-rns-net"),
        include_ticket_supported: cfg!(feature = "native-rns-net"),
    }
}

pub fn native_delivery_type_name() -> &'static str {
    "lxmf::Message"
}

pub fn delivery_display_name_from_app_data(app_data: &[u8]) -> Option<String> {
    lxmf::wire::announce::display_name_from_delivery_app_data(app_data)
}

pub fn delivery_announce_stamp_cost(app_data: &[u8]) -> Option<u8> {
    let mut cursor = std::io::Cursor::new(app_data);
    let value = rmpv::decode::read_value(&mut cursor).ok()?;
    if cursor.position() != app_data.len() as u64 {
        return None;
    }
    let rmpv::Value::Array(items) = value else {
        return None;
    };
    let cost = items.get(1).and_then(value_as_u64)?;
    if cost == 0 || cost >= 255 {
        return None;
    }
    u8::try_from(cost).ok()
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
    let timestamp = wire.payload.timestamp;
    let (lxm_data, transient_id) =
        pack_destination_salted_propagation_transient(&wire, recipient_public_key)?;
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
fn pack_destination_salted_propagation_transient(
    wire: &lxmf::WireMessage,
    recipient_public_key: [u8; 64],
) -> AppResult<(Vec<u8>, [u8; 32])> {
    let packed = wire.pack().map_err(|err| {
        AppError::Runtime(format!("LXMF wire pack before propagation failed: {err}"))
    })?;
    if packed.len() <= 16 {
        return Err(AppError::Runtime(
            "LXMF wire payload is shorter than destination hash".into(),
        ));
    }
    let mut public_key = [0u8; 32];
    public_key.copy_from_slice(&recipient_public_key[..32]);
    let encrypted = encrypt_for_public_key(
        &PublicKey::from(public_key),
        &packed[..16],
        &packed[16..],
        rand_core::OsRng,
    )
    .map_err(|_| AppError::Runtime("LXMF propagation transient encrypt failed".into()))?;

    let mut lxm_data = Vec::with_capacity(16 + encrypted.len());
    lxm_data.extend_from_slice(&packed[..16]);
    lxm_data.extend_from_slice(&encrypted);
    let digest = Sha256::digest(&lxm_data);
    let mut transient_id = [0u8; 32];
    transient_id.copy_from_slice(digest.as_slice());
    Ok((lxm_data, transient_id))
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
    let mut stamp = vec![0u8; LXMF_STAMP_SIZE];
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
    if transient_data.len() <= PROPAGATION_LXMF_OVERHEAD + LXMF_STAMP_SIZE {
        return None;
    }
    let split = transient_data.len() - LXMF_STAMP_SIZE;
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
    #[cfg(not(feature = "native-rns-net"))]
    if envelope.include_ticket {
        return Err(AppError::Unsupported(
            "native LXMF include-ticket sending requires the native-rns-net feature".into(),
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
    let (mut fields, attachments) = attachment_fields_from_paths(&envelope.attachments)?;
    #[cfg(feature = "native-rns-net")]
    if envelope.include_ticket {
        insert_lxmf_field(&mut fields, FIELD_TICKET, generate_lxmf_ticket_field());
    }
    message.fields = fields;
    let mut reply_ticket_used = false;
    #[cfg(feature = "native-rns-net")]
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

#[cfg(feature = "native-rns-net")]
fn apply_reply_ticket_stamp(
    message: &mut lxmf::Message,
    ticket: &NativeLxmfReplyTicket,
) -> AppResult<()> {
    let message_id = lxmf_message_id(message)?;
    let stamp = ticket_stamp_for_message(&ticket.ticket, &message_id)?;
    message.set_stamp_from_bytes(&stamp);
    Ok(())
}

#[cfg(feature = "native-rns-net")]
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

#[cfg(feature = "native-rns-net")]
pub fn generate_direct_stamp_for_message(
    message_id: [u8; 32],
    target_cost: u8,
    max_attempts: u64,
) -> AppResult<GeneratedDirectStamp> {
    let workblock = rns_core::stamp::stamp_workblock(&message_id, DIRECT_WORKBLOCK_EXPAND_ROUNDS);
    let mut stamp = vec![0u8; LXMF_STAMP_SIZE];
    for attempt in 1..=max_attempts {
        rand_core::OsRng.fill_bytes(&mut stamp);
        if rns_core::stamp::stamp_valid(&stamp, target_cost, &workblock) {
            let stamp_value = rns_core::stamp::stamp_value(&workblock, &stamp);
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

#[cfg(feature = "native-rns-net")]
pub fn validate_direct_stamp(message_id: &[u8; 32], stamp: &[u8], target_cost: u8) -> Option<u32> {
    let workblock = rns_core::stamp::stamp_workblock(message_id, DIRECT_WORKBLOCK_EXPAND_ROUNDS);
    if rns_core::stamp::stamp_valid(stamp, target_cost, &workblock) {
        Some(rns_core::stamp::stamp_value(&workblock, stamp))
    } else {
        None
    }
}

#[cfg(feature = "native-rns-net")]
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
    Ok(wire.message_id())
}

#[cfg(feature = "native-rns-net")]
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

#[cfg(feature = "native-rns-net")]
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
    let fields = native_lxmf_summary_fields_from_message_fields(message.fields.as_ref());

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

fn native_lxmf_summary_fields_from_message_fields(
    message_fields: Option<&rmpv::Value>,
) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    annotate_lxmf_field_presence(&mut fields, message_fields);
    #[cfg(feature = "native-rns-net")]
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

#[cfg(feature = "native-rns-net")]
pub fn lxmf_delivery_destination_hash_from_private_identity_bytes(
    private_identity_bytes: &[u8],
) -> AppResult<[u8; 16]> {
    let identity = PrivateIdentity::from_private_key_bytes(private_identity_bytes)
        .map_err(|_| AppError::Runtime("native Reticulum identity is invalid".into()))?;
    let mut identity_hash = [0u8; 16];
    identity_hash.copy_from_slice(identity.address_hash().as_slice());
    Ok(rns_core::destination::destination_hash(
        "lxmf",
        &["delivery"],
        Some(&identity_hash),
    ))
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

#[cfg(feature = "native-rns-net")]
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

#[cfg(feature = "native-rns-net")]
fn generate_lxmf_ticket_field() -> rmpv::Value {
    let mut ticket = vec![0u8; LXMF_TICKET_LENGTH];
    rand_core::OsRng.fill_bytes(&mut ticket);
    let expires = current_unix_secs_f64() + LXMF_TICKET_EXPIRY_SECONDS;
    rmpv::Value::Array(vec![rmpv::Value::F64(expires), rmpv::Value::Binary(ticket)])
}

#[cfg(feature = "native-rns-net")]
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

fn hex_bytes(bytes: &[u8]) -> String {
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

    #[cfg(feature = "native-rns-net")]
    fn lxmf_delivery_hash(identity: &PrivateIdentity) -> [u8; 16] {
        let mut identity_hash = [0u8; 16];
        identity_hash.copy_from_slice(identity.address_hash().as_slice());
        rns_core::destination::destination_hash("lxmf", &["delivery"], Some(&identity_hash))
    }

    #[cfg(feature = "native-rns-net")]
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
        assert_eq!(
            parity.include_ticket_supported,
            cfg!(feature = "native-rns-net")
        );
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
            outbound.message.stamp.as_ref().map(Vec::as_slice),
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
            attachments: Vec::new(),
        };

        let outbound = build_outbound_message(&envelope, SRC).expect("outbound");

        assert_eq!(outbound.delivery.method, lxmf::TransportMethod::Propagated);
        assert!(!outbound.include_ticket);
    }

    #[test]
    #[cfg(feature = "native-rns-net")]
    fn include_ticket_send_adds_lxmf_ticket_field() {
        let envelope = MessageEnvelope {
            peer_hash: DEST.into(),
            title: "Subject".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: true,
            native_reply_ticket: None,
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
    #[cfg(feature = "native-rns-net")]
    fn signed_ticketed_wire_message_decodes_reply_ticket_metadata() {
        let provider = NativeReticulumIdentityProvider;
        let private = provider
            .create_identity_material("sender")
            .expect("native identity");
        let signer = PrivateIdentity::from_private_key_bytes(&private).expect("signer");
        let source = signer.address_hash().to_hex_string();
        let envelope = MessageEnvelope {
            peer_hash: DEST.into(),
            title: "Ticketed".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: true,
            native_reply_ticket: None,
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
            .get("native_lxmf_reply_ticket_expires")
            .is_some());
    }

    #[test]
    #[cfg(not(feature = "native-rns-net"))]
    fn include_ticket_requires_native_rns_net_feature() {
        let envelope = MessageEnvelope {
            peer_hash: DEST.into(),
            title: "Subject".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: true,
            native_reply_ticket: None,
            attachments: Vec::new(),
        };

        let error = build_outbound_message(&envelope, SRC).expect_err("ticket requires native");

        assert!(error.to_string().contains("native-rns-net"));
    }

    #[test]
    #[cfg(feature = "native-rns-net")]
    fn ticket_stamp_uses_ticket_and_message_id_truncated_hash() {
        let ticket = [0x11u8; LXMF_TICKET_LENGTH];
        let message_id = [0x22u8; 32];

        let stamp = ticket_stamp_for_message(&ticket, &message_id).expect("ticket stamp");
        let repeat = ticket_stamp_for_message(&ticket, &message_id).expect("repeat stamp");

        assert_eq!(stamp.len(), 16);
        assert_eq!(stamp, repeat);
    }

    #[test]
    #[cfg(feature = "native-rns-net")]
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
            native_reply_ticket: None,
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
            pack_destination_salted_propagation_transient(&wire, receiver_public)
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

    #[cfg(feature = "native-rns-net")]
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
    fn invalid_destination_is_rejected_before_wire_encoding() {
        let envelope = MessageEnvelope {
            peer_hash: "not-a-hash".into(),
            title: "Subject".into(),
            body: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: None,
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
