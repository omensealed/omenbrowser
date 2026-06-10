use std::io::Cursor;

use rmpv::Value;

use super::{ChatOp, Frame, FrameBody, FrameValue, ProtocolError};

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
    let value = Value::Array(vec![
        Value::from(frame.version as u64),
        Value::from(frame.op as u16 as u64),
        Value::from(frame.flags as u64),
        Value::from(frame.seq as u64),
        frame
            .room_id
            .map(|room_id| Value::from(room_id as u64))
            .unwrap_or(Value::Nil),
        body_to_value(&frame.body),
    ]);
    let mut out = Vec::new();
    rmpv::encode::write_value(&mut out, &value)
        .map_err(|error| CodecError::Encode(error.to_string()))?;
    Ok(out)
}

pub fn decode_frame(bytes: &[u8]) -> Result<Frame, CodecError> {
    let value = rmpv::decode::read_value(&mut Cursor::new(bytes))
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

fn body_to_value(body: &FrameBody) -> Value {
    match body {
        FrameBody::Empty => Value::Nil,
        FrameBody::Text(value) => Value::from(value.as_str()),
        FrameBody::Fields(values) => {
            Value::Array(values.iter().map(frame_value_to_value).collect())
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
    use crate::protocol::{ChatOp, Frame, FrameBody, FrameValue};

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
}
