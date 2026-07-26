// Public compatibility fixture emitted by the reviewed v0.9.6-3 codec.
// These are protocol bytes and labels, not identity or authentication material.
pub const APPLICATION_VERSION: &str = "0.9.6-3";
pub const PROTOCOL_VERSION: u8 = 1;
pub const PROTOCOL_NAME: &str = "omenchat-v0.1";

pub const ORDINARY_ROOM_MESSAGE: &[u8] = &[
    0x96, 0x01, 0x14, 0x00, 0x07, 0x2a, 0x92, 0x64, 0xaa, 0x68, 0x65, 0x6c, 0x6c, 0x6f,
    0x20, 0x72, 0x6f, 0x6f, 0x6d,
];
