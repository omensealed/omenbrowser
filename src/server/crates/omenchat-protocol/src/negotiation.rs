use std::collections::BTreeSet;

use crate::{
    ClientInstanceId, FrameBody, FrameValue, DURABLE_MUTATION_CAPABILITY, PROTOCOL_NAME,
    REACTIONS_CAPABILITY, REPLY_MENTIONS_CAPABILITY,
};

pub const SESSION_CAPABILITY_MAX_ITEMS: usize = 64;
pub const SESSION_CAPABILITY_MAX_BYTES: usize = 128;

const SESSION_OPEN_PROTOCOL_INDEX: usize = 0;
const SESSION_OPEN_LXMF_DESTINATION_INDEX: usize = 2;
const SESSION_OPEN_CAPABILITIES_INDEX: usize = 3;
const SESSION_OPEN_CLIENT_INSTANCE_INDEX: usize = 4;
const SESSION_ACCEPT_CAPABILITIES_INDEX: usize = 6;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionOpenNegotiation {
    pub requested_capabilities: Vec<String>,
    pub client_instance_id: Option<ClientInstanceId>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionAcceptNegotiation {
    pub accepted_capabilities: Vec<String>,
}

pub fn parse_session_open_negotiation(
    body: &FrameBody,
) -> Result<Option<SessionOpenNegotiation>, SessionNegotiationError> {
    let FrameBody::Fields(fields) = body else {
        return Ok(None);
    };
    let capabilities = fields.get(SESSION_OPEN_CAPABILITIES_INDEX);
    let client_instance = fields.get(SESSION_OPEN_CLIENT_INSTANCE_INDEX);
    if capabilities.is_none() && client_instance.is_none() {
        return Ok(None);
    }
    validate_protocol_field(fields)?;
    let requested_capabilities = parse_capabilities(capabilities.ok_or(
        SessionNegotiationError::Malformed("client instance requires a capability array"),
    )?)?;
    let client_instance_id = match client_instance {
        None | Some(FrameValue::Nil) => None,
        Some(FrameValue::Bytes(bytes)) => Some(ClientInstanceId::try_from(bytes.as_slice())?),
        Some(_) => {
            return Err(SessionNegotiationError::Malformed(
                "client instance id must be binary or nil",
            ))
        }
    };
    if requested_capabilities
        .iter()
        .any(|capability| capability == DURABLE_MUTATION_CAPABILITY)
        && client_instance_id.is_none()
    {
        return Err(SessionNegotiationError::Malformed(
            "durable mutation capability requires a client instance id",
        ));
    }
    Ok(Some(SessionOpenNegotiation {
        requested_capabilities,
        client_instance_id,
    }))
}

pub fn parse_session_accept_negotiation(
    body: &FrameBody,
) -> Result<Option<SessionAcceptNegotiation>, SessionNegotiationError> {
    let FrameBody::Fields(fields) = body else {
        return Ok(None);
    };
    let Some(capabilities) = fields.get(SESSION_ACCEPT_CAPABILITIES_INDEX) else {
        return Ok(None);
    };
    validate_protocol_field(fields)?;
    Ok(Some(SessionAcceptNegotiation {
        accepted_capabilities: parse_capabilities(capabilities)?,
    }))
}

pub fn with_session_open_negotiation(
    body: FrameBody,
    negotiation: &SessionOpenNegotiation,
) -> Result<FrameBody, SessionNegotiationError> {
    validate_capability_list(&negotiation.requested_capabilities)?;
    if negotiation
        .requested_capabilities
        .iter()
        .any(|capability| capability == DURABLE_MUTATION_CAPABILITY)
        && negotiation.client_instance_id.is_none()
    {
        return Err(SessionNegotiationError::Malformed(
            "durable mutation capability requires a client instance id",
        ));
    }

    let mut fields = match body {
        FrameBody::Empty => vec![FrameValue::String(PROTOCOL_NAME.into())],
        FrameBody::Text(display_name) => vec![
            FrameValue::String(PROTOCOL_NAME.into()),
            FrameValue::String(display_name),
        ],
        FrameBody::Fields(fields) => fields,
    };
    if fields.len() > SESSION_OPEN_CAPABILITIES_INDEX {
        return Err(SessionNegotiationError::Malformed(
            "session open already contains negotiation fields",
        ));
    }
    if fields.is_empty() {
        fields.push(FrameValue::String(PROTOCOL_NAME.into()));
    } else if let Some(protocol) = fields.get(SESSION_OPEN_PROTOCOL_INDEX) {
        if !matches!(protocol, FrameValue::String(value) if value == PROTOCOL_NAME) {
            return Err(SessionNegotiationError::ProtocolName);
        }
    }
    while fields.len() <= SESSION_OPEN_LXMF_DESTINATION_INDEX {
        fields.push(FrameValue::Nil);
    }
    fields.push(capabilities_value(&negotiation.requested_capabilities));
    fields.push(
        negotiation
            .client_instance_id
            .map(|id| FrameValue::Bytes(id.into_bytes().to_vec()))
            .unwrap_or(FrameValue::Nil),
    );
    Ok(FrameBody::Fields(fields))
}

pub fn with_session_accept_negotiation(
    body: FrameBody,
    negotiation: &SessionAcceptNegotiation,
) -> Result<FrameBody, SessionNegotiationError> {
    validate_capability_list(&negotiation.accepted_capabilities)?;
    let FrameBody::Fields(mut fields) = body else {
        return Err(SessionNegotiationError::Malformed(
            "session accept negotiation requires a fields body",
        ));
    };
    if fields.len() > SESSION_ACCEPT_CAPABILITIES_INDEX {
        return Err(SessionNegotiationError::Malformed(
            "session accept already contains negotiation fields",
        ));
    }
    validate_protocol_field(&fields)?;
    while fields.len() < SESSION_ACCEPT_CAPABILITIES_INDEX {
        fields.push(FrameValue::Nil);
    }
    fields.push(capabilities_value(&negotiation.accepted_capabilities));
    Ok(FrameBody::Fields(fields))
}

fn validate_protocol_field(fields: &[FrameValue]) -> Result<(), SessionNegotiationError> {
    if matches!(
        fields.get(SESSION_OPEN_PROTOCOL_INDEX),
        Some(FrameValue::String(protocol)) if protocol == PROTOCOL_NAME
    ) {
        Ok(())
    } else {
        Err(SessionNegotiationError::ProtocolName)
    }
}

fn parse_capabilities(value: &FrameValue) -> Result<Vec<String>, SessionNegotiationError> {
    let FrameValue::Array(values) = value else {
        return Err(SessionNegotiationError::Malformed(
            "capabilities must be an array",
        ));
    };
    if values.len() > SESSION_CAPABILITY_MAX_ITEMS {
        return Err(SessionNegotiationError::CapabilityCount);
    }
    let mut capabilities = Vec::with_capacity(values.len());
    for value in values {
        let FrameValue::String(capability) = value else {
            return Err(SessionNegotiationError::Malformed(
                "capability names must be strings",
            ));
        };
        validate_capability_name(capability)?;
        capabilities.push(capability.clone());
    }
    validate_capability_list(&capabilities)?;
    Ok(capabilities)
}

fn validate_capability_list(capabilities: &[String]) -> Result<(), SessionNegotiationError> {
    if capabilities.len() > SESSION_CAPABILITY_MAX_ITEMS {
        return Err(SessionNegotiationError::CapabilityCount);
    }
    let mut unique = BTreeSet::new();
    for capability in capabilities {
        validate_capability_name(capability)?;
        if !unique.insert(capability.as_str()) {
            return Err(SessionNegotiationError::DuplicateCapability);
        }
    }
    if unique.contains(REPLY_MENTIONS_CAPABILITY) && !unique.contains(DURABLE_MUTATION_CAPABILITY) {
        return Err(SessionNegotiationError::MissingCapabilityDependency);
    }
    if unique.contains(REACTIONS_CAPABILITY) && !unique.contains(DURABLE_MUTATION_CAPABILITY) {
        return Err(SessionNegotiationError::MissingReactionsDependency);
    }
    Ok(())
}

fn validate_capability_name(capability: &str) -> Result<(), SessionNegotiationError> {
    if capability.is_empty()
        || capability.len() > SESSION_CAPABILITY_MAX_BYTES
        || !capability
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(SessionNegotiationError::CapabilityName);
    }
    Ok(())
}

fn capabilities_value(capabilities: &[String]) -> FrameValue {
    FrameValue::Array(
        capabilities
            .iter()
            .cloned()
            .map(FrameValue::String)
            .collect(),
    )
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SessionNegotiationError {
    #[error("session negotiation protocol name does not match {PROTOCOL_NAME}")]
    ProtocolName,
    #[error("invalid session negotiation: {0}")]
    Malformed(&'static str),
    #[error("session capability count exceeds {SESSION_CAPABILITY_MAX_ITEMS}")]
    CapabilityCount,
    #[error("session capability name is empty, invalid, or exceeds {SESSION_CAPABILITY_MAX_BYTES} bytes")]
    CapabilityName,
    #[error("session capability names must be unique")]
    DuplicateCapability,
    #[error("{REPLY_MENTIONS_CAPABILITY} requires {DURABLE_MUTATION_CAPABILITY}")]
    MissingCapabilityDependency,
    #[error("{REACTIONS_CAPABILITY} requires {DURABLE_MUTATION_CAPABILITY}")]
    MissingReactionsDependency,
    #[error(transparent)]
    Durable(#[from] crate::DurableMutationError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn current_session_open() -> FrameBody {
        FrameBody::Fields(vec![
            FrameValue::String(PROTOCOL_NAME.into()),
            FrameValue::String("alice".into()),
            FrameValue::String("lxmf-destination".into()),
        ])
    }

    fn current_session_accept() -> FrameBody {
        FrameBody::Fields(vec![
            FrameValue::String(PROTOCOL_NAME.into()),
            FrameValue::Array(Vec::new()),
            FrameValue::Nil,
            FrameValue::U64(0),
            FrameValue::U64(30),
            FrameValue::U64(512 * 1024),
        ])
    }

    #[test]
    fn legacy_handshakes_have_no_implicit_capabilities() {
        assert_eq!(parse_session_open_negotiation(&FrameBody::Empty), Ok(None));
        assert_eq!(
            parse_session_open_negotiation(&current_session_open()),
            Ok(None)
        );
        assert_eq!(
            parse_session_accept_negotiation(&current_session_accept()),
            Ok(None)
        );
    }

    #[test]
    fn session_open_extension_preserves_existing_lxmf_field() {
        let negotiation = SessionOpenNegotiation {
            requested_capabilities: vec![DURABLE_MUTATION_CAPABILITY.into()],
            client_instance_id: Some(ClientInstanceId::new([7; 16])),
        };
        let body = with_session_open_negotiation(current_session_open(), &negotiation)
            .expect("extend session open");
        let FrameBody::Fields(fields) = &body else {
            panic!("fields body");
        };
        assert_eq!(
            fields.get(SESSION_OPEN_LXMF_DESTINATION_INDEX),
            Some(&FrameValue::String("lxmf-destination".into()))
        );
        assert_eq!(parse_session_open_negotiation(&body), Ok(Some(negotiation)));
    }

    #[test]
    fn session_accept_extension_requires_explicit_acceptance() {
        let negotiation = SessionAcceptNegotiation {
            accepted_capabilities: vec![DURABLE_MUTATION_CAPABILITY.into()],
        };
        let body = with_session_accept_negotiation(current_session_accept(), &negotiation)
            .expect("extend session accept");
        assert_eq!(
            parse_session_accept_negotiation(&body),
            Ok(Some(negotiation))
        );
    }

    #[test]
    fn durable_request_requires_exact_client_instance_and_bounded_unique_names() {
        let no_instance = FrameBody::Fields(vec![
            FrameValue::String(PROTOCOL_NAME.into()),
            FrameValue::Nil,
            FrameValue::Nil,
            FrameValue::Array(vec![FrameValue::String(DURABLE_MUTATION_CAPABILITY.into())]),
        ]);
        assert!(matches!(
            parse_session_open_negotiation(&no_instance),
            Err(SessionNegotiationError::Malformed(_))
        ));

        let wrong_instance = FrameBody::Fields(vec![
            FrameValue::String(PROTOCOL_NAME.into()),
            FrameValue::Nil,
            FrameValue::Nil,
            FrameValue::Array(vec![FrameValue::String(DURABLE_MUTATION_CAPABILITY.into())]),
            FrameValue::Bytes(vec![1; 15]),
        ]);
        assert!(matches!(
            parse_session_open_negotiation(&wrong_instance),
            Err(SessionNegotiationError::Durable(_))
        ));

        let duplicate = SessionAcceptNegotiation {
            accepted_capabilities: vec!["a".into(), "a".into()],
        };
        assert_eq!(
            with_session_accept_negotiation(current_session_accept(), &duplicate),
            Err(SessionNegotiationError::DuplicateCapability)
        );
    }

    #[test]
    fn negotiation_rejects_oversized_or_invalid_capability_shapes() {
        let too_many = FrameValue::Array(
            (0..=SESSION_CAPABILITY_MAX_ITEMS)
                .map(|index| FrameValue::String(format!("cap-{index}")))
                .collect(),
        );
        let body = FrameBody::Fields(vec![
            FrameValue::String(PROTOCOL_NAME.into()),
            FrameValue::Nil,
            FrameValue::Nil,
            too_many,
        ]);
        assert_eq!(
            parse_session_open_negotiation(&body),
            Err(SessionNegotiationError::CapabilityCount)
        );

        let invalid = SessionAcceptNegotiation {
            accepted_capabilities: vec!["not a capability".into()],
        };
        assert_eq!(
            with_session_accept_negotiation(current_session_accept(), &invalid),
            Err(SessionNegotiationError::CapabilityName)
        );
    }

    #[test]
    fn reply_mentions_capability_requires_durable_mutations() {
        let missing_base = SessionAcceptNegotiation {
            accepted_capabilities: vec![REPLY_MENTIONS_CAPABILITY.into()],
        };
        assert_eq!(
            with_session_accept_negotiation(current_session_accept(), &missing_base),
            Err(SessionNegotiationError::MissingCapabilityDependency)
        );

        let complete = SessionAcceptNegotiation {
            accepted_capabilities: vec![
                DURABLE_MUTATION_CAPABILITY.into(),
                REPLY_MENTIONS_CAPABILITY.into(),
            ],
        };
        let body = with_session_accept_negotiation(current_session_accept(), &complete)
            .expect("dependent capability set");
        assert_eq!(parse_session_accept_negotiation(&body), Ok(Some(complete)));
    }

    #[test]
    fn reactions_capability_requires_durable_mutations() {
        let missing_base = SessionAcceptNegotiation {
            accepted_capabilities: vec![REACTIONS_CAPABILITY.into()],
        };
        assert_eq!(
            with_session_accept_negotiation(current_session_accept(), &missing_base),
            Err(SessionNegotiationError::MissingReactionsDependency)
        );

        let complete = SessionAcceptNegotiation {
            accepted_capabilities: vec![
                DURABLE_MUTATION_CAPABILITY.into(),
                REACTIONS_CAPABILITY.into(),
            ],
        };
        let body = with_session_accept_negotiation(current_session_accept(), &complete)
            .expect("dependent capability set");
        assert_eq!(parse_session_accept_negotiation(&body), Ok(Some(complete)));
    }
}
