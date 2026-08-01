use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use rns_transport::destination::link::LinkEvent;
use rns_transport::destination::{DestinationName, SingleInputDestination};
use rns_transport::hash::AddressHash;
use rns_transport::identity::PrivateIdentity;
use rns_transport::packet::PacketContext;
use rns_transport::transport::SendPacketOutcome;
use tokio::sync::Mutex as AsyncMutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::chat::invitation_capability::{
    InvitationCapabilityCodecError, InvitationCapabilityRequest, InvitationCapabilityResponse,
    INVITATION_CAPABILITY_DESTINATION_APPLICATION, INVITATION_CAPABILITY_DESTINATION_ASPECT,
    OMENCHAT_LXMF_INVITATIONS_CAPABILITY,
};

const INVITATION_CAPABILITY_ENDPOINT_SHUTDOWN_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(1);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct InvitationCapabilityEndpointStats {
    accepted_requests: usize,
    rejected_requests: usize,
    sent_responses: usize,
    failed_responses: usize,
}

#[derive(Default)]
struct InvitationCapabilityEndpointCounters {
    accepted_requests: AtomicUsize,
    rejected_requests: AtomicUsize,
    sent_responses: AtomicUsize,
    failed_responses: AtomicUsize,
}

impl InvitationCapabilityEndpointCounters {
    fn snapshot(&self) -> InvitationCapabilityEndpointStats {
        InvitationCapabilityEndpointStats {
            accepted_requests: self.accepted_requests.load(Ordering::Relaxed),
            rejected_requests: self.rejected_requests.load(Ordering::Relaxed),
            sent_responses: self.sent_responses.load(Ordering::Relaxed),
            failed_responses: self.failed_responses.load(Ordering::Relaxed),
        }
    }
}

pub(crate) struct PreparedInvitationCapabilityEndpoint {
    destination_hash: AddressHash,
    destination: Arc<AsyncMutex<SingleInputDestination>>,
}

impl PreparedInvitationCapabilityEndpoint {
    pub(crate) async fn register(
        transport: &mut reticulum_rs::runtime::Transport,
        identity: PrivateIdentity,
    ) -> Self {
        let destination = transport
            .add_destination(
                identity,
                DestinationName::new(
                    INVITATION_CAPABILITY_DESTINATION_APPLICATION,
                    INVITATION_CAPABILITY_DESTINATION_ASPECT,
                ),
            )
            .await;
        let destination_hash = destination.lock().await.desc.address_hash;
        Self {
            destination_hash,
            destination,
        }
    }

    pub(crate) fn spawn(
        self,
        transport: Arc<reticulum_rs::runtime::Transport>,
    ) -> InvitationCapabilityEndpointOwner {
        let shutdown = CancellationToken::new();
        let counters = Arc::new(InvitationCapabilityEndpointCounters::default());
        let task = tokio::spawn(run_invitation_capability_endpoint(
            transport.clone(),
            self.destination_hash,
            shutdown.clone(),
            counters.clone(),
        ));
        InvitationCapabilityEndpointOwner {
            inner: Arc::new(InvitationCapabilityEndpointInner {
                transport,
                destination_hash: self.destination_hash,
                _destination: self.destination,
                shutdown,
                counters,
                task: AsyncMutex::new(Some(task)),
                deregistered: std::sync::atomic::AtomicBool::new(false),
            }),
        }
    }
}

#[derive(Clone)]
pub(crate) struct InvitationCapabilityEndpointOwner {
    inner: Arc<InvitationCapabilityEndpointInner>,
}

struct InvitationCapabilityEndpointInner {
    transport: Arc<reticulum_rs::runtime::Transport>,
    destination_hash: AddressHash,
    _destination: Arc<AsyncMutex<SingleInputDestination>>,
    shutdown: CancellationToken,
    counters: Arc<InvitationCapabilityEndpointCounters>,
    task: AsyncMutex<Option<JoinHandle<()>>>,
    deregistered: std::sync::atomic::AtomicBool,
}

impl InvitationCapabilityEndpointOwner {
    pub(crate) fn destination_hash(&self) -> AddressHash {
        self.inner.destination_hash
    }

    #[cfg(test)]
    fn stats(&self) -> InvitationCapabilityEndpointStats {
        self.inner.counters.snapshot()
    }

    pub(crate) fn cancel(&self) {
        self.inner.shutdown.cancel();
    }

    pub(crate) async fn shutdown(&self) -> Result<bool, String> {
        self.inner.shutdown.cancel();
        let mut task_result = Ok(());
        if let Some(task) = self.inner.task.lock().await.take() {
            let mut task = task;
            match tokio::time::timeout(INVITATION_CAPABILITY_ENDPOINT_SHUTDOWN_TIMEOUT, &mut task)
                .await
            {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    task_result = Err(format!(
                        "invitation capability endpoint worker join failed: {error}"
                    ));
                }
                Err(_) => {
                    task.abort();
                    let _ = task.await;
                    task_result = Err(
                        "invitation capability endpoint worker exceeded shutdown deadline".into(),
                    );
                }
            }
        }
        if self.inner.deregistered.swap(true, Ordering::AcqRel) {
            task_result?;
            return Ok(false);
        }
        let deregistered = self
            .inner
            .transport
            .deregister_destination(&self.inner.destination_hash)
            .await;
        task_result?;
        Ok(deregistered)
    }
}

impl Drop for InvitationCapabilityEndpointInner {
    fn drop(&mut self) {
        self.shutdown.cancel();
        if let Some(task) = self.task.get_mut().take() {
            task.abort();
        }
    }
}

async fn run_invitation_capability_endpoint(
    transport: Arc<reticulum_rs::runtime::Transport>,
    destination_hash: AddressHash,
    shutdown: CancellationToken,
    counters: Arc<InvitationCapabilityEndpointCounters>,
) {
    let mut events = transport.in_link_events();
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            event = events.recv() => {
                let Ok(event) = event else {
                    if matches!(event, Err(tokio::sync::broadcast::error::RecvError::Closed)) {
                        break;
                    }
                    continue;
                };
                if event.address_hash != destination_hash {
                    continue;
                }
                let LinkEvent::Data(payload) = event.event else {
                    continue;
                };
                if payload.context() != PacketContext::Request {
                    continue;
                }
                let response = match invitation_capability_response(payload.as_slice()) {
                    Ok(response) => {
                        counters.accepted_requests.fetch_add(1, Ordering::Relaxed);
                        response
                    }
                    Err(_) => {
                        counters.rejected_requests.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                };
                let Some(link) = transport.find_in_link(&event.id).await else {
                    counters.failed_responses.fetch_add(1, Ordering::Relaxed);
                    continue;
                };
                let packet = {
                    let link = link.lock().await;
                    let mut packet = match link.data_packet(&response) {
                        Ok(packet) => packet,
                        Err(_) => {
                            counters.failed_responses.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }
                    };
                    packet.context = PacketContext::Response;
                    packet
                };
                if matches!(
                    transport.send_link_packet_on_bound_iface(&link, packet).await,
                    SendPacketOutcome::SentDirect | SendPacketOutcome::SentBroadcast
                ) {
                    counters.sent_responses.fetch_add(1, Ordering::Relaxed);
                } else {
                    counters.failed_responses.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }
}

fn invitation_capability_response(
    request: &[u8],
) -> Result<Vec<u8>, InvitationCapabilityCodecError> {
    let request = InvitationCapabilityRequest::decode(request)?;
    InvitationCapabilityResponse::new(
        request.nonce,
        vec![OMENCHAT_LXMF_INVITATIONS_CAPABILITY.to_owned()],
    )?
    .encode()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core::OsRng;

    #[test]
    fn handler_returns_only_the_exact_bounded_capability_and_nonce() {
        let request = InvitationCapabilityRequest { nonce: [0x31; 16] };
        let response =
            invitation_capability_response(&request.encode().expect("request")).expect("response");
        let response = InvitationCapabilityResponse::decode(&response).expect("decode response");
        assert_eq!(response.nonce, request.nonce);
        assert_eq!(
            response.capabilities,
            vec![OMENCHAT_LXMF_INVITATIONS_CAPABILITY]
        );
        assert!(invitation_capability_response(&[0xc0]).is_err());
    }

    #[tokio::test]
    async fn endpoint_registers_same_identity_destination_and_shutdown_joins_and_deregisters() {
        let identity = PrivateIdentity::new_from_rand(OsRng);
        let expected = SingleInputDestination::new(
            identity.clone(),
            DestinationName::new(
                INVITATION_CAPABILITY_DESTINATION_APPLICATION,
                INVITATION_CAPABILITY_DESTINATION_ASPECT,
            ),
        )
        .desc
        .address_hash;
        let config = reticulum_rs::runtime::TransportConfig::new(
            "invitation-capability-endpoint-test",
            &identity,
            false,
        );
        let mut transport = reticulum_rs::runtime::Transport::new(config);
        let prepared =
            PreparedInvitationCapabilityEndpoint::register(&mut transport, identity).await;
        assert_eq!(prepared.destination_hash, expected);
        let transport = Arc::new(transport);
        assert!(transport.has_destination(&expected).await);

        let owner = prepared.spawn(transport.clone());
        assert_eq!(owner.destination_hash(), expected);
        assert_eq!(
            owner.inner._destination.lock().await.desc.address_hash,
            expected
        );
        assert_eq!(owner.stats(), InvitationCapabilityEndpointStats::default());
        let second_handle_reference = owner.clone();
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(1), owner.shutdown())
                .await
                .expect("bounded endpoint shutdown")
                .expect("endpoint task join")
        );
        assert!(!transport.has_destination(&expected).await);
        assert!(!tokio::time::timeout(
            std::time::Duration::from_secs(1),
            second_handle_reference.shutdown()
        )
        .await
        .expect("bounded repeated endpoint shutdown")
        .expect("repeated endpoint task join"));
    }

    #[tokio::test]
    async fn dropping_endpoint_owner_cancels_the_worker_without_network_activity() {
        let identity = PrivateIdentity::new_from_rand(OsRng);
        let config = reticulum_rs::runtime::TransportConfig::new(
            "invitation-capability-drop-test",
            &identity,
            false,
        );
        let mut transport = reticulum_rs::runtime::Transport::new(config);
        let prepared =
            PreparedInvitationCapabilityEndpoint::register(&mut transport, identity).await;
        let transport = Arc::new(transport);
        let owner = prepared.spawn(transport);
        assert!(!owner.inner.shutdown.is_cancelled());
        let shutdown = owner.inner.shutdown.clone();
        drop(owner);
        assert!(shutdown.is_cancelled());
    }
}
