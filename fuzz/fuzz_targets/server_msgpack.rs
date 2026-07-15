#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = omenchatd::protocol::codec::decode_frame(data);
    let _ = omenchatd::protocol::batch::decode_values(data);
});
