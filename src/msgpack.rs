use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MsgpackLimitError(&'static str);

impl fmt::Display for MsgpackLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for MsgpackLimitError {}

pub(crate) fn validate_msgpack_with_limits(
    bytes: &[u8],
    max_bytes: usize,
    max_scalar_bytes: usize,
    max_container_items: usize,
    max_total_values: usize,
    max_nesting_depth: usize,
) -> Result<(), MsgpackLimitError> {
    if bytes.len() > max_bytes {
        return Err(MsgpackLimitError("MessagePack byte limit exceeded"));
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
        return Err(MsgpackLimitError("trailing MessagePack data"));
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
    fn value(&mut self, depth: usize) -> Result<(), MsgpackLimitError> {
        if depth > self.max_nesting_depth {
            return Err(MsgpackLimitError("MessagePack nesting limit exceeded"));
        }
        self.values = self.values.saturating_add(1);
        if self.values > self.max_total_values {
            return Err(MsgpackLimitError("MessagePack value limit exceeded"));
        }
        let marker = self.take_u8()?;
        match marker {
            0x00..=0x7f | 0xc0 | 0xc2 | 0xc3 | 0xe0..=0xff => Ok(()),
            0x80..=0x8f => self.container(((marker & 0x0f) as usize) * 2, depth),
            0x90..=0x9f => self.container((marker & 0x0f) as usize, depth),
            0xa0..=0xbf => self.scalar((marker & 0x1f) as usize),
            0xc1 => Err(MsgpackLimitError("reserved MessagePack marker")),
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

    fn container(&mut self, items: usize, depth: usize) -> Result<(), MsgpackLimitError> {
        if items > self.max_container_items {
            return Err(MsgpackLimitError("MessagePack container limit exceeded"));
        }
        for _ in 0..items {
            self.value(depth + 1)?;
        }
        Ok(())
    }

    fn scalar(&mut self, len: usize) -> Result<(), MsgpackLimitError> {
        if len > self.max_scalar_bytes {
            return Err(MsgpackLimitError("MessagePack scalar limit exceeded"));
        }
        self.skip(len)
    }

    fn read_uint(&mut self, width: usize) -> Result<usize, MsgpackLimitError> {
        let bytes = self.take(width)?;
        let mut value = 0_u64;
        for byte in bytes {
            value = (value << 8) | u64::from(*byte);
        }
        usize::try_from(value).map_err(|_| MsgpackLimitError("MessagePack length overflow"))
    }

    fn take_u8(&mut self) -> Result<u8, MsgpackLimitError> {
        Ok(self.take(1)?[0])
    }

    fn skip(&mut self, len: usize) -> Result<(), MsgpackLimitError> {
        self.take(len).map(|_| ())
    }

    fn take(&mut self, len: usize) -> Result<&[u8], MsgpackLimitError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(MsgpackLimitError("MessagePack length overflow"))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(MsgpackLimitError("truncated MessagePack value"))?;
        self.offset = end;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_preflight_preserves_exact_and_next_limits() {
        let exact_scalar = [0xd9, 0x01, b'x'];
        validate_msgpack_with_limits(&exact_scalar, 3, 1, 2, 2, 1).expect("exact scalar limit");
        assert!(validate_msgpack_with_limits(&exact_scalar, 2, 1, 2, 2, 1)
            .unwrap_err()
            .to_string()
            .contains("byte limit"));
        assert!(validate_msgpack_with_limits(&exact_scalar, 3, 0, 2, 2, 1)
            .unwrap_err()
            .to_string()
            .contains("scalar limit"));

        let exact_container = [0x92, 0xc0, 0xc0];
        validate_msgpack_with_limits(&exact_container, 3, 1, 2, 3, 1)
            .expect("exact container and value limits");
        assert!(
            validate_msgpack_with_limits(&exact_container, 3, 1, 1, 3, 1)
                .unwrap_err()
                .to_string()
                .contains("container limit")
        );
        assert!(
            validate_msgpack_with_limits(&exact_container, 3, 1, 2, 2, 1)
                .unwrap_err()
                .to_string()
                .contains("value limit")
        );
    }

    #[test]
    fn shared_preflight_rejects_depth_trailing_reserved_and_truncated_data() {
        assert!(validate_msgpack_with_limits(&[0x91, 0xc0], 2, 1, 1, 2, 0)
            .unwrap_err()
            .to_string()
            .contains("nesting limit"));
        assert!(validate_msgpack_with_limits(&[0xc0, 0xc0], 2, 1, 1, 2, 1)
            .unwrap_err()
            .to_string()
            .contains("trailing"));
        assert!(validate_msgpack_with_limits(&[0xc1], 1, 1, 1, 1, 1)
            .unwrap_err()
            .to_string()
            .contains("reserved"));
        assert!(validate_msgpack_with_limits(&[0xd9, 0x01], 2, 1, 1, 1, 1)
            .unwrap_err()
            .to_string()
            .contains("truncated"));
    }
}
