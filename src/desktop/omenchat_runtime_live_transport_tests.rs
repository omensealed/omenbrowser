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
