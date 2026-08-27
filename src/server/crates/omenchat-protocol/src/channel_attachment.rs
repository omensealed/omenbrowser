use thiserror::Error;

pub const CHANNEL_ATTACHMENT_CAPABILITY: &str = "omenchat-channel-attachments-v1";
pub const CHANNEL_ATTACHMENT_MESSAGE_TYPE: u16 = 0x4f41;
pub const CHANNEL_ATTACHMENT_MAX_RESOURCE_ID_BYTES: usize = 128;
const MAGIC: &[u8; 4] = b"OCAT";
const VERSION: u8 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChannelAttachmentFrame {
    Start {
        resource_id: String,
        total_bytes: u64,
    },
    Data {
        resource_id: String,
        offset: u64,
        bytes: Vec<u8>,
    },
    Finish {
        resource_id: String,
        total_bytes: u64,
        digest: [u8; 32],
    },
    Cancel {
        resource_id: String,
    },
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ChannelAttachmentError {
    #[error("invalid channel attachment frame")]
    Invalid,
    #[error("channel attachment resource id is invalid")]
    InvalidResourceId,
    #[error("channel attachment frame exceeds its MDU")]
    PayloadTooLarge,
}

impl ChannelAttachmentFrame {
    pub fn encode(&self, max_payload: usize) -> Result<Vec<u8>, ChannelAttachmentError> {
        let (kind, resource_id) = match self {
            Self::Start { resource_id, .. } => (1, resource_id),
            Self::Data { resource_id, .. } => (2, resource_id),
            Self::Finish { resource_id, .. } => (3, resource_id),
            Self::Cancel { resource_id } => (4, resource_id),
        };
        let id = resource_id.as_bytes();
        if id.is_empty() || id.len() > CHANNEL_ATTACHMENT_MAX_RESOURCE_ID_BYTES {
            return Err(ChannelAttachmentError::InvalidResourceId);
        }
        let id_len =
            u16::try_from(id.len()).map_err(|_| ChannelAttachmentError::InvalidResourceId)?;
        let mut out = Vec::with_capacity(max_payload.min(256));
        out.extend_from_slice(MAGIC);
        out.push(VERSION);
        out.push(kind);
        out.extend_from_slice(&id_len.to_be_bytes());
        match self {
            Self::Start { total_bytes, .. } => out.extend_from_slice(&total_bytes.to_be_bytes()),
            Self::Data { offset, .. } => out.extend_from_slice(&offset.to_be_bytes()),
            Self::Finish {
                total_bytes,
                digest,
                ..
            } => {
                out.extend_from_slice(&total_bytes.to_be_bytes());
                out.extend_from_slice(digest);
            }
            Self::Cancel { .. } => {}
        }
        out.extend_from_slice(id);
        if let Self::Data { bytes, .. } = self {
            out.extend_from_slice(bytes);
        }
        if out.len() > max_payload {
            return Err(ChannelAttachmentError::PayloadTooLarge);
        }
        Ok(out)
    }

    pub fn decode(raw: &[u8]) -> Result<Self, ChannelAttachmentError> {
        if raw.len() < 8 || &raw[..4] != MAGIC || raw[4] != VERSION {
            return Err(ChannelAttachmentError::Invalid);
        }
        let kind = raw[5];
        let id_len = u16::from_be_bytes([raw[6], raw[7]]) as usize;
        if id_len == 0 || id_len > CHANNEL_ATTACHMENT_MAX_RESOURCE_ID_BYTES {
            return Err(ChannelAttachmentError::InvalidResourceId);
        }
        let fixed: usize = match kind {
            1 | 2 => 16,
            3 => 48,
            4 => 8,
            _ => return Err(ChannelAttachmentError::Invalid),
        };
        let id_end = fixed
            .checked_add(id_len)
            .ok_or(ChannelAttachmentError::Invalid)?;
        if raw.len() < id_end || (kind != 2 && raw.len() != id_end) {
            return Err(ChannelAttachmentError::Invalid);
        }
        let resource_id = std::str::from_utf8(&raw[fixed..id_end])
            .map_err(|_| ChannelAttachmentError::InvalidResourceId)?
            .to_owned();
        match kind {
            1 => Ok(Self::Start {
                resource_id,
                total_bytes: read_u64(&raw[8..16])?,
            }),
            2 => Ok(Self::Data {
                resource_id,
                offset: read_u64(&raw[8..16])?,
                bytes: raw[id_end..].to_vec(),
            }),
            3 => {
                let mut digest = [0u8; 32];
                digest.copy_from_slice(&raw[16..48]);
                Ok(Self::Finish {
                    resource_id,
                    total_bytes: read_u64(&raw[8..16])?,
                    digest,
                })
            }
            4 => Ok(Self::Cancel { resource_id }),
            _ => Err(ChannelAttachmentError::Invalid),
        }
    }

    pub fn data_header_len(resource_id: &str) -> Result<usize, ChannelAttachmentError> {
        if resource_id.is_empty() || resource_id.len() > CHANNEL_ATTACHMENT_MAX_RESOURCE_ID_BYTES {
            return Err(ChannelAttachmentError::InvalidResourceId);
        }
        Ok(16 + resource_id.len())
    }
}

fn read_u64(raw: &[u8]) -> Result<u64, ChannelAttachmentError> {
    let bytes: [u8; 8] = raw
        .try_into()
        .map_err(|_| ChannelAttachmentError::Invalid)?;
    Ok(u64::from_be_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_roundtrip_and_obey_payload_boundary() {
        let frame = ChannelAttachmentFrame::Data {
            resource_id: "upload:1:2:3".into(),
            offset: 17,
            bytes: vec![0x5a; 32],
        };
        let encoded = frame.encode(128).expect("bounded frame");
        assert_eq!(ChannelAttachmentFrame::decode(&encoded), Ok(frame));
        assert_eq!(
            ChannelAttachmentFrame::Data {
                resource_id: "upload:1:2:3".into(),
                offset: 0,
                bytes: vec![0; 128],
            }
            .encode(128),
            Err(ChannelAttachmentError::PayloadTooLarge)
        );
    }
}
