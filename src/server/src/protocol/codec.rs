use std::io::Cursor;

use rmpv::{Value, ValueRef};

use super::{ChatOp, Frame, FrameBody, FrameValue, ProtocolError};

pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
const MAX_SCALAR_BYTES: usize = 512 * 1024;
const MAX_CONTAINER_ITEMS: usize = 4096;
const MAX_TOTAL_VALUES: usize = 8192;
const MAX_NESTING_DEPTH: usize = 16;

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
    if bytes.len() > max_bytes {
        return Err(CodecError::Decode("MessagePack byte limit exceeded".into()));
    }
    let mut scanner = MsgpackScanner {
        bytes,
        offset: 0,
        values: 0,
        max_scalar_bytes,
        max_container_items,
        max_total_values,
        max_nesting_depth,
    };
    scanner.value(0)?;
    if scanner.offset != bytes.len() {
        return Err(CodecError::Decode("trailing MessagePack data".into()));
    }
    Ok(())
}

struct MsgpackScanner<'a> {
    bytes: &'a [u8],
    offset: usize,
    values: usize,
    max_scalar_bytes: usize,
    max_container_items: usize,
    max_total_values: usize,
    max_nesting_depth: usize,
}

impl MsgpackScanner<'_> {
    fn value(&mut self, depth: usize) -> Result<(), CodecError> {
        if depth > self.max_nesting_depth {
            return Err(CodecError::Decode(
                "MessagePack nesting limit exceeded".into(),
            ));
        }
        self.values = self.values.saturating_add(1);
        if self.values > self.max_total_values {
            return Err(CodecError::Decode(
                "MessagePack value limit exceeded".into(),
            ));
        }
        let marker = self.take_u8()?;
        match marker {
            0x00..=0x7f | 0xc0 | 0xc2 | 0xc3 | 0xe0..=0xff => Ok(()),
            0x80..=0x8f => self.container(((marker & 0x0f) as usize) * 2, depth),
            0x90..=0x9f => self.container((marker & 0x0f) as usize, depth),
            0xa0..=0xbf => self.scalar((marker & 0x1f) as usize),
            0xc1 => Err(CodecError::Decode("reserved MessagePack marker".into())),
            0xc4 | 0xd9 => {
                let len = self.read_uint(1)?;
                self.scalar(len)
            }
            0xc5 | 0xda => {
                let len = self.read_uint(2)?;
                self.scalar(len)
            }
            0xc6 | 0xdb => {
                let len = self.read_uint(4)?;
                self.scalar(len)
            }
            0xc7 => {
                let len = self.read_uint(1)?;
                self.scalar(len.saturating_add(1))
            }
            0xc8 => {
                let len = self.read_uint(2)?;
                self.scalar(len.saturating_add(1))
            }
            0xc9 => {
                let len = self.read_uint(4)?;
                self.scalar(len.saturating_add(1))
            }
            0xca => self.skip(4),
            0xcb => self.skip(8),
            0xcc | 0xd0 => self.skip(1),
            0xcd | 0xd1 => self.skip(2),
            0xce | 0xd2 => self.skip(4),
            0xcf | 0xd3 => self.skip(8),
            0xd4 => self.skip(2),
            0xd5 => self.skip(3),
            0xd6 => self.skip(5),
            0xd7 => self.skip(9),
            0xd8 => self.skip(17),
            0xdc => {
                let len = self.read_uint(2)?;
                self.container(len, depth)
            }
            0xdd => {
                let len = self.read_uint(4)?;
                self.container(len, depth)
            }
            0xde => {
                let len = self.read_uint(2)?;
                self.container(len.saturating_mul(2), depth)
            }
            0xdf => {
                let len = self.read_uint(4)?;
                self.container(len.saturating_mul(2), depth)
            }
        }
    }

    fn container(&mut self, items: usize, depth: usize) -> Result<(), CodecError> {
        if items > self.max_container_items {
            return Err(CodecError::Decode(
                "MessagePack container limit exceeded".into(),
            ));
        }
        for _ in 0..items {
            self.value(depth + 1)?;
        }
        Ok(())
    }

    fn scalar(&mut self, len: usize) -> Result<(), CodecError> {
        if len > self.max_scalar_bytes {
            return Err(CodecError::Decode(
                "MessagePack scalar limit exceeded".into(),
            ));
        }
        self.skip(len)
    }

    fn read_uint(&mut self, width: usize) -> Result<usize, CodecError> {
        let bytes = self.take(width)?;
        let mut value = 0u64;
        for byte in bytes {
            value = (value << 8) | u64::from(*byte);
        }
        usize::try_from(value).map_err(|_| CodecError::Decode("MessagePack length overflow".into()))
    }

    fn take_u8(&mut self) -> Result<u8, CodecError> {
        Ok(self.take(1)?[0])
    }
    fn skip(&mut self, len: usize) -> Result<(), CodecError> {
        self.take(len).map(|_| ())
    }
    fn take(&mut self, len: usize) -> Result<&[u8], CodecError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| CodecError::Decode("MessagePack length overflow".into()))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| CodecError::Decode("truncated MessagePack value".into()))?;
        self.offset = end;
        Ok(value)
    }
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
    use crate::protocol::batch::{resource_offer_body, ResourceOffer};
    use crate::protocol::{ChatOp, Frame, FrameBody, FrameValue};

    mod v0_6_0_1 {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fixtures/omenchat/v0_6_0_1_wire.rs"
        ));
    }

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
                        compression: crate::protocol::Compression::Bzip2,
                        uncompressed_len: 4096,
                        compressed_len: 128,
                        purpose: "history".into(),
                    }),
                ),
                v0_6_0_1::HISTORY_RESOURCE_OFFER,
            ),
        ];

        assert_eq!(
            crate::protocol::PROTOCOL_VERSION,
            v0_6_0_1::PROTOCOL_VERSION
        );
        assert_eq!(crate::protocol::PROTOCOL_NAME, v0_6_0_1::PROTOCOL_NAME);
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
