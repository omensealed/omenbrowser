use std::io::{Cursor, Read, Write};

use bzip2::read::BzDecoder;
use bzip2::write::BzEncoder;
use bzip2::Compression as Bzip2Compression;
use rmpv::Value;

use super::{Compression, FrameBody, FrameValue, ProtocolError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceOffer {
    pub resource_id: String,
    pub compression: Compression,
    pub uncompressed_len: u64,
    pub compressed_len: u64,
    pub purpose: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompressedBatch {
    pub compression: Compression,
    pub uncompressed_len: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum BatchError {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("batch encode failed: {0}")]
    Encode(String),
    #[error("batch decode failed: {0}")]
    Decode(String),
    #[error("compression failed: {0}")]
    Compression(std::io::Error),
}

pub fn compressed_values_body(values: &[FrameValue]) -> Result<FrameBody, BatchError> {
    let batch = compressed_values_batch(values)?;
    Ok(FrameBody::Fields(vec![
        FrameValue::U64(batch.compression as u8 as u64),
        FrameValue::U64(batch.uncompressed_len),
        FrameValue::Bytes(batch.bytes),
    ]))
}

pub fn compressed_values_payload(values: &[FrameValue]) -> Result<Vec<u8>, BatchError> {
    let body = compressed_values_body(values)?;
    let FrameBody::Fields(fields) = body else {
        return Err(ProtocolError::MalformedFrame("expected compressed batch fields").into());
    };
    encode_values(&fields)
}

pub fn decode_compressed_values_payload(bytes: &[u8]) -> Result<Vec<FrameValue>, BatchError> {
    decode_compressed_values_body(&FrameBody::Fields(decode_values(bytes)?))
}

pub fn compressed_values_batch(values: &[FrameValue]) -> Result<CompressedBatch, BatchError> {
    let raw = encode_values(values)?;
    let compressed = compress_bzip2(&raw)?;
    Ok(CompressedBatch {
        compression: Compression::Bzip2,
        uncompressed_len: raw.len() as u64,
        bytes: compressed,
    })
}

pub fn decode_compressed_values_body(body: &FrameBody) -> Result<Vec<FrameValue>, BatchError> {
    let FrameBody::Fields(fields) = body else {
        return Err(ProtocolError::MalformedFrame("expected compressed batch fields").into());
    };
    if fields.len() != 3 {
        return Err(ProtocolError::MalformedFrame("expected three compressed batch fields").into());
    }
    let compression = field_as_u64(&fields[0], "compression")?;
    let uncompressed_len = field_as_u64(&fields[1], "uncompressed_len")?;
    let payload = field_as_bytes(&fields[2], "payload")?;
    let decoded = match Compression::try_from(compression)? {
        Compression::None => payload.to_vec(),
        Compression::Bzip2 => decompress_bzip2(payload)?,
    };
    if decoded.len() as u64 != uncompressed_len {
        return Err(ProtocolError::MalformedFrame("batch length mismatch").into());
    }
    decode_values(&decoded)
}

pub fn resource_offer_body(offer: &ResourceOffer) -> FrameBody {
    FrameBody::Fields(vec![
        FrameValue::String(offer.resource_id.clone()),
        FrameValue::U64(offer.compression as u8 as u64),
        FrameValue::U64(offer.uncompressed_len),
        FrameValue::U64(offer.compressed_len),
        FrameValue::String(offer.purpose.clone()),
    ])
}

pub fn decode_resource_offer_body(body: &FrameBody) -> Result<ResourceOffer, BatchError> {
    let FrameBody::Fields(fields) = body else {
        return Err(ProtocolError::MalformedFrame("expected resource offer fields").into());
    };
    if fields.len() != 5 {
        return Err(ProtocolError::MalformedFrame("expected five resource offer fields").into());
    }
    Ok(ResourceOffer {
        resource_id: field_as_string(&fields[0], "resource_id")?.to_owned(),
        compression: Compression::try_from(field_as_u64(&fields[1], "compression")?)?,
        uncompressed_len: field_as_u64(&fields[2], "uncompressed_len")?,
        compressed_len: field_as_u64(&fields[3], "compressed_len")?,
        purpose: field_as_string(&fields[4], "purpose")?.to_owned(),
    })
}

pub fn encoded_compressed_len(values: &[FrameValue]) -> Result<(usize, usize), BatchError> {
    let batch = compressed_values_batch(values)?;
    Ok((batch.uncompressed_len as usize, batch.bytes.len()))
}

pub fn encode_values(values: &[FrameValue]) -> Result<Vec<u8>, BatchError> {
    let value = Value::Array(values.iter().map(frame_value_to_value).collect());
    let mut out = Vec::new();
    rmpv::encode::write_value(&mut out, &value)
        .map_err(|error| BatchError::Encode(error.to_string()))?;
    Ok(out)
}

pub fn decode_values(bytes: &[u8]) -> Result<Vec<FrameValue>, BatchError> {
    let value = rmpv::decode::read_value(&mut Cursor::new(bytes))
        .map_err(|error| BatchError::Decode(error.to_string()))?;
    let Value::Array(values) = value else {
        return Err(ProtocolError::MalformedFrame("expected batch array").into());
    };
    values.iter().map(value_to_frame_value).collect()
}

fn compress_bzip2(bytes: &[u8]) -> Result<Vec<u8>, BatchError> {
    let mut encoder = BzEncoder::new(Vec::new(), Bzip2Compression::best());
    encoder.write_all(bytes).map_err(BatchError::Compression)?;
    encoder.finish().map_err(BatchError::Compression)
}

fn decompress_bzip2(bytes: &[u8]) -> Result<Vec<u8>, BatchError> {
    let mut decoder = BzDecoder::new(bytes);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(BatchError::Compression)?;
    Ok(out)
}

fn field_as_u64(value: &FrameValue, field: &'static str) -> Result<u64, ProtocolError> {
    match value {
        FrameValue::U64(value) => Ok(*value),
        FrameValue::I64(value) if *value >= 0 => Ok(*value as u64),
        _ => Err(ProtocolError::MalformedFrame(field)),
    }
}

fn field_as_bytes<'a>(
    value: &'a FrameValue,
    field: &'static str,
) -> Result<&'a [u8], ProtocolError> {
    match value {
        FrameValue::Bytes(value) => Ok(value),
        _ => Err(ProtocolError::MalformedFrame(field)),
    }
}

fn field_as_string<'a>(
    value: &'a FrameValue,
    field: &'static str,
) -> Result<&'a str, ProtocolError> {
    match value {
        FrameValue::String(value) => Ok(value),
        _ => Err(ProtocolError::MalformedFrame(field)),
    }
}

fn frame_value_to_value(value: &FrameValue) -> Value {
    match value {
        FrameValue::Nil => Value::Nil,
        FrameValue::Bool(value) => Value::Boolean(*value),
        FrameValue::U64(value) => Value::from(*value),
        FrameValue::I64(value) => Value::from(*value),
        FrameValue::String(value) => Value::from(value.as_str()),
        FrameValue::Bytes(value) => Value::Binary(value.clone()),
        FrameValue::Array(values) => {
            Value::Array(values.iter().map(frame_value_to_value).collect())
        }
    }
}

fn value_to_frame_value(value: &Value) -> Result<FrameValue, BatchError> {
    match value {
        Value::Nil => Ok(FrameValue::Nil),
        Value::Boolean(value) => Ok(FrameValue::Bool(*value)),
        Value::Integer(value) => {
            if let Some(value) = value.as_u64() {
                Ok(FrameValue::U64(value))
            } else if let Some(value) = value.as_i64() {
                Ok(FrameValue::I64(value))
            } else {
                Err(ProtocolError::MalformedFrame("integer does not fit supported range").into())
            }
        }
        Value::String(value) => Ok(FrameValue::String(
            value.as_str().unwrap_or_default().to_owned(),
        )),
        Value::Binary(value) => Ok(FrameValue::Bytes(value.clone())),
        Value::Array(values) => Ok(FrameValue::Array(
            values
                .iter()
                .map(value_to_frame_value)
                .collect::<Result<Vec<_>, _>>()?,
        )),
        _ => Err(ProtocolError::MalformedFrame("unsupported nested value").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compressed_batch_round_trips_values() {
        let values = vec![
            FrameValue::Array(vec![
                FrameValue::U64(1),
                FrameValue::String("hello room".into()),
            ]),
            FrameValue::Array(vec![
                FrameValue::U64(2),
                FrameValue::String("older history".repeat(20)),
            ]),
        ];

        let body = compressed_values_body(&values).expect("compress");
        let decoded = decode_compressed_values_body(&body).expect("decode");

        assert_eq!(decoded, values);
    }

    #[test]
    fn compressed_resource_payload_round_trips_values() {
        let values = vec![FrameValue::Array(vec![
            FrameValue::U64(7),
            FrameValue::String("resource payload".repeat(16)),
        ])];

        let payload = compressed_values_payload(&values).expect("payload");
        let decoded = decode_compressed_values_payload(&payload).expect("decode payload");

        assert_eq!(decoded, values);
    }

    #[test]
    fn resource_offer_round_trips_fields() {
        let offer = ResourceOffer {
            resource_id: "history:1:50".into(),
            compression: Compression::Bzip2,
            uncompressed_len: 100,
            compressed_len: 42,
            purpose: "history".into(),
        };

        let decoded = decode_resource_offer_body(&resource_offer_body(&offer)).expect("decode");

        assert_eq!(decoded, offer);
    }
}
