use std::io::Cursor;

use rmpv::{Value, ValueRef};

use super::protocol::{ChatOp, Frame, FrameBody, FrameValue, ProtocolError};
use crate::protocol_limits::{
    OMENCHAT_FRAME_MAX_BYTES, OMENCHAT_FRAME_MAX_CONTAINER_ITEMS, OMENCHAT_FRAME_MAX_NESTING_DEPTH,
    OMENCHAT_FRAME_MAX_SCALAR_BYTES, OMENCHAT_FRAME_MAX_TOTAL_VALUES,
};

pub const MAX_FRAME_BYTES: usize = OMENCHAT_FRAME_MAX_BYTES;
const MAX_SCALAR_BYTES: usize = OMENCHAT_FRAME_MAX_SCALAR_BYTES;
const MAX_CONTAINER_ITEMS: usize = OMENCHAT_FRAME_MAX_CONTAINER_ITEMS;
const MAX_TOTAL_VALUES: usize = OMENCHAT_FRAME_MAX_TOTAL_VALUES;
const MAX_NESTING_DEPTH: usize = OMENCHAT_FRAME_MAX_NESTING_DEPTH;

#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("MessagePack encode failed: {0}")]
    Encode(String),
    #[error("MessagePack decode failed: {0}")]
    Decode(String),
}

pub fn encode_frame(frame: &Frame) -> Result<Vec<u8>, CodecError> {
    let value = ValueRef::Array(vec![
        ValueRef::from(frame.version as u64),
        ValueRef::from(frame.op as u16 as u64),
        ValueRef::from(frame.flags as u64),
        ValueRef::from(frame.seq as u64),
        frame
            .room_id
            .map(|room_id| ValueRef::from(room_id as u64))
            .unwrap_or(ValueRef::Nil),
        body_to_value_ref(&frame.body),
    ]);
    let mut out = Vec::new();
    rmpv::encode::write_value_ref(&mut out, &value)
        .map_err(|error| CodecError::Encode(error.to_string()))?;
    Ok(out)
}

pub fn decode_frame(bytes: &[u8]) -> Result<Frame, CodecError> {
    validate_msgpack(bytes)?;
    let mut cursor = Cursor::new(bytes);
    let value = rmpv::decode::read_value(&mut cursor)
        .map_err(|error| CodecError::Decode(error.to_string()))?;
    let values = match value {
        Value::Array(values) if values.len() == 6 => values,
        _ => return Err(ProtocolError::MalformedFrame("expected six-item frame array").into()),
    };
    let version = value_as_u64(&values[0], "version")? as u8;
    let op = ChatOp::try_from(value_as_u64(&values[1], "op")?)?;
    let flags = value_as_u64(&values[2], "flags")? as u16;
    let seq = value_as_u64(&values[3], "seq")? as u32;
    let room_id = match &values[4] {
        Value::Nil => None,
        value => Some(value_as_u64(value, "room_id")? as u32),
    };
    let body = value_to_body(&values[5])?;
    Ok(Frame {
        version,
        op,
        flags,
        seq,
        room_id,
        body,
    })
}

fn validate_msgpack(bytes: &[u8]) -> Result<(), CodecError> {
    validate_msgpack_with_limits(
        bytes,
        MAX_FRAME_BYTES,
        MAX_SCALAR_BYTES,
        MAX_CONTAINER_ITEMS,
        MAX_TOTAL_VALUES,
        MAX_NESTING_DEPTH,
    )
}

pub(crate) fn validate_msgpack_with_limits(
    bytes: &[u8],
    max_bytes: usize,
    max_scalar_bytes: usize,
    max_container_items: usize,
    max_total_values: usize,
    max_nesting_depth: usize,
) -> Result<(), CodecError> {
    crate::msgpack::validate_msgpack_with_limits(
        bytes,
        max_bytes,
        max_scalar_bytes,
        max_container_items,
        max_total_values,
        max_nesting_depth,
    )
    .map_err(|error| CodecError::Decode(error.to_string()))
}

fn body_to_value_ref(body: &FrameBody) -> ValueRef<'_> {
    match body {
        FrameBody::Empty => ValueRef::Nil,
        FrameBody::Text(value) => ValueRef::from(value.as_str()),
        FrameBody::Fields(values) => {
            ValueRef::Array(values.iter().map(frame_value_to_value_ref).collect())
        }
    }
}

fn value_to_body(value: &Value) -> Result<FrameBody, CodecError> {
    match value {
        Value::Nil => Ok(FrameBody::Empty),
        Value::String(value) => Ok(FrameBody::Text(
            value.as_str().unwrap_or_default().to_owned(),
        )),
        Value::Array(values) => Ok(FrameBody::Fields(
            values
                .iter()
                .map(value_to_frame_value)
                .collect::<Result<Vec<_>, _>>()?,
        )),
        _ => Err(ProtocolError::MalformedFrame("unsupported frame body shape").into()),
    }
}

fn frame_value_to_value_ref(value: &FrameValue) -> ValueRef<'_> {
    match value {
        FrameValue::Nil => ValueRef::Nil,
        FrameValue::Bool(value) => ValueRef::Boolean(*value),
        FrameValue::U64(value) => ValueRef::from(*value),
        FrameValue::I64(value) => ValueRef::from(*value),
        FrameValue::String(value) => ValueRef::from(value.as_str()),
        FrameValue::Bytes(value) => ValueRef::Binary(value),
        FrameValue::Array(values) => {
            ValueRef::Array(values.iter().map(frame_value_to_value_ref).collect())
        }
    }
}

fn value_to_frame_value(value: &Value) -> Result<FrameValue, CodecError> {
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

fn value_as_u64(value: &Value, field: &'static str) -> Result<u64, CodecError> {
    value
        .as_u64()
        .ok_or(ProtocolError::MalformedFrame(field).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::protocol::batch::{resource_offer_body, ResourceOffer};
    use crate::chat::protocol::{ChatOp, Frame, FrameBody, FrameValue};

    use omenchat_protocol::fixtures::{reply_mentions_v1, v0_6_0_1};
    use omenchat_protocol::{ReplyReference, RichMessageBody};

    #[test]
    fn v0_6_0_1_frame_fixtures_remain_bidirectionally_exact() {
        let fixtures = [
            (
                Frame::new(ChatOp::SessionOpen, 1, None, FrameBody::Empty),
                v0_6_0_1::SESSION_OPEN,
            ),
            (
                Frame::new(
                    ChatOp::RoomMessage,
                    7,
                    Some(42),
                    FrameBody::Fields(vec![
                        FrameValue::U64(100),
                        FrameValue::String("hello room".into()),
                    ]),
                ),
                v0_6_0_1::ROOM_MESSAGE,
            ),
            (
                Frame::new(
                    ChatOp::HistoryResourceOffer,
                    11,
                    Some(7),
                    resource_offer_body(&ResourceOffer {
                        resource_id: "history:7:fixture".into(),
                        compression: crate::chat::protocol::Compression::Bzip2,
                        uncompressed_len: 4096,
                        compressed_len: 128,
                        purpose: "history".into(),
                    }),
                ),
                v0_6_0_1::HISTORY_RESOURCE_OFFER,
            ),
        ];

        assert_eq!(
            crate::chat::protocol::PROTOCOL_VERSION,
            v0_6_0_1::PROTOCOL_VERSION
        );
        assert_eq!(
            crate::chat::protocol::PROTOCOL_NAME,
            v0_6_0_1::PROTOCOL_NAME
        );
        assert_eq!(v0_6_0_1::LINK_CONTEXT, 0x4f);
        assert_eq!(v0_6_0_1::RESOURCE_METADATA_PREFIX, b"omenchat-resource:");
        for (frame, expected) in fixtures {
            assert_eq!(
                encode_frame(&frame).expect("encode current frame"),
                expected
            );
            assert_eq!(decode_frame(expected).expect("decode v0.6 fixture"), frame);
        }
    }

    #[test]
    fn round_trips_compact_frame() {
        let frame = Frame::new(
            ChatOp::RoomMessage,
            7,
            Some(42),
            FrameBody::Fields(vec![
                FrameValue::U64(100),
                FrameValue::String("hello room".into()),
            ]),
        );

        let encoded = encode_frame(&frame).expect("encode frame");
        let decoded = decode_frame(&encoded).expect("decode frame");

        assert_eq!(decoded, frame);
    }

    #[test]
    fn reply_mentions_v1_fixture_is_bidirectionally_exact_but_inert() {
        let frame = Frame::new(
            ChatOp::RoomMessage,
            7,
            Some(7),
            RichMessageBody {
                body: "hello".into(),
                reply_to: Some(ReplyReference {
                    room_id: 7,
                    event_id: 42,
                }),
                mentioned_user_ids: vec![2, 9],
            }
            .into_frame_body()
            .expect("bounded rich message"),
        );

        assert_eq!(
            encode_frame(&frame).expect("encode rich message"),
            reply_mentions_v1::ROOM_MESSAGE
        );
        assert_eq!(
            decode_frame(reply_mentions_v1::ROOM_MESSAGE).expect("decode rich message"),
            frame
        );
    }

    #[test]
    fn borrowed_encoder_preserves_binary_wire_bytes() {
        let frame = Frame::new(
            ChatOp::RoomMessage,
            9,
            Some(7),
            FrameBody::Fields(vec![FrameValue::Array(vec![
                FrameValue::Bytes(vec![0x00, 0x7f, 0x80, 0xff]),
                FrameValue::String("wire".into()),
            ])]),
        );
        let owned = Value::Array(vec![
            Value::from(frame.version as u64),
            Value::from(frame.op as u16 as u64),
            Value::from(frame.flags as u64),
            Value::from(frame.seq as u64),
            Value::from(7_u64),
            Value::Array(vec![Value::Array(vec![
                Value::Binary(vec![0x00, 0x7f, 0x80, 0xff]),
                Value::from("wire"),
            ])]),
        ]);
        let mut expected = Vec::new();
        rmpv::encode::write_value(&mut expected, &owned).expect("owned reference encode");

        assert_eq!(encode_frame(&frame).expect("borrowed encode"), expected);
    }

    #[test]
    fn rejects_trailing_data_before_decode() {
        let mut encoded =
            encode_frame(&Frame::new(ChatOp::Ping, 1, None, FrameBody::Empty)).unwrap();
        encoded.push(0xc0);
        assert!(decode_frame(&encoded)
            .unwrap_err()
            .to_string()
            .contains("trailing"));
    }

    #[test]
    fn rejects_oversized_scalar_and_deep_nesting_before_allocation() {
        let oversized = [0xdb, 0x00, 0x08, 0x00, 0x01];
        assert!(decode_frame(&oversized)
            .unwrap_err()
            .to_string()
            .contains("scalar limit"));
        let mut deep = vec![0x91; MAX_NESTING_DEPTH + 2];
        deep.push(0xc0);
        assert!(decode_frame(&deep)
            .unwrap_err()
            .to_string()
            .contains("nesting limit"));
    }

    #[test]
    fn rejects_frame_container_and_total_value_limits() {
        assert!(decode_frame(&vec![0xc0; MAX_FRAME_BYTES + 1])
            .unwrap_err()
            .to_string()
            .contains("byte limit"));
        let oversized_container = [0xdd, 0x00, 0x00, 0x10, 0x01];
        assert!(decode_frame(&oversized_container)
            .unwrap_err()
            .to_string()
            .contains("container limit"));
        let mut too_many_values = vec![0x92, 0xdc, 0x10, 0x00];
        too_many_values.extend(std::iter::repeat_n(0xc0, MAX_CONTAINER_ITEMS));
        too_many_values.extend([0xdc, 0x10, 0x00]);
        too_many_values.extend(std::iter::repeat_n(0xc0, MAX_CONTAINER_ITEMS));
        assert!(decode_frame(&too_many_values)
            .unwrap_err()
            .to_string()
            .contains("value limit"));
    }

    #[test]
    fn accepts_current_scalar_compatibility_boundary() {
        let frame = Frame::new(
            ChatOp::RoomMessage,
            2,
            Some(1),
            FrameBody::Text("x".repeat(MAX_SCALAR_BYTES)),
        );
        assert_eq!(decode_frame(&encode_frame(&frame).unwrap()).unwrap(), frame);
    }
}
