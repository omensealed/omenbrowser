#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = omenbrowser_rs::chat::codec::decode_frame(data);
    let _ = omenbrowser_rs::chat::protocol::batch::decode_values(data);
});
