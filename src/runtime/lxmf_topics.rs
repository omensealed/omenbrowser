use thiserror::Error;

pub const LXMF_TOPIC_CAP_TOPICS: &str = "sdk.capability.topics";
pub const LXMF_TOPIC_CAP_SUBSCRIPTIONS: &str = "sdk.capability.topic_subscriptions";
pub const LXMF_TOPIC_CAP_FANOUT: &str = "sdk.capability.topic_fanout";
pub const LXMF_TOPIC_CAP_CURSOR_REPLAY: &str = "sdk.capability.cursor_replay";
pub const LXMF_TOPIC_CAP_ASYNC_EVENTS: &str = "sdk.capability.async_events";
pub const LXMF_TOPIC_CAPABILITY_MAX_ITEMS: usize = 64;
pub const LXMF_TOPIC_CAPABILITY_NAME_MAX_BYTES: usize = 128;
pub const LXMF_TOPIC_CAPABILITY_MAX_ACCOUNTED_BYTES: usize = 4 * 1024;
pub const LXMF_TOPIC_PUBLISHED_EVENT_TYPE: &str = "sdk_topic_published";
pub const LXMF_TOPIC_EVENT_MAX_ACCOUNTED_BYTES: usize = 8 * 1024;
pub const LXMF_TOPIC_EVENT_ID_MAX_BYTES: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LxmfTopicBackendMode {
    ManagedNative,
    ExternalSdkRpc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LxmfTopicReceiveReadiness {
    ProductAdapterMissing,
    CapabilityAbsent,
    RecoveryUnproven,
    TopicEventContractUnproven,
    PublisherAuthenticationUnproven,
    EligibleForReceiveAdapter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LxmfTopicEventContractState {
    NotTopicPublication,
    Malformed,
    PublisherAuthenticationAbsent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LxmfTopicEventContractFinding {
    pub state: LxmfTopicEventContractState,
    pub sdk_peer_id_present: bool,
    pub topic_id_present: bool,
    pub correlation_id_present: bool,
    pub authenticated_publisher: bool,
    pub admission_allowed: bool,
}

pub fn inspect_external_topic_event_contract(
    event_type: &str,
    sdk_peer_id: Option<&str>,
    payload: &serde_json::Value,
) -> LxmfTopicEventContractFinding {
    if event_type != LXMF_TOPIC_PUBLISHED_EVENT_TYPE {
        return LxmfTopicEventContractFinding {
            state: LxmfTopicEventContractState::NotTopicPublication,
            sdk_peer_id_present: sdk_peer_id.is_some(),
            topic_id_present: false,
            correlation_id_present: false,
            authenticated_publisher: false,
            admission_allowed: false,
        };
    }
    let accounted_bytes = serde_json::to_vec(payload).map_or(usize::MAX, |bytes| bytes.len());
    let topic_id = payload.get("topic_id").and_then(serde_json::Value::as_str);
    let correlation_id = payload
        .get("correlation_id")
        .and_then(serde_json::Value::as_str);
    let shape_valid = accounted_bytes <= LXMF_TOPIC_EVENT_MAX_ACCOUNTED_BYTES
        && topic_id.is_some_and(|value| {
            !value.is_empty()
                && value.len() <= LXMF_TOPIC_EVENT_ID_MAX_BYTES
                && !value.chars().any(char::is_control)
        })
        && correlation_id.is_none_or(|value| {
            !value.is_empty()
                && value.len() <= LXMF_TOPIC_EVENT_ID_MAX_BYTES
                && !value.chars().any(char::is_control)
        })
        && payload.get("payload").is_some();

    LxmfTopicEventContractFinding {
        state: if shape_valid {
            LxmfTopicEventContractState::PublisherAuthenticationAbsent
        } else {
            LxmfTopicEventContractState::Malformed
        },
        sdk_peer_id_present: sdk_peer_id.is_some(),
        topic_id_present: topic_id.is_some(),
        correlation_id_present: correlation_id.is_some(),
        authenticated_publisher: false,
        admission_allowed: false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LxmfTopicCapabilityReport {
    pub mode: LxmfTopicBackendMode,
    pub topics: bool,
    pub subscriptions: bool,
    pub fanout: bool,
    pub cursor_replay: bool,
    pub async_events: bool,
    pub topic_event_contract_proven: bool,
    pub authenticated_publisher_events: bool,
    pub cursor_gap_recovery_proven: bool,
    pub receive_readiness: LxmfTopicReceiveReadiness,
}

impl LxmfTopicCapabilityReport {
    pub const fn managed_native_unwired() -> Self {
        Self {
            mode: LxmfTopicBackendMode::ManagedNative,
            topics: false,
            subscriptions: false,
            fanout: false,
            cursor_replay: false,
            async_events: false,
            topic_event_contract_proven: false,
            authenticated_publisher_events: false,
            cursor_gap_recovery_proven: false,
            receive_readiness: LxmfTopicReceiveReadiness::ProductAdapterMissing,
        }
    }

    pub fn external_negotiated(
        effective_capabilities: &[String],
        topic_event_contract_proven: bool,
        authenticated_publisher_events: bool,
        cursor_gap_recovery_proven: bool,
    ) -> Result<Self, LxmfTopicCapabilityError> {
        validate_capability_snapshot(effective_capabilities)?;
        let has = |wanted: &str| {
            effective_capabilities
                .iter()
                .any(|capability| capability == wanted)
        };
        let topics = has(LXMF_TOPIC_CAP_TOPICS);
        let subscriptions = has(LXMF_TOPIC_CAP_SUBSCRIPTIONS);
        let fanout = has(LXMF_TOPIC_CAP_FANOUT);
        let cursor_replay = has(LXMF_TOPIC_CAP_CURSOR_REPLAY);
        let async_events = has(LXMF_TOPIC_CAP_ASYNC_EVENTS);
        let receive_readiness = if !topics || !subscriptions {
            LxmfTopicReceiveReadiness::CapabilityAbsent
        } else if !cursor_replay || !async_events || !cursor_gap_recovery_proven {
            LxmfTopicReceiveReadiness::RecoveryUnproven
        } else if !topic_event_contract_proven {
            LxmfTopicReceiveReadiness::TopicEventContractUnproven
        } else if !authenticated_publisher_events {
            LxmfTopicReceiveReadiness::PublisherAuthenticationUnproven
        } else {
            LxmfTopicReceiveReadiness::EligibleForReceiveAdapter
        };
        Ok(Self {
            mode: LxmfTopicBackendMode::ExternalSdkRpc,
            topics,
            subscriptions,
            fanout,
            cursor_replay,
            async_events,
            topic_event_contract_proven,
            authenticated_publisher_events,
            cursor_gap_recovery_proven,
            receive_readiness,
        })
    }

    pub const fn may_publish(&self) -> bool {
        self.topics && self.fanout
    }

    pub const fn may_activate_receive_adapter(&self) -> bool {
        matches!(
            self.receive_readiness,
            LxmfTopicReceiveReadiness::EligibleForReceiveAdapter
        )
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum LxmfTopicCapabilityError {
    #[error("LXMF topic capability snapshot contains too many entries")]
    TooManyCapabilities,
    #[error("LXMF topic capability name is invalid")]
    InvalidCapability,
    #[error("LXMF topic capability snapshot exceeds its byte budget")]
    CapabilityBytes,
}

fn validate_capability_snapshot(capabilities: &[String]) -> Result<(), LxmfTopicCapabilityError> {
    if capabilities.len() > LXMF_TOPIC_CAPABILITY_MAX_ITEMS {
        return Err(LxmfTopicCapabilityError::TooManyCapabilities);
    }
    let mut accounted_bytes = 0usize;
    for capability in capabilities {
        if capability.is_empty()
            || capability.len() > LXMF_TOPIC_CAPABILITY_NAME_MAX_BYTES
            || capability.chars().any(char::is_control)
        {
            return Err(LxmfTopicCapabilityError::InvalidCapability);
        }
        accounted_bytes = accounted_bytes
            .checked_add(capability.len())
            .ok_or(LxmfTopicCapabilityError::CapabilityBytes)?;
        if accounted_bytes > LXMF_TOPIC_CAPABILITY_MAX_ACCOUNTED_BYTES {
            return Err(LxmfTopicCapabilityError::CapabilityBytes);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn advertised() -> Vec<String> {
        [
            LXMF_TOPIC_CAP_TOPICS,
            LXMF_TOPIC_CAP_SUBSCRIPTIONS,
            LXMF_TOPIC_CAP_FANOUT,
            LXMF_TOPIC_CAP_CURSOR_REPLAY,
            LXMF_TOPIC_CAP_ASYNC_EVENTS,
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }

    #[test]
    fn current_product_modes_do_not_infer_topic_support_from_dependency_presence() {
        let managed = LxmfTopicCapabilityReport::managed_native_unwired();
        assert_eq!(
            managed.receive_readiness,
            LxmfTopicReceiveReadiness::ProductAdapterMissing
        );
        assert!(!managed.may_publish());
        assert!(!managed.may_activate_receive_adapter());

        let external =
            LxmfTopicCapabilityReport::external_negotiated(&advertised(), false, false, false)
                .expect("bounded capability report");
        assert!(external.may_publish());
        assert_eq!(
            external.receive_readiness,
            LxmfTopicReceiveReadiness::RecoveryUnproven
        );
        assert!(!external.may_activate_receive_adapter());
    }

    #[test]
    fn external_receive_requires_recovery_topic_contract_and_publisher_authentication() {
        let capabilities = advertised();
        let recovery_only =
            LxmfTopicCapabilityReport::external_negotiated(&capabilities, false, false, true)
                .expect("report");
        assert_eq!(
            recovery_only.receive_readiness,
            LxmfTopicReceiveReadiness::TopicEventContractUnproven
        );
        let topic_contract =
            LxmfTopicCapabilityReport::external_negotiated(&capabilities, true, false, true)
                .expect("report");
        assert_eq!(
            topic_contract.receive_readiness,
            LxmfTopicReceiveReadiness::PublisherAuthenticationUnproven
        );
        let eligible =
            LxmfTopicCapabilityReport::external_negotiated(&capabilities, true, true, true)
                .expect("report");
        assert!(eligible.may_activate_receive_adapter());
    }

    #[test]
    fn capability_snapshot_is_item_name_and_byte_bounded() {
        assert_eq!(
            LxmfTopicCapabilityReport::external_negotiated(
                &vec!["valid".into(); LXMF_TOPIC_CAPABILITY_MAX_ITEMS + 1],
                false,
                false,
                false,
            ),
            Err(LxmfTopicCapabilityError::TooManyCapabilities)
        );
        assert_eq!(
            LxmfTopicCapabilityReport::external_negotiated(
                &["x".repeat(LXMF_TOPIC_CAPABILITY_NAME_MAX_BYTES + 1)],
                false,
                false,
                false,
            ),
            Err(LxmfTopicCapabilityError::InvalidCapability)
        );
        assert_eq!(
            LxmfTopicCapabilityReport::external_negotiated(
                &vec!["x".repeat(128); 33],
                false,
                false,
                false,
            ),
            Err(LxmfTopicCapabilityError::CapabilityBytes)
        );
    }

    #[test]
    fn topic_event_classifier_never_upgrades_generic_peer_or_payload_to_authentication() {
        let payload = serde_json::json!({
            "topic_id": "topic-1",
            "correlation_id": "correlation-1",
            "ts_ms": 1,
            "payload": { "pointer": true }
        });
        for peer_id in [None, Some("unproven-sdk-peer")] {
            let finding = inspect_external_topic_event_contract(
                LXMF_TOPIC_PUBLISHED_EVENT_TYPE,
                peer_id,
                &payload,
            );
            assert_eq!(
                finding.state,
                LxmfTopicEventContractState::PublisherAuthenticationAbsent
            );
            assert_eq!(finding.sdk_peer_id_present, peer_id.is_some());
            assert!(finding.topic_id_present);
            assert!(finding.correlation_id_present);
            assert!(!finding.authenticated_publisher);
            assert!(!finding.admission_allowed);
        }

        let oversized = serde_json::json!({
            "topic_id": "topic-1",
            "payload": "x".repeat(LXMF_TOPIC_EVENT_MAX_ACCOUNTED_BYTES)
        });
        assert_eq!(
            inspect_external_topic_event_contract(
                LXMF_TOPIC_PUBLISHED_EVENT_TYPE,
                None,
                &oversized,
            )
            .state,
            LxmfTopicEventContractState::Malformed
        );
        assert_eq!(
            inspect_external_topic_event_contract("delivery_state", None, &payload).state,
            LxmfTopicEventContractState::NotTopicPublication
        );
    }

    #[cfg(feature = "native-lxmf-sdk")]
    #[test]
    fn locked_097_sdk_exposes_topic_calls_but_only_generic_event_provenance() {
        use lxmf_sdk::LxmfSdkTopics;

        fn assert_topics_trait<T: LxmfSdkTopics>() {}
        assert_topics_trait::<lxmf_sdk::Client<lxmf_sdk::RpcBackendClient>>();
        let _create = lxmf_sdk::TopicCreateRequest {
            topic_path: Some(lxmf_sdk::TopicPath("omenbrowser/nomadnet".into())),
            metadata: Default::default(),
            extensions: Default::default(),
        };
        let _subscribe = lxmf_sdk::TopicSubscriptionRequest {
            topic_id: lxmf_sdk::TopicId("topic-id".into()),
            cursor: None,
            extensions: Default::default(),
        };
        let _publish = lxmf_sdk::TopicPublishRequest {
            topic_id: lxmf_sdk::TopicId("topic-id".into()),
            payload: serde_json::json!({"pointer": true}),
            correlation_id: Some("correlation".into()),
            extensions: Default::default(),
        };
        let event: lxmf_sdk::SdkEvent = serde_json::from_value(serde_json::json!({
            "event_id": "event",
            "runtime_id": "runtime",
            "stream_id": "stream",
            "seq_no": 1,
            "contract_version": 2,
            "ts_ms": 1,
            "event_type": "topic.example",
            "severity": "info",
            "source_component": "test",
            "operation_id": null,
            "message_id": null,
            "peer_id": "peer",
            "correlation_id": null,
            "trace_id": null,
            "payload": {},
            "extensions": {}
        }))
        .expect("generic SDK event");
        assert_eq!(event.peer_id.as_deref(), Some("peer"));
        assert!(lxmf_sdk::profiles::supports_capability(
            lxmf_sdk::Profile::DesktopFull,
            LXMF_TOPIC_CAP_TOPICS,
        ));
        assert!(lxmf_sdk::profiles::supports_capability(
            lxmf_sdk::Profile::DesktopFull,
            LXMF_TOPIC_CAP_SUBSCRIPTIONS,
        ));
        assert!(lxmf_sdk::profiles::supports_capability(
            lxmf_sdk::Profile::DesktopFull,
            LXMF_TOPIC_CAP_FANOUT,
        ));
    }

    #[cfg(feature = "native-lxmf-sdk")]
    #[test]
    fn locked_097_daemon_reproducer_has_no_publisher_or_subscription_cursor_proof() {
        fn request(id: u64, method: &str, params: serde_json::Value) -> rns_rpc::RpcRequest {
            rns_rpc::RpcRequest {
                id,
                method: method.to_owned(),
                params: Some(params),
            }
        }

        let store = rns_rpc::MessagesStore::in_memory().expect("in-memory topic audit store");
        let daemon = rns_rpc::RpcDaemon::with_store(store, "topic-audit-daemon".into());
        let created = daemon
            .handle_rpc(request(
                1,
                "sdk_topic_create_v2",
                serde_json::json!({ "topic_path": "omenbrowser/nomadnet" }),
            ))
            .expect("create topic");
        assert!(created.error.is_none());
        let topic_id = created.result.expect("topic result")["topic"]["topic_id"]
            .as_str()
            .expect("topic id")
            .to_owned();

        let subscribed = daemon
            .handle_rpc(request(
                2,
                "sdk_topic_subscribe_v2",
                serde_json::json!({
                    "topic_id": topic_id.clone(),
                    "cursor": "not-a-valid-event-cursor"
                }),
            ))
            .expect("subscribe topic");
        assert!(subscribed.error.is_none());
        assert_eq!(
            subscribed
                .result
                .as_ref()
                .and_then(|value| value["accepted"].as_bool()),
            Some(true),
            "0.9.7 accepts but does not validate or apply the subscription cursor"
        );

        let published = daemon
            .handle_rpc(request(
                3,
                "sdk_topic_publish_v2",
                serde_json::json!({
                    "topic_id": topic_id.clone(),
                    "payload": { "pointer": true },
                    "correlation_id": "correlation-1"
                }),
            ))
            .expect("publish topic");
        assert!(published.error.is_none());

        let polled = daemon
            .handle_rpc(request(
                4,
                "sdk_poll_events_v2",
                serde_json::json!({ "cursor": null, "max": 64 }),
            ))
            .expect("poll topic event");
        assert!(polled.error.is_none());
        let result = polled.result.expect("poll result");
        let event = result["events"]
            .as_array()
            .expect("event rows")
            .iter()
            .find(|event| event["event_type"] == LXMF_TOPIC_PUBLISHED_EVENT_TYPE)
            .expect("topic publication event");
        assert!(event.get("peer_id").is_none());
        assert!(event["payload"].get("publisher_id").is_none());
        assert!(event["payload"].get("publisher_identity").is_none());
        let finding = inspect_external_topic_event_contract(
            event["event_type"].as_str().expect("event type"),
            event.get("peer_id").and_then(serde_json::Value::as_str),
            &event["payload"],
        );
        assert_eq!(
            finding.state,
            LxmfTopicEventContractState::PublisherAuthenticationAbsent
        );
        assert!(!finding.admission_allowed);

        let telemetry = daemon
            .handle_rpc(request(
                5,
                "sdk_telemetry_query_v2",
                serde_json::json!({ "topic_id": topic_id, "limit": 8 }),
            ))
            .expect("query topic telemetry");
        assert!(telemetry.error.is_none());
        let telemetry = telemetry.result.expect("telemetry result");
        let points = telemetry["points"].as_array().expect("telemetry points");
        assert_eq!(points.len(), 1);
        assert!(points[0]["tags"].get("peer_id").is_none());
        assert!(points[0].get("publisher_id").is_none());
    }
}
