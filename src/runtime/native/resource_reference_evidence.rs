use std::time::Duration;

use rns_transport::destination::link::{Link, LinkHandleResult};
use rns_transport::destination::{DestinationName, SingleOutputDestination};
use rns_transport::identity::PrivateIdentity;
use rns_transport::resource::{ResourceAdvertisement, ResourceEventKind};
use rns_transport::{PacketContext, PacketDataBuffer};
use tokio::sync::broadcast;
use tokio::time::timeout;

#[tokio::test]
async fn locked_099_public_resource_hash_is_observable_before_dispatch_and_cancellable() {
    let local_identity =
        PrivateIdentity::new_from_name("omenbrowser-resource-reference-evidence-local");
    let config = reticulum_rs::runtime::TransportConfig::new(
        "omenbrowser-resource-reference-evidence",
        &local_identity,
        false,
    );
    let transport = reticulum_rs::runtime::Transport::new(config);
    let mut channel = transport
        .iface_manager()
        .lock()
        .await
        .new_channel_with_role(8, rns_transport::iface::IfaceRole::Unicast);
    let ingress_iface = *channel.address();

    let remote_identity =
        PrivateIdentity::new_from_name("omenbrowser-resource-reference-evidence-remote");
    let destination = SingleOutputDestination::new(
        *remote_identity.as_identity(),
        DestinationName::new("lxmf", "delivery"),
    );
    let outbound = transport.link(destination.desc).await;
    let request = outbound.lock().await.request();
    let (inbound_events, _) = broadcast::channel(4);
    let mut inbound = Link::new_from_request(
        &request,
        remote_identity.sign_key().clone(),
        destination.desc,
        inbound_events,
    )
    .expect("public inbound Link fixture");
    assert!(matches!(
        outbound
            .lock()
            .await
            .handle_packet(&inbound.prove(), ingress_iface),
        LinkHandleResult::Activated
    ));
    let link_id = *outbound.lock().await.id();

    while channel.tx_channel.try_recv().is_ok() {}
    let mut resource_events = transport.resource_events();
    let application_offer_id = b"00000000000000000000000000000001".to_vec();
    let mut observed_hash = None;
    let returned_hash = transport
        .send_resource_observed(
            &link_id,
            b"bounded deterministic payload".to_vec(),
            Some(application_offer_id),
            |hash| observed_hash = Some(hash),
        )
        .await
        .expect("public Resource advertisement dispatch");
    assert_eq!(observed_hash, Some(returned_hash));

    let advertisement_packet = timeout(Duration::from_millis(200), channel.tx_channel.recv())
        .await
        .expect("bounded Resource advertisement wait")
        .expect("Resource advertisement packet");
    assert_eq!(
        advertisement_packet.packet.context,
        PacketContext::ResourceAdvrtisement
    );
    let mut decrypted = PacketDataBuffer::new();
    let plain_len = {
        let plain = inbound
            .decrypt(
                advertisement_packet.packet.data.as_slice(),
                decrypted.accuire_buf_max(),
            )
            .expect("decrypt deterministic advertisement");
        plain.len()
    };
    decrypted.resize(plain_len);
    let advertisement =
        ResourceAdvertisement::unpack(decrypted.as_slice()).expect("public advertisement decode");
    assert_eq!(advertisement.hash, returned_hash);
    assert_eq!(advertisement.original_hash, returned_hash);

    assert!(
        resource_events.try_recv().is_err(),
        "successful advertisement is not terminal delivery evidence"
    );
    assert!(
        transport
            .cancel_resource(&link_id, returned_hash)
            .await
            .expect("cancel active Resource"),
        "active Resource must be cancellable by its observed hash"
    );
    let cancelled = timeout(Duration::from_millis(200), resource_events.recv())
        .await
        .expect("bounded cancellation-event wait")
        .expect("Resource cancellation event");
    assert_eq!(cancelled.link_id, link_id);
    assert_eq!(cancelled.hash, returned_hash);
    assert!(matches!(
        cancelled.kind,
        ResourceEventKind::OutboundCancelled
    ));
}

#[test]
fn locked_099_resource_complete_retains_payload_and_metadata_as_owned_vectors() {
    fn owned_buffers(
        complete: rns_transport::resource::ResourceComplete,
    ) -> (Vec<u8>, Option<Vec<u8>>) {
        (complete.data, complete.metadata)
    }

    let complete = rns_transport::resource::ResourceComplete {
        data: b"whole completed payload".to_vec(),
        metadata: Some(b"application offer id".to_vec()),
        request_id: None,
        is_request: false,
        is_response: false,
    };
    let (data, metadata) = owned_buffers(complete);
    assert_eq!(data, b"whole completed payload");
    assert_eq!(
        metadata.as_deref(),
        Some(b"application offer id".as_slice())
    );
}
