use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

struct CountingAllocator;

static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_BYTES: AtomicUsize = AtomicUsize::new(0);
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            note_allocation(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, pointer: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        let replacement = unsafe { System.realloc(pointer, old, new_size) };
        if !replacement.is_null() {
            if new_size >= old.size() {
                note_allocation(new_size - old.size());
            } else {
                LIVE_BYTES.fetch_sub(old.size() - new_size, Ordering::Relaxed);
            }
        }
        replacement
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn note_allocation(bytes: usize) {
    ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    let live = LIVE_BYTES.fetch_add(bytes, Ordering::Relaxed) + bytes;
    PEAK_BYTES.fetch_max(live, Ordering::Relaxed);
}

fn measure(mut operation: impl FnMut(), iterations: u64) -> (u128, u64, usize) {
    let baseline = LIVE_BYTES.load(Ordering::SeqCst);
    PEAK_BYTES.store(baseline, Ordering::SeqCst);
    ALLOCATIONS.store(0, Ordering::SeqCst);
    let started = Instant::now();
    for _ in 0..iterations {
        operation();
    }
    let elapsed = started.elapsed().as_nanos();
    let allocations = ALLOCATIONS.load(Ordering::SeqCst);
    let peak_delta = PEAK_BYTES.load(Ordering::SeqCst).saturating_sub(baseline);
    (elapsed / u128::from(iterations), allocations, peak_delta)
}

fn report(name: &str, iterations: u64, operation: impl FnMut()) {
    let (ns_per_op, allocations, peak_delta) = measure(operation, iterations);
    println!(
        "{name}\t{iterations}\t{ns_per_op}\t{}\t{peak_delta}",
        allocations / iterations
    );
}

fn main() {
    const SHORT_ITERATIONS: u64 = 100_000;
    const LARGE_ITERATIONS: u64 = 1_000;
    let declared_oversized_scalar = [0xdb, 0x00, 0x08, 0x00, 0x01];
    let actual_oversized = vec![0xc0; 4 * 1024 * 1024 + 1];

    let client_frame = omenbrowser_rs::chat::protocol::Frame::new(
        omenbrowser_rs::chat::protocol::ChatOp::Ping,
        1,
        None,
        omenbrowser_rs::chat::protocol::FrameBody::Empty,
    );
    let client_valid = omenbrowser_rs::chat::codec::encode_frame(&client_frame).unwrap();
    let server_frame = omenchatd::protocol::Frame::new(
        omenchatd::protocol::ChatOp::Ping,
        1,
        None,
        omenchatd::protocol::FrameBody::Empty,
    );
    let server_valid = omenchatd::protocol::codec::encode_frame(&server_frame).unwrap();
    let client_binary_frame = omenbrowser_rs::chat::protocol::Frame::new(
        omenbrowser_rs::chat::protocol::ChatOp::RoomMessage,
        2,
        Some(1),
        omenbrowser_rs::chat::protocol::FrameBody::Fields(vec![
            omenbrowser_rs::chat::protocol::FrameValue::Bytes(vec![0x55; 512 * 1024]),
        ]),
    );
    let server_binary_frame = omenchatd::protocol::Frame::new(
        omenchatd::protocol::ChatOp::RoomMessage,
        2,
        Some(1),
        omenchatd::protocol::FrameBody::Fields(vec![
            omenchatd::protocol::FrameValue::Bytes(vec![0x55; 512 * 1024]),
        ]),
    );
    let client_binary_payload = match &client_binary_frame.body {
        omenbrowser_rs::chat::protocol::FrameBody::Fields(values) => match &values[0] {
            omenbrowser_rs::chat::protocol::FrameValue::Bytes(value) => value,
            _ => unreachable!("measurement fixture is binary"),
        },
        _ => unreachable!("measurement fixture has fields"),
    };
    let server_binary_payload = match &server_binary_frame.body {
        omenchatd::protocol::FrameBody::Fields(values) => match &values[0] {
            omenchatd::protocol::FrameValue::Bytes(value) => value,
            _ => unreachable!("measurement fixture is binary"),
        },
        _ => unreachable!("measurement fixture has fields"),
    };
    let client_binary_batch = vec![omenbrowser_rs::chat::protocol::FrameValue::Bytes(vec![
        0x66;
        512 * 1024
    ])];
    let server_binary_batch = vec![omenchatd::protocol::FrameValue::Bytes(vec![
        0x66;
        512 * 1024
    ])];
    let client_batch_payload = match &client_binary_batch[0] {
        omenbrowser_rs::chat::protocol::FrameValue::Bytes(value) => value,
        _ => unreachable!("measurement fixture is binary"),
    };
    let server_batch_payload = match &server_binary_batch[0] {
        omenchatd::protocol::FrameValue::Bytes(value) => value,
        _ => unreachable!("measurement fixture is binary"),
    };
    let client_compressed_valid = omenbrowser_rs::chat::protocol::batch::compressed_values_body(&[
        omenbrowser_rs::chat::protocol::FrameValue::String("a".repeat(64 * 1024)),
    ])
    .unwrap();
    let mut client_compressed_lying =
        omenbrowser_rs::chat::protocol::batch::compressed_values_body(&[
            omenbrowser_rs::chat::protocol::FrameValue::String("a".repeat(4 * 1024 * 1024 + 1)),
        ])
        .unwrap();
    if let omenbrowser_rs::chat::protocol::FrameBody::Fields(fields) = &mut client_compressed_lying
    {
        fields[1] = omenbrowser_rs::chat::protocol::FrameValue::U64(1);
    }
    let client_compressed_advertised = omenbrowser_rs::chat::protocol::FrameBody::Fields(vec![
        omenbrowser_rs::chat::protocol::FrameValue::U64(1),
        omenbrowser_rs::chat::protocol::FrameValue::U64(4 * 1024 * 1024 + 1),
        omenbrowser_rs::chat::protocol::FrameValue::Bytes(Vec::new()),
    ]);
    let server_compressed_valid = omenchatd::protocol::batch::compressed_values_body(&[
        omenchatd::protocol::FrameValue::String("a".repeat(64 * 1024)),
    ])
    .unwrap();
    let mut server_compressed_lying = omenchatd::protocol::batch::compressed_values_body(&[
        omenchatd::protocol::FrameValue::String("a".repeat(4 * 1024 * 1024 + 1)),
    ])
    .unwrap();
    if let omenchatd::protocol::FrameBody::Fields(fields) = &mut server_compressed_lying {
        fields[1] = omenchatd::protocol::FrameValue::U64(1);
    }
    let server_compressed_advertised = omenchatd::protocol::FrameBody::Fields(vec![
        omenchatd::protocol::FrameValue::U64(1),
        omenchatd::protocol::FrameValue::U64(4 * 1024 * 1024 + 1),
        omenchatd::protocol::FrameValue::Bytes(Vec::new()),
    ]);

    println!("case\titerations\tns_per_op\tallocations_per_op\tpeak_live_byte_delta");
    report("client_encode_binary_512k", 1_000, || {
        let _ = black_box(omenbrowser_rs::chat::codec::encode_frame(
            &client_binary_frame,
        ));
    });
    report("server_encode_binary_512k", 1_000, || {
        let _ = black_box(omenchatd::protocol::codec::encode_frame(
            &server_binary_frame,
        ));
    });
    report("client_encode_binary_512k_legacy_clone", 1_000, || {
        let legacy_copy = client_binary_payload.clone();
        let encoded = omenbrowser_rs::chat::codec::encode_frame(&client_binary_frame);
        let _ = black_box((legacy_copy, encoded));
    });
    report("server_encode_binary_512k_legacy_clone", 1_000, || {
        let legacy_copy = server_binary_payload.clone();
        let encoded = omenchatd::protocol::codec::encode_frame(&server_binary_frame);
        let _ = black_box((legacy_copy, encoded));
    });
    report("client_encode_batch_binary_512k", 1_000, || {
        let _ = black_box(omenbrowser_rs::chat::protocol::batch::encode_values(
            &client_binary_batch,
        ));
    });
    report("server_encode_batch_binary_512k", 1_000, || {
        let _ = black_box(omenchatd::protocol::batch::encode_values(
            &server_binary_batch,
        ));
    });
    report("client_encode_batch_binary_512k_legacy_clone", 1_000, || {
        let legacy_copy = client_batch_payload.clone();
        let encoded =
            omenbrowser_rs::chat::protocol::batch::encode_values(&client_binary_batch);
        let _ = black_box((legacy_copy, encoded));
    });
    report("server_encode_batch_binary_512k_legacy_clone", 1_000, || {
        let legacy_copy = server_batch_payload.clone();
        let encoded = omenchatd::protocol::batch::encode_values(&server_binary_batch);
        let _ = black_box((legacy_copy, encoded));
    });
    report("client_valid_frame", SHORT_ITERATIONS, || {
        let _ = black_box(omenbrowser_rs::chat::codec::decode_frame(&client_valid));
    });
    report("client_declared_oversized", SHORT_ITERATIONS, || {
        let _ = black_box(omenbrowser_rs::chat::codec::decode_frame(
            &declared_oversized_scalar,
        ));
    });
    report("client_actual_oversized", LARGE_ITERATIONS, || {
        let _ = black_box(omenbrowser_rs::chat::codec::decode_frame(&actual_oversized));
    });
    report("client_batch_declared_oversized", SHORT_ITERATIONS, || {
        let _ = black_box(omenbrowser_rs::chat::protocol::batch::decode_values(
            &declared_oversized_scalar,
        ));
    });
    report("server_valid_frame", SHORT_ITERATIONS, || {
        let _ = black_box(omenchatd::protocol::codec::decode_frame(&server_valid));
    });
    report("server_declared_oversized", SHORT_ITERATIONS, || {
        let _ = black_box(omenchatd::protocol::codec::decode_frame(
            &declared_oversized_scalar,
        ));
    });
    report("server_actual_oversized", LARGE_ITERATIONS, || {
        let _ = black_box(omenchatd::protocol::codec::decode_frame(&actual_oversized));
    });
    report("server_batch_declared_oversized", SHORT_ITERATIONS, || {
        let _ = black_box(omenchatd::protocol::batch::decode_values(
            &declared_oversized_scalar,
        ));
    });
    report("client_compressed_valid_64k", 1_000, || {
        let _ = black_box(
            omenbrowser_rs::chat::protocol::batch::decode_compressed_values_body(
                &client_compressed_valid,
            ),
        );
    });
    report(
        "client_compressed_advertised_oversized",
        SHORT_ITERATIONS,
        || {
            let _ = black_box(
                omenbrowser_rs::chat::protocol::batch::decode_compressed_values_body(
                    &client_compressed_advertised,
                ),
            );
        },
    );
    report("client_compressed_lying_4m_as_1", 10_000, || {
        let _ = black_box(
            omenbrowser_rs::chat::protocol::batch::decode_compressed_values_body(
                &client_compressed_lying,
            ),
        );
    });
    report("server_compressed_valid_64k", 1_000, || {
        let _ = black_box(omenchatd::protocol::batch::decode_compressed_values_body(
            &server_compressed_valid,
        ));
    });
    report(
        "server_compressed_advertised_oversized",
        SHORT_ITERATIONS,
        || {
            let _ = black_box(omenchatd::protocol::batch::decode_compressed_values_body(
                &server_compressed_advertised,
            ));
        },
    );
    report("server_compressed_lying_4m_as_1", 10_000, || {
        let _ = black_box(omenchatd::protocol::batch::decode_compressed_values_body(
            &server_compressed_lying,
        ));
    });
}
