use sha2::{Digest, Sha256};

use crate::{ChatOp, FrameBody, FrameValue, RoomId, PROTOCOL_VERSION};

pub const DURABLE_MUTATION_CAPABILITY: &str = "durable-mutations-v1";
pub const DURABLE_NOTICE_ACK_CAPABILITY: &str = "durable-room-notice-ack-v1";
pub const DURABLE_MUTATION_ENVELOPE_TAG: &str = "durable-mutation-v1";
pub const CLIENT_INSTANCE_ID_BYTES: usize = 16;
pub const MUTATION_ID_BYTES: usize = 16;
pub const REQUEST_HASH_BYTES: usize = 32;

const CANONICAL_DOMAIN: &[u8] = b"omenchat durable mutation v1\0";
const CANONICAL_MAX_BYTES: usize = 1024 * 1024;
const CANONICAL_MAX_SCALAR_BYTES: usize = 512 * 1024;
const CANONICAL_MAX_CONTAINER_ITEMS: usize = 4096;
const CANONICAL_MAX_TOTAL_VALUES: usize = 8192;
const CANONICAL_MAX_DEPTH: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClientInstanceId([u8; CLIENT_INSTANCE_ID_BYTES]);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MutationId([u8; MUTATION_ID_BYTES]);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RequestHash([u8; REQUEST_HASH_BYTES]);

macro_rules! fixed_identifier {
    ($name:ident, $len:ident, $label:literal) => {
        impl $name {
            pub const fn new(bytes: [u8; $len]) -> Self {
                Self(bytes)
            }

            pub const fn as_bytes(&self) -> &[u8; $len] {
                &self.0
            }

            pub const fn into_bytes(self) -> [u8; $len] {
                self.0
            }
        }

        impl TryFrom<&[u8]> for $name {
            type Error = DurableMutationError;

            fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
                let bytes: [u8; $len] =
                    bytes
                        .try_into()
                        .map_err(|_| DurableMutationError::IdentifierLength {
                            field: $label,
                            expected: $len,
                            actual: bytes.len(),
                        })?;
                Ok(Self(bytes))
            }
        }
    };
}

fixed_identifier!(
    ClientInstanceId,
    CLIENT_INSTANCE_ID_BYTES,
    "client_instance_id"
);
fixed_identifier!(MutationId, MUTATION_ID_BYTES, "mutation_id");
fixed_identifier!(RequestHash, REQUEST_HASH_BYTES, "request_hash");

#[derive(Clone, Debug, PartialEq)]
pub struct DurableMutationEnvelope {
    pub mutation_id: MutationId,
    pub request_hash: RequestHash,
    pub body: FrameBody,
}

impl DurableMutationEnvelope {
    pub fn into_frame_body(self) -> Result<FrameBody, DurableMutationError> {
        validate_canonical_body(&self.body)?;
        let (kind, value) = legacy_body_parts(self.body);
        Ok(FrameBody::Fields(vec![
            FrameValue::String(DURABLE_MUTATION_ENVELOPE_TAG.into()),
            FrameValue::Bytes(self.mutation_id.into_bytes().to_vec()),
            FrameValue::Bytes(self.request_hash.into_bytes().to_vec()),
            FrameValue::U64(kind),
            value,
        ]))
    }

    pub fn from_frame_body(body: &FrameBody) -> Result<Self, DurableMutationError> {
        let FrameBody::Fields(fields) = body else {
            return Err(DurableMutationError::Envelope(
                "durable mutation envelope must be a fields body",
            ));
        };
        if fields.len() != 5 {
            return Err(DurableMutationError::Envelope(
                "durable mutation envelope must contain exactly five fields",
            ));
        }
        if !matches!(
            fields.first(),
            Some(FrameValue::String(tag)) if tag == DURABLE_MUTATION_ENVELOPE_TAG
        ) {
            return Err(DurableMutationError::Envelope(
                "durable mutation envelope tag is invalid",
            ));
        }
        let mutation_id = match fields.get(1) {
            Some(FrameValue::Bytes(bytes)) => MutationId::try_from(bytes.as_slice())?,
            _ => {
                return Err(DurableMutationError::Envelope(
                    "durable mutation id must be binary",
                ))
            }
        };
        let request_hash = match fields.get(2) {
            Some(FrameValue::Bytes(bytes)) => RequestHash::try_from(bytes.as_slice())?,
            _ => {
                return Err(DurableMutationError::Envelope(
                    "durable request hash must be binary",
                ))
            }
        };
        let body = match (fields.get(3), fields.get(4)) {
            (Some(FrameValue::U64(0)), Some(FrameValue::Nil)) => FrameBody::Empty,
            (Some(FrameValue::U64(1)), Some(FrameValue::String(text))) => {
                validate_canonical_text(text)?;
                FrameBody::Text(text.clone())
            }
            (Some(FrameValue::U64(2)), Some(FrameValue::Array(values))) => {
                validate_canonical_fields(values)?;
                FrameBody::Fields(values.clone())
            }
            _ => {
                return Err(DurableMutationError::Envelope(
                    "durable mutation legacy body kind and value do not agree",
                ))
            }
        };
        Ok(Self {
            mutation_id,
            request_hash,
            body,
        })
    }
}

fn legacy_body_parts(body: FrameBody) -> (u64, FrameValue) {
    match body {
        FrameBody::Empty => (0, FrameValue::Nil),
        FrameBody::Text(text) => (1, FrameValue::String(text)),
        FrameBody::Fields(values) => (2, FrameValue::Array(values)),
    }
}

pub fn canonical_mutation_request_hash(
    op: ChatOp,
    room_id: Option<RoomId>,
    body: &FrameBody,
) -> Result<RequestHash, DurableMutationError> {
    let mut writer = CanonicalWriter::default();
    writer.bytes(CANONICAL_DOMAIN)?;
    writer.bytes(&[PROTOCOL_VERSION])?;
    writer.bytes(&(op as u16).to_be_bytes())?;
    match room_id {
        Some(room_id) => {
            writer.bytes(&[1])?;
            writer.bytes(&room_id.to_be_bytes())?;
        }
        None => writer.bytes(&[0])?,
    }
    writer.body(body)?;
    Ok(RequestHash::new(writer.finish()))
}

pub fn validate_canonical_body(body: &FrameBody) -> Result<(), DurableMutationError> {
    let mut writer = CanonicalWriter::default();
    writer.body(body)
}

fn validate_canonical_text(text: &str) -> Result<(), DurableMutationError> {
    let mut writer = CanonicalWriter::default();
    writer.text_body(text)
}

fn validate_canonical_fields(values: &[FrameValue]) -> Result<(), DurableMutationError> {
    let mut writer = CanonicalWriter::default();
    writer.fields_body(values)
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DurableMutationError {
    #[error("{field} must contain exactly {expected} bytes; received {actual}")]
    IdentifierLength {
        field: &'static str,
        expected: usize,
        actual: usize,
    },
    #[error("invalid durable mutation envelope: {0}")]
    Envelope(&'static str),
    #[error("durable mutation canonical scalar exceeds {CANONICAL_MAX_SCALAR_BYTES} bytes")]
    ScalarLimit,
    #[error("durable mutation canonical container exceeds {CANONICAL_MAX_CONTAINER_ITEMS} items")]
    ContainerLimit,
    #[error("durable mutation canonical value count exceeds {CANONICAL_MAX_TOTAL_VALUES}")]
    ValueLimit,
    #[error("durable mutation canonical nesting exceeds {CANONICAL_MAX_DEPTH}")]
    DepthLimit,
    #[error("durable mutation canonical encoding exceeds {CANONICAL_MAX_BYTES} bytes")]
    ByteLimit,
}

#[derive(Default)]
struct CanonicalWriter {
    hasher: Sha256,
    bytes: usize,
    values: usize,
}

impl CanonicalWriter {
    fn finish(self) -> [u8; REQUEST_HASH_BYTES] {
        self.hasher.finalize().into()
    }

    fn bytes(&mut self, bytes: &[u8]) -> Result<(), DurableMutationError> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .ok_or(DurableMutationError::ByteLimit)?;
        if self.bytes > CANONICAL_MAX_BYTES {
            return Err(DurableMutationError::ByteLimit);
        }
        self.hasher.update(bytes);
        Ok(())
    }

    fn length(&mut self, value: usize) -> Result<(), DurableMutationError> {
        self.bytes(&(value as u64).to_be_bytes())
    }

    fn scalar(&mut self, tag: u8, value: &[u8]) -> Result<(), DurableMutationError> {
        if value.len() > CANONICAL_MAX_SCALAR_BYTES {
            return Err(DurableMutationError::ScalarLimit);
        }
        self.bytes(&[tag])?;
        self.length(value.len())?;
        self.bytes(value)
    }

    fn body(&mut self, body: &FrameBody) -> Result<(), DurableMutationError> {
        match body {
            FrameBody::Empty => self.bytes(&[0]),
            FrameBody::Text(text) => self.text_body(text),
            FrameBody::Fields(values) => self.fields_body(values),
        }
    }

    fn text_body(&mut self, text: &str) -> Result<(), DurableMutationError> {
        self.scalar(1, text.as_bytes())
    }

    fn fields_body(&mut self, values: &[FrameValue]) -> Result<(), DurableMutationError> {
        self.bytes(&[2])?;
        self.container(values.len())?;
        for value in values {
            self.value(value, 1)?;
        }
        Ok(())
    }

    fn container(&mut self, items: usize) -> Result<(), DurableMutationError> {
        if items > CANONICAL_MAX_CONTAINER_ITEMS {
            return Err(DurableMutationError::ContainerLimit);
        }
        self.length(items)
    }

    fn value(&mut self, value: &FrameValue, depth: usize) -> Result<(), DurableMutationError> {
        if depth > CANONICAL_MAX_DEPTH {
            return Err(DurableMutationError::DepthLimit);
        }
        self.values = self.values.saturating_add(1);
        if self.values > CANONICAL_MAX_TOTAL_VALUES {
            return Err(DurableMutationError::ValueLimit);
        }
        match value {
            FrameValue::Nil => self.bytes(&[0]),
            FrameValue::Bool(value) => self.bytes(&[1, u8::from(*value)]),
            FrameValue::U64(value) => {
                self.bytes(&[2])?;
                self.bytes(&value.to_be_bytes())
            }
            FrameValue::I64(value) => {
                self.bytes(&[3])?;
                self.bytes(&value.to_be_bytes())
            }
            FrameValue::String(value) => self.scalar(4, value.as_bytes()),
            FrameValue::Bytes(value) => self.scalar(5, value),
            FrameValue::Array(values) => {
                self.bytes(&[6])?;
                self.container(values.len())?;
                for value in values {
                    self.value(value, depth + 1)?;
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn fixed_identifiers_require_exact_lengths_without_displaying_bytes() {
        assert_eq!(
            ClientInstanceId::try_from(&[1u8; 15][..]).unwrap_err(),
            DurableMutationError::IdentifierLength {
                field: "client_instance_id",
                expected: 16,
                actual: 15,
            }
        );
        assert!(MutationId::try_from(&[2u8; 16][..]).is_ok());
        assert!(RequestHash::try_from(&[3u8; 32][..]).is_ok());
    }

    #[test]
    fn envelope_round_trips_each_legacy_body_kind_and_rejects_mismatch() {
        for body in [
            FrameBody::Empty,
            FrameBody::Text("hello".into()),
            FrameBody::Fields(vec![FrameValue::U64(7), FrameValue::String("room".into())]),
        ] {
            let envelope = DurableMutationEnvelope {
                mutation_id: MutationId::new([4; 16]),
                request_hash: canonical_mutation_request_hash(ChatOp::RoomMessage, Some(1), &body)
                    .expect("hash"),
                body,
            };
            let wire = envelope.clone().into_frame_body().expect("encode");
            assert_eq!(
                DurableMutationEnvelope::from_frame_body(&wire).expect("decode"),
                envelope
            );
        }

        let malformed = FrameBody::Fields(vec![
            FrameValue::String(DURABLE_MUTATION_ENVELOPE_TAG.into()),
            FrameValue::Bytes(vec![4; 16]),
            FrameValue::Bytes(vec![5; 32]),
            FrameValue::U64(1),
            FrameValue::Nil,
        ]);
        assert!(matches!(
            DurableMutationEnvelope::from_frame_body(&malformed),
            Err(DurableMutationError::Envelope(_))
        ));

        let oversized = DurableMutationEnvelope {
            mutation_id: MutationId::new([6; 16]),
            request_hash: RequestHash::new([7; 32]),
            body: FrameBody::Text("x".repeat(CANONICAL_MAX_SCALAR_BYTES + 1)),
        };
        assert_eq!(
            oversized.into_frame_body(),
            Err(DurableMutationError::ScalarLimit)
        );

        let oversized_wire = FrameBody::Fields(vec![
            FrameValue::String(DURABLE_MUTATION_ENVELOPE_TAG.into()),
            FrameValue::Bytes(vec![6; 16]),
            FrameValue::Bytes(vec![7; 32]),
            FrameValue::U64(1),
            FrameValue::String("x".repeat(CANONICAL_MAX_SCALAR_BYTES + 1)),
        ]);
        assert_eq!(
            DurableMutationEnvelope::from_frame_body(&oversized_wire),
            Err(DurableMutationError::ScalarLimit)
        );
    }

    #[test]
    fn canonical_hash_vector_is_stable_and_content_scoped() {
        let body = FrameBody::Fields(vec![
            FrameValue::U64(100),
            FrameValue::String("hello room".into()),
            FrameValue::Array(vec![
                FrameValue::Bool(true),
                FrameValue::Bytes(vec![0, 1, 2]),
            ]),
        ]);
        let hash = canonical_mutation_request_hash(ChatOp::RoomMessage, Some(42), &body)
            .expect("canonical hash");
        assert_eq!(
            hex(hash.as_bytes()),
            "e5b7c4e3f76fd1ec1b9f0cf500170177535cfb2d0ca78cc1184086a5b1a8f720"
        );

        assert_ne!(
            hash,
            canonical_mutation_request_hash(ChatOp::RoomAction, Some(42), &body).expect("op hash")
        );
        assert_ne!(
            hash,
            canonical_mutation_request_hash(ChatOp::RoomMessage, Some(43), &body)
                .expect("room hash")
        );
        assert_ne!(
            hash,
            canonical_mutation_request_hash(
                ChatOp::RoomMessage,
                Some(42),
                &FrameBody::Text("hello room".into())
            )
            .expect("body hash")
        );
    }

    #[test]
    fn canonical_validation_rejects_scalar_container_value_and_depth_overload() {
        assert_eq!(
            validate_canonical_body(&FrameBody::Text("x".repeat(CANONICAL_MAX_SCALAR_BYTES + 1))),
            Err(DurableMutationError::ScalarLimit)
        );
        assert_eq!(
            validate_canonical_body(&FrameBody::Fields(vec![
                FrameValue::Nil;
                CANONICAL_MAX_CONTAINER_ITEMS + 1
            ])),
            Err(DurableMutationError::ContainerLimit)
        );
        let values = vec![
            FrameValue::Array(vec![FrameValue::Nil, FrameValue::Nil]);
            CANONICAL_MAX_CONTAINER_ITEMS
        ];
        assert_eq!(
            validate_canonical_body(&FrameBody::Fields(values)),
            Err(DurableMutationError::ValueLimit)
        );
        let mut deep = FrameValue::Nil;
        for _ in 0..=CANONICAL_MAX_DEPTH {
            deep = FrameValue::Array(vec![deep]);
        }
        assert_eq!(
            validate_canonical_body(&FrameBody::Fields(vec![deep])),
            Err(DurableMutationError::DepthLimit)
        );
    }
}
