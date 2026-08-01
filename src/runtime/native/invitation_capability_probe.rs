use std::sync::Arc;
use std::time::{Duration, Instant};

use rand_core::{OsRng, RngCore};
use rns_transport::destination::link::{Link, LinkEvent, LinkStatus};
use rns_transport::destination::{DestinationName, SingleOutputDestination};
use rns_transport::hash::AddressHash;
use rns_transport::identity::Identity;
use rns_transport::packet::PacketContext;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken as ShutdownToken;

#[cfg(test)]
use crate::chat::invitation_capability::InvitationCapabilityState;
use crate::chat::invitation_capability::{
    InvitationCapabilityEvidenceOwner, InvitationCapabilityProbeOutcome,
    InvitationCapabilityRequest, InvitationCapabilityResponse,
    INVITATION_CAPABILITY_DESTINATION_APPLICATION, INVITATION_CAPABILITY_DESTINATION_ASPECT,
    INVITATION_CAPABILITY_PROBE_DEADLINE_MS, OMENCHAT_LXMF_INVITATIONS_CAPABILITY,
};
use crate::runtime::network::CancellationToken;

const LXMF_DELIVERY_APPLICATION: &str = "lxmf";
const LXMF_DELIVERY_ASPECT: &str = "delivery";
const LINK_EVENT_WAIT_STEP: Duration = Duration::from_millis(100);

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum InvitationCapabilityProbeError {
    #[error("invitation capability probe was cancelled")]
    Cancelled,
    #[error("invitation capability probe admission failed: {0}")]
    Admission(String),
    #[error("peer identity does not match the expected LXMF delivery destination")]
    IdentityConflict,
    #[error("invitation capability response correlation does not match the request")]
    CorrelationConflict,
    #[error("invitation capability destination path was unavailable")]
    PathUnavailable,
    #[error("invitation capability Link was unavailable")]
    LinkUnavailable,
    #[error("invitation capability response was invalid")]
    InvalidResponse,
    #[error("invitation capability probe timed out")]
    Timeout,
    #[error("invitation capability transport stream closed")]
    StreamClosed,
    #[error("invitation capability request could not be dispatched")]
    Dispatch,
}

#[derive(Clone)]
pub(crate) struct InvitationCapabilityProbeAdapter {
    transport: Arc<reticulum_rs::runtime::Transport>,
    evidence: Arc<Mutex<InvitationCapabilityEvidenceOwner>>,
    shutdown: ShutdownToken,
    clock_started: Instant,
}

impl InvitationCapabilityProbeAdapter {
    pub(crate) fn new(transport: Arc<reticulum_rs::runtime::Transport>) -> Self {
        Self {
            transport,
            evidence: Arc::new(Mutex::new(InvitationCapabilityEvidenceOwner::default())),
            shutdown: ShutdownToken::new(),
            clock_started: Instant::now(),
        }
    }

    pub(crate) fn cancel(&self) {
        self.shutdown.cancel();
    }

    pub(crate) async fn shutdown(&self) {
        self.shutdown.cancel();
        self.evidence.lock().await.clear();
    }

    #[cfg(test)]
    pub(crate) async fn state(&self, peer_destination: &str) -> InvitationCapabilityState {
        self.evidence
            .lock()
            .await
            .state(peer_destination, self.now_ms())
    }

    pub(crate) async fn probe(
        &self,
        peer_delivery_destination: AddressHash,
        peer_identity: Identity,
        cancel: CancellationToken,
    ) -> Result<InvitationCapabilityProbeOutcome, InvitationCapabilityProbeError> {
        self.reject_cancelled(&cancel)?;
        let peer = peer_delivery_destination.to_hex_string();
        let started_ms = self.now_ms();
        self.evidence
            .lock()
            .await
            .begin_probe(&peer, started_ms)
            .map_err(|error| InvitationCapabilityProbeError::Admission(error.to_string()))?;

        let result = self
            .probe_admitted(peer_delivery_destination, peer_identity, cancel)
            .await;
        let completed_ms = self.now_ms();
        let outcome = match &result {
            Ok(outcome) => *outcome,
            Err(
                InvitationCapabilityProbeError::IdentityConflict
                | InvitationCapabilityProbeError::CorrelationConflict,
            ) => InvitationCapabilityProbeOutcome::Conflict,
            Err(_) => InvitationCapabilityProbeOutcome::Unknown,
        };
        let _ = self
            .evidence
            .lock()
            .await
            .complete_probe(&peer, outcome, completed_ms);
        result
    }

    async fn probe_admitted(
        &self,
        peer_delivery_destination: AddressHash,
        peer_identity: Identity,
        cancel: CancellationToken,
    ) -> Result<InvitationCapabilityProbeOutcome, InvitationCapabilityProbeError> {
        self.reject_cancelled(&cancel)?;
        let delivery = SingleOutputDestination::new(
            peer_identity,
            DestinationName::new(LXMF_DELIVERY_APPLICATION, LXMF_DELIVERY_ASPECT),
        );
        if delivery.desc.address_hash != peer_delivery_destination {
            return Err(InvitationCapabilityProbeError::IdentityConflict);
        }

        let capability_destination = SingleOutputDestination::new(
            peer_identity,
            DestinationName::new(
                INVITATION_CAPABILITY_DESTINATION_APPLICATION,
                INVITATION_CAPABILITY_DESTINATION_ASPECT,
            ),
        );
        let capability_hash = capability_destination.desc.address_hash;
        let deadline = Instant::now() + probe_deadline();
        self.wait_for_path(capability_hash, deadline, &cancel)
            .await?;
        let link = self
            .open_link(capability_destination, deadline, &cancel)
            .await?;
        let result = self.exchange(&link, deadline, &cancel).await;
        close_link(&self.transport, &link).await;
        result
    }

    async fn wait_for_path(
        &self,
        destination: AddressHash,
        deadline: Instant,
        cancel: &CancellationToken,
    ) -> Result<(), InvitationCapabilityProbeError> {
        if self.transport.path_status(&destination).await.path_found {
            return Ok(());
        }
        self.transport.request_path(&destination, None, None).await;
        loop {
            self.reject_cancelled(cancel)?;
            if self.transport.path_status(&destination).await.path_found {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(InvitationCapabilityProbeError::PathUnavailable);
            }
            tokio::select! {
                _ = self.shutdown.cancelled() => return Err(InvitationCapabilityProbeError::Cancelled),
                _ = cancel.cancelled() => return Err(InvitationCapabilityProbeError::Cancelled),
                _ = tokio::time::sleep(LINK_EVENT_WAIT_STEP) => {}
            }
        }
    }

    async fn open_link(
        &self,
        destination: SingleOutputDestination,
        deadline: Instant,
        cancel: &CancellationToken,
    ) -> Result<Arc<Mutex<Link>>, InvitationCapabilityProbeError> {
        let mut events = self.transport.out_link_events();
        let link = self.transport.link(destination.desc).await;
        let link_id = *link.lock().await.id();
        let result = loop {
            if let Err(error) = self.reject_cancelled(cancel) {
                break Err(error);
            }
            let status = link.lock().await.status();
            match status {
                LinkStatus::Active => break Ok(()),
                LinkStatus::Stale | LinkStatus::Closed => {
                    break Err(InvitationCapabilityProbeError::LinkUnavailable);
                }
                LinkStatus::Pending | LinkStatus::Handshake => {}
            }
            if Instant::now() >= deadline {
                break Err(InvitationCapabilityProbeError::Timeout);
            }
            tokio::select! {
                _ = self.shutdown.cancelled() => break Err(InvitationCapabilityProbeError::Cancelled),
                _ = cancel.cancelled() => break Err(InvitationCapabilityProbeError::Cancelled),
                event = events.recv() => match event {
                    Ok(event) if event.id == link_id && matches!(event.event, LinkEvent::Activated) => break Ok(()),
                    Ok(event) if event.id == link_id && matches!(event.event, LinkEvent::Closed) => break Err(InvitationCapabilityProbeError::LinkUnavailable),
                    Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break Err(InvitationCapabilityProbeError::StreamClosed),
                },
                _ = tokio::time::sleep(LINK_EVENT_WAIT_STEP) => {}
            }
        };
        if let Err(error) = result {
            close_link(&self.transport, &link).await;
            Err(error)
        } else {
            Ok(link)
        }
    }

    async fn exchange(
        &self,
        link: &Arc<Mutex<Link>>,
        deadline: Instant,
        cancel: &CancellationToken,
    ) -> Result<InvitationCapabilityProbeOutcome, InvitationCapabilityProbeError> {
        self.reject_cancelled(cancel)?;
        let mut nonce = [0u8; 16];
        OsRng.fill_bytes(&mut nonce);
        let request = InvitationCapabilityRequest { nonce };
        let request_bytes = request
            .encode()
            .map_err(|_| InvitationCapabilityProbeError::Dispatch)?;
        let link_id = *link.lock().await.id();
        let packet = {
            let link = link.lock().await;
            let mut packet = link
                .data_packet(&request_bytes)
                .map_err(|_| InvitationCapabilityProbeError::Dispatch)?;
            packet.context = PacketContext::Request;
            packet
        };
        let mut responses = self.transport.received_data_events();
        if !matches!(
            self.transport
                .send_link_packet_on_bound_iface(link, packet)
                .await,
            rns_transport::transport::SendPacketOutcome::SentDirect
                | rns_transport::transport::SendPacketOutcome::SentBroadcast
        ) {
            return Err(InvitationCapabilityProbeError::Dispatch);
        }

        loop {
            self.reject_cancelled(cancel)?;
            if Instant::now() >= deadline {
                return Err(InvitationCapabilityProbeError::Timeout);
            }
            tokio::select! {
                _ = self.shutdown.cancelled() => return Err(InvitationCapabilityProbeError::Cancelled),
                _ = cancel.cancelled() => return Err(InvitationCapabilityProbeError::Cancelled),
                response = responses.recv() => match response {
                    Ok(response) if response.destination == link_id && response.context == Some(PacketContext::Response) => {
                        return classify_response(&request, response.data.as_slice());
                    }
                    Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return Err(InvitationCapabilityProbeError::StreamClosed),
                },
                _ = tokio::time::sleep(LINK_EVENT_WAIT_STEP) => {}
            }
        }
    }

    fn reject_cancelled(
        &self,
        cancel: &CancellationToken,
    ) -> Result<(), InvitationCapabilityProbeError> {
        if self.shutdown.is_cancelled() || cancel.is_cancelled() {
            Err(InvitationCapabilityProbeError::Cancelled)
        } else {
            Ok(())
        }
    }

    fn now_ms(&self) -> u64 {
        u64::try_from(self.clock_started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

fn probe_deadline() -> Duration {
    Duration::from_millis(INVITATION_CAPABILITY_PROBE_DEADLINE_MS)
}

fn classify_response(
    request: &InvitationCapabilityRequest,
    bytes: &[u8],
) -> Result<InvitationCapabilityProbeOutcome, InvitationCapabilityProbeError> {
    let response = InvitationCapabilityResponse::decode(bytes)
        .map_err(|_| InvitationCapabilityProbeError::InvalidResponse)?;
    match response.supports_for(request, OMENCHAT_LXMF_INVITATIONS_CAPABILITY) {
        Ok(true) => Ok(InvitationCapabilityProbeOutcome::Supported),
        Ok(false) => Ok(InvitationCapabilityProbeOutcome::Unsupported),
        Err(crate::chat::invitation_capability::InvitationCapabilityCodecError::NonceMismatch) => {
            Err(InvitationCapabilityProbeError::CorrelationConflict)
        }
        Err(_) => Err(InvitationCapabilityProbeError::InvalidResponse),
    }
}

async fn close_link(transport: &Arc<reticulum_rs::runtime::Transport>, link: &Arc<Mutex<Link>>) {
    let teardown = {
        let mut link = link.lock().await;
        link.teardown().map(|packet| (link.ingress_iface(), packet))
    };
    if let Some((Some(interface), packet)) = teardown {
        transport.send_direct(interface, packet).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core::OsRng;
    use rns_transport::identity::PrivateIdentity;

    fn test_adapter(name: &str) -> InvitationCapabilityProbeAdapter {
        let identity = PrivateIdentity::new_from_name(name);
        let config = reticulum_rs::runtime::TransportConfig::new(name, &identity, false);
        InvitationCapabilityProbeAdapter::new(Arc::new(reticulum_rs::runtime::Transport::new(
            config,
        )))
    }

    #[tokio::test]
    async fn pre_cancelled_probe_dispatches_nothing_and_retains_no_evidence() {
        let adapter = test_adapter("invitation-capability-pre-cancelled");
        let peer = PrivateIdentity::new_from_rand(OsRng);
        let delivery = SingleOutputDestination::new(
            *peer.as_identity(),
            DestinationName::new(LXMF_DELIVERY_APPLICATION, LXMF_DELIVERY_ASPECT),
        );
        let cancel = CancellationToken::new();
        cancel.cancel();

        assert_eq!(
            adapter
                .probe(delivery.desc.address_hash, *peer.as_identity(), cancel)
                .await,
            Err(InvitationCapabilityProbeError::Cancelled)
        );
        assert_eq!(
            adapter
                .state(&delivery.desc.address_hash.to_hex_string())
                .await,
            InvitationCapabilityState::Unknown
        );
        assert_eq!(adapter.transport.link_count().await, 0);
    }

    #[tokio::test]
    async fn identity_mismatch_is_conflict_before_path_or_link_work() {
        let adapter = test_adapter("invitation-capability-conflict");
        let peer = PrivateIdentity::new_from_rand(OsRng);
        let wrong_destination = AddressHash::new_from_rand(OsRng);

        assert_eq!(
            adapter
                .probe(
                    wrong_destination,
                    *peer.as_identity(),
                    CancellationToken::new()
                )
                .await,
            Err(InvitationCapabilityProbeError::IdentityConflict)
        );
        assert_eq!(
            adapter.state(&wrong_destination.to_hex_string()).await,
            InvitationCapabilityState::Conflict
        );
        assert_eq!(adapter.transport.link_count().await, 0);
    }

    #[tokio::test]
    async fn runtime_shutdown_clears_bounded_evidence_and_rejects_future_probe() {
        let adapter = test_adapter("invitation-capability-shutdown");
        let peer = PrivateIdentity::new_from_rand(OsRng);
        let wrong_destination = AddressHash::new_from_rand(OsRng);
        let _ = adapter
            .probe(
                wrong_destination,
                *peer.as_identity(),
                CancellationToken::new(),
            )
            .await;
        assert_eq!(
            adapter.state(&wrong_destination.to_hex_string()).await,
            InvitationCapabilityState::Conflict
        );

        adapter.shutdown().await;
        assert_eq!(
            adapter.state(&wrong_destination.to_hex_string()).await,
            InvitationCapabilityState::Unknown
        );
        assert_eq!(
            adapter
                .probe(
                    wrong_destination,
                    *peer.as_identity(),
                    CancellationToken::new()
                )
                .await,
            Err(InvitationCapabilityProbeError::Cancelled)
        );
    }

    #[test]
    fn response_classification_requires_exact_nonce_and_capability() {
        let request = InvitationCapabilityRequest { nonce: [0x41; 16] };
        let supported = InvitationCapabilityResponse::new(
            request.nonce,
            vec![OMENCHAT_LXMF_INVITATIONS_CAPABILITY.to_owned()],
        )
        .expect("supported response")
        .encode()
        .expect("encode supported response");
        let unsupported = InvitationCapabilityResponse::new(request.nonce, Vec::new())
            .expect("unsupported response")
            .encode()
            .expect("encode unsupported response");
        let replay = InvitationCapabilityResponse::new(
            [0x42; 16],
            vec![OMENCHAT_LXMF_INVITATIONS_CAPABILITY.to_owned()],
        )
        .expect("replayed response")
        .encode()
        .expect("encode replayed response");

        assert_eq!(
            classify_response(&request, &supported),
            Ok(InvitationCapabilityProbeOutcome::Supported)
        );
        assert_eq!(
            classify_response(&request, &unsupported),
            Ok(InvitationCapabilityProbeOutcome::Unsupported)
        );
        assert_eq!(
            classify_response(&request, &replay),
            Err(InvitationCapabilityProbeError::CorrelationConflict)
        );
        assert_eq!(
            classify_response(&request, &[0xc0]),
            Err(InvitationCapabilityProbeError::InvalidResponse)
        );
        assert_eq!(
            probe_deadline(),
            Duration::from_millis(INVITATION_CAPABILITY_PROBE_DEADLINE_MS)
        );
    }
}
