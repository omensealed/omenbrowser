use super::*;

#[tokio::test]
async fn omenchat_transport_tracks_ping_pong_rtt_for_monitoring() {
    let mut transport = DesktopOmenChatTransport::new([0x71; 16], 1_000);
    transport.last_ping_epoch_ms = 2_000;
    transport.awaiting_pong = true;

    let pong = crate::chat::protocol::Frame::new(
        crate::chat::protocol::ChatOp::Pong,
        7,
        None,
        crate::chat::protocol::FrameBody::Empty,
    );
    transport.push_incoming_frame(
        crate::chat::codec::encode_frame(&pong).expect("encode pong"),
        2_125,
    );

    assert_eq!(transport.pongs_in, 1);
    assert_eq!(transport.last_pong_epoch_ms, 2_125);
    assert_eq!(transport.last_ping_rtt_ms, Some(125));
    assert!(!transport.awaiting_pong);
}

#[test]
fn omenchat_transport_consumes_and_bounds_resource_payloads_and_offers() {
    let mut transport = DesktopOmenChatTransport::new([0x72; 16], 1_000);
    let metadata = |id: &str| Some(crate::chat::rns::resource_metadata(id));

    assert!(transport.push_resource(metadata("upload:first"), vec![1, 2, 3], 1_001));
    assert_eq!(transport.resource_cached_bytes, 3);
    assert_eq!(
        transport.fetch_resource("upload:first").expect("resource"),
        Some(vec![1, 2, 3])
    );
    assert_eq!(transport.resource_cached_bytes, 0);
    assert!(transport
        .fetch_resource("upload:first")
        .expect("consumed resource")
        .is_none());

    assert!(!transport.push_resource(
        metadata("upload:oversize"),
        vec![0; crate::desktop::OMENCHAT_RESOURCE_MAX_BYTES + 1],
        1_002,
    ));
    assert_eq!(transport.rejected_resources, 1);

    for index in 0..=crate::desktop::OMENCHAT_RESOURCE_CACHE_MAX_ITEMS {
        assert!(transport.push_resource(
            metadata(&format!("history:{index}")),
            vec![index as u8],
            1_010 + index as u64,
        ));
    }
    assert_eq!(
        transport.resources.len(),
        crate::desktop::OMENCHAT_RESOURCE_CACHE_MAX_ITEMS
    );
    assert!(!transport.resources.contains_key("history:0"));

    let mut byte_bounded = DesktopOmenChatTransport::new([0x73; 16], 1_000);
    assert!(byte_bounded.push_resource(
        metadata("history:large-a"),
        vec![0; crate::desktop::OMENCHAT_RESOURCE_MAX_BYTES],
        1_001,
    ));
    assert!(byte_bounded.push_resource(
        metadata("history:large-b"),
        vec![0; crate::desktop::OMENCHAT_RESOURCE_MAX_BYTES],
        1_002,
    ));
    assert!(byte_bounded.push_resource(metadata("history:new"), vec![0], 1_003));
    assert!(!byte_bounded.resources.contains_key("history:large-a"));
    assert!(
        byte_bounded.resource_cached_bytes <= crate::desktop::OMENCHAT_RESOURCE_CACHE_MAX_BYTES
    );

    for index in 0..crate::desktop::OMENCHAT_PENDING_RESOURCE_OFFER_MAX_ITEMS {
        transport
            .defer_resource_offer(&format!("pending:{index}"), vec![0x41])
            .expect("bounded pending offer");
    }
    assert_eq!(
        transport.pending_resource_offer_count(),
        crate::desktop::OMENCHAT_PENDING_RESOURCE_OFFER_MAX_ITEMS
    );
    assert!(transport
        .defer_resource_offer("pending:overflow", vec![0x42])
        .is_err());
    assert_eq!(transport.rejected_resource_offers, 1);

    let mut byte_bounded_offers = DesktopOmenChatTransport::new([0x74; 16], 1_000);
    for index in 0..4 {
        byte_bounded_offers
            .defer_resource_offer(
                &format!("large-pending:{index}"),
                vec![0; crate::chat::codec::MAX_FRAME_BYTES],
            )
            .expect("exact pending byte budget");
    }
    assert_eq!(
        byte_bounded_offers.pending_resource_offer_bytes,
        crate::desktop::OMENCHAT_PENDING_RESOURCE_OFFER_MAX_BYTES
    );
    assert!(byte_bounded_offers
        .defer_resource_offer("large-pending:overflow", vec![0])
        .is_err());

    assert!(transport.push_resource(metadata("pending:0"), vec![0x55], 2_000));
    assert_eq!(transport.pending_resource_offer_bytes, 31);
    assert_eq!(
        transport.pending_resource_offer_count(),
        crate::desktop::OMENCHAT_PENDING_RESOURCE_OFFER_MAX_ITEMS - 1
    );
    assert_eq!(transport.incoming_frame_bytes, 1);
    assert_eq!(
        transport.recv_frame().expect("replayed offer"),
        Some(vec![0x41])
    );
    assert_eq!(transport.incoming_frame_bytes, 0);
}

#[test]
fn omenchat_transport_bounds_all_payload_queues_and_releases_byte_accounting() {
    let frame_bytes = crate::desktop::OMENCHAT_TRANSPORT_FRAME_QUEUE_MAX_BYTES
        / crate::desktop::OMENCHAT_TRANSPORT_FRAME_QUEUE_MAX_ITEMS;
    let mut inbound = DesktopOmenChatTransport::new([0x75; 16], 1_000);
    for index in 0..crate::desktop::OMENCHAT_TRANSPORT_FRAME_QUEUE_MAX_ITEMS {
        assert!(inbound.push_incoming_frame(vec![index as u8; frame_bytes], 1_001));
    }
    assert_eq!(
        inbound.incoming_frame_bytes,
        crate::desktop::OMENCHAT_TRANSPORT_FRAME_QUEUE_MAX_BYTES
    );
    assert!(!inbound.push_incoming_frame(vec![0], 1_002));
    assert_eq!(inbound.rejected_incoming_frames, 1);
    while inbound.recv_frame().expect("receive frame").is_some() {}
    assert_eq!(inbound.incoming_frames.len(), 0);
    assert_eq!(inbound.incoming_frame_bytes, 0);

    let mut outbound = DesktopOmenChatTransport::new([0x76; 16], 1_000);
    for index in 0..crate::desktop::OMENCHAT_TRANSPORT_FRAME_QUEUE_MAX_ITEMS {
        outbound
            .send_frame(vec![index as u8; frame_bytes])
            .expect("bounded outgoing frame");
    }
    assert!(outbound.send_frame(vec![0]).is_err());
    assert_eq!(outbound.rejected_outgoing_frames, 1);
    assert_eq!(
        outbound.outgoing_frame_bytes,
        crate::desktop::OMENCHAT_TRANSPORT_FRAME_QUEUE_MAX_BYTES
    );
    assert_eq!(
        outbound.take_outgoing_frames().len(),
        crate::desktop::OMENCHAT_TRANSPORT_FRAME_QUEUE_MAX_ITEMS
    );
    assert_eq!(outbound.outgoing_frame_bytes, 0);
    outbound.send_frame(vec![0]).expect("permit released");

    let mut resource_bytes = DesktopOmenChatTransport::new([0x77; 16], 1_000);
    for index in 0..2 {
        resource_bytes
            .send_resource(
                &format!("resource:{index}"),
                vec![index as u8; crate::desktop::OMENCHAT_RESOURCE_MAX_BYTES],
            )
            .expect("exact outgoing Resource byte budget");
    }
    assert_eq!(
        resource_bytes.outgoing_resource_bytes,
        crate::desktop::OMENCHAT_TRANSPORT_RESOURCE_QUEUE_MAX_BYTES
    );
    assert!(resource_bytes
        .send_resource("resource:overflow", vec![0])
        .is_err());
    assert_eq!(resource_bytes.rejected_outgoing_resources, 1);
    assert_eq!(resource_bytes.take_outgoing_resources().len(), 2);
    assert_eq!(resource_bytes.outgoing_resource_bytes, 0);

    let mut resource_items = DesktopOmenChatTransport::new([0x78; 16], 1_000);
    for index in 0..crate::desktop::OMENCHAT_TRANSPORT_RESOURCE_QUEUE_MAX_ITEMS {
        resource_items
            .send_resource(&format!("empty:{index}"), Vec::new())
            .expect("bounded outgoing Resource item");
    }
    assert!(resource_items
        .send_resource("empty:overflow", Vec::new())
        .is_err());
    assert_eq!(resource_items.rejected_outgoing_resources, 1);

    let mut invalid_resource = DesktopOmenChatTransport::new([0x79; 16], 1_000);
    assert!(invalid_resource
        .send_resource(
            "resource:oversize",
            vec![0; crate::desktop::OMENCHAT_RESOURCE_MAX_BYTES + 1],
        )
        .is_err());
    assert!(invalid_resource
        .send_resource(
            &"x".repeat(crate::desktop::OMENCHAT_TRANSPORT_RESOURCE_ID_MAX_BYTES + 1),
            Vec::new(),
        )
        .is_err());
    assert_eq!(invalid_resource.rejected_outgoing_resources, 2);
    assert_eq!(invalid_resource.outgoing_resource_bytes, 0);
}
