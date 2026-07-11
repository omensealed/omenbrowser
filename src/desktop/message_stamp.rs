use crate::messaging::MessageSummary;

use super::format_epoch_secs;

pub(crate) fn lxmf_message_compact_stamp_status(message: &MessageSummary) -> Option<String> {
    if message.incoming {
        return None;
    }
    if matches!(
        message
            .fields
            .get("native_lxmf_reply_ticket_used")
            .map(String::as_str),
        Some("true")
    ) || matches!(
        message
            .fields
            .get("native_lxmf_stamp_state")
            .map(String::as_str),
        Some("ticket_stamp")
    ) {
        return Some("stamp: reply ticket".into());
    }
    if matches!(
        message
            .fields
            .get("native_lxmf_stamp_state")
            .map(String::as_str),
        Some("direct_stamp")
    ) {
        return Some(match message.fields.get("native_lxmf_direct_stamp_cost") {
            Some(cost) => format!("stamp: direct cost {cost}"),
            None => "stamp: direct cost".into(),
        });
    }
    if let Some(cost) = message.fields.get("native_lxmf_propagation_stamp_cost") {
        return Some(format!("stamp: propagation cost {cost}"));
    }
    if matches!(
        message
            .fields
            .get("native_lxmf_reply_ticket_offered")
            .map(String::as_str),
        Some("true")
    ) {
        return Some("ticket: reply ticket offered".into());
    }
    None
}

pub(crate) fn lxmf_stamp_status_lines(message: &MessageSummary) -> Vec<String> {
    let mut lines = Vec::new();
    if matches!(
        message
            .fields
            .get("native_lxmf_reply_ticket_offered")
            .map(String::as_str),
        Some("true")
    ) {
        lines.push("ticket: reply ticket offered".into());
    }
    if matches!(
        message
            .fields
            .get("native_lxmf_reply_ticket_used")
            .map(String::as_str),
        Some("true")
    ) {
        lines.push("ticket: remembered reply ticket used".into());
    }
    if let Some(stamp_state) = message.fields.get("native_lxmf_stamp_state") {
        lines.push(format!("stamp: {}", lxmf_stamp_state_label(stamp_state)));
        if stamp_state == "direct_stamp" {
            push_lxmf_stamp_cost_line(
                &mut lines,
                "direct stamp",
                message.fields.get("native_lxmf_direct_stamp_cost"),
                message.fields.get("native_lxmf_direct_stamp_value"),
                message.fields.get("native_lxmf_direct_stamp_attempts"),
            );
        }
    }
    if message
        .fields
        .contains_key("native_lxmf_propagation_stamp_cost")
    {
        push_lxmf_stamp_cost_line(
            &mut lines,
            "propagation stamp",
            message.fields.get("native_lxmf_propagation_stamp_cost"),
            message.fields.get("native_lxmf_propagation_stamp_value"),
            message.fields.get("native_lxmf_propagation_stamp_attempts"),
        );
    }
    if let Some(ticket_state) = message.fields.get("native_lxmf_reply_ticket_state") {
        lines.push(format!("reply ticket: {ticket_state}"));
    }
    if let Some(expires) = message
        .fields
        .get("native_lxmf_reply_ticket_expires")
        .and_then(|value| value.parse::<f64>().ok())
    {
        lines.push(format!(
            "reply ticket expires: {}",
            format_epoch_secs(expires)
        ));
    }
    lines
}

fn push_lxmf_stamp_cost_line(
    lines: &mut Vec<String>,
    label: &str,
    cost: Option<&String>,
    value: Option<&String>,
    attempts: Option<&String>,
) {
    match (cost, value, attempts) {
        (Some(cost), Some(value), Some(attempts)) => {
            lines.push(format!(
                "{label}: cost {cost}, value {value}, attempts {attempts}"
            ));
        }
        (Some(cost), Some(value), None) => {
            lines.push(format!("{label}: cost {cost}, value {value}"));
        }
        (Some(cost), None, _) => {
            lines.push(format!("{label}: cost {cost}"));
        }
        _ => {}
    }
}

fn lxmf_stamp_state_label(state: &str) -> &str {
    match state {
        "ticket_stamp" => "reply ticket",
        "direct_stamp" => "direct cost stamp",
        "propagation_stamp" => "propagation cost stamp",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::messaging::{MessageSummary, TransportMethod};

    use super::*;

    fn message_with_fields(fields: BTreeMap<String, String>) -> MessageSummary {
        MessageSummary {
            peer_hash: "peer".into(),
            peer_label: "Peer".into(),
            title: "Subject".into(),
            content: "Body".into(),
            timestamp: 1.0,
            transport_method: TransportMethod::Direct,
            delivered: false,
            failed: false,
            incoming: false,
            unread: false,
            message_id: Some("packet-1".into()),
            fields,
            attachments: Vec::new(),
        }
    }

    #[test]
    fn lxmf_stamp_status_lines_show_ticket_evidence_without_state() {
        let message = message_with_fields(BTreeMap::from([
            ("native_lxmf_reply_ticket_offered".into(), "true".into()),
            ("native_lxmf_reply_ticket_used".into(), "true".into()),
            ("native_lxmf_stamp_state".into(), "ticket_stamp".into()),
            ("native_lxmf_reply_ticket_state".into(), "valid".into()),
            (
                "native_lxmf_reply_ticket_expires".into(),
                "1782921166.557".into(),
            ),
        ]));

        let lines = lxmf_stamp_status_lines(&message);

        assert!(lines
            .iter()
            .any(|line| line == "ticket: reply ticket offered"));
        assert!(lines
            .iter()
            .any(|line| line == "ticket: remembered reply ticket used"));
        assert!(lines.iter().any(|line| line == "stamp: reply ticket"));
        assert!(lines.iter().any(|line| line == "reply ticket: valid"));
        assert!(lines
            .iter()
            .any(|line| line.starts_with("reply ticket expires: ")));
    }

    #[test]
    fn lxmf_stamp_status_lines_show_ticket_summary() {
        let message = message_with_fields(BTreeMap::from([
            ("native_lxmf_reply_ticket_used".into(), "true".into()),
            ("native_lxmf_stamp_state".into(), "ticket_stamp".into()),
        ]));

        let lines = lxmf_stamp_status_lines(&message);

        assert_eq!(lines[0], "ticket: remembered reply ticket used");
        assert_eq!(lines[1], "stamp: reply ticket");
    }

    #[test]
    fn lxmf_message_compact_stamp_status_summarizes_reply_ticket() {
        let message = message_with_fields(BTreeMap::from([
            ("native_lxmf_reply_ticket_used".into(), "true".into()),
            ("native_lxmf_stamp_state".into(), "ticket_stamp".into()),
        ]));

        assert_eq!(
            lxmf_message_compact_stamp_status(&message).as_deref(),
            Some("stamp: reply ticket")
        );
    }

    #[test]
    fn lxmf_message_compact_stamp_status_summarizes_direct_cost() {
        let message = message_with_fields(BTreeMap::from([
            ("native_lxmf_stamp_state".into(), "direct_stamp".into()),
            ("native_lxmf_direct_stamp_cost".into(), "8".into()),
        ]));

        assert_eq!(
            lxmf_message_compact_stamp_status(&message).as_deref(),
            Some("stamp: direct cost 8")
        );
    }

    #[test]
    fn lxmf_message_compact_stamp_status_summarizes_propagation_cost() {
        let message = message_with_fields(BTreeMap::from([(
            "native_lxmf_propagation_stamp_cost".into(),
            "16".into(),
        )]));

        assert_eq!(
            lxmf_message_compact_stamp_status(&message).as_deref(),
            Some("stamp: propagation cost 16")
        );
    }

    #[test]
    fn lxmf_stamp_status_lines_show_direct_stamp_cost_evidence() {
        let message = message_with_fields(BTreeMap::from([
            ("native_lxmf_stamp_state".into(), "direct_stamp".into()),
            ("native_lxmf_direct_stamp_cost".into(), "8".into()),
            ("native_lxmf_direct_stamp_value".into(), "10".into()),
            ("native_lxmf_direct_stamp_attempts".into(), "42".into()),
        ]));

        let lines = lxmf_stamp_status_lines(&message);

        assert!(lines.iter().any(|line| line == "stamp: direct cost stamp"));
        assert!(lines
            .iter()
            .any(|line| line == "direct stamp: cost 8, value 10, attempts 42"));
    }

    #[test]
    fn lxmf_stamp_status_lines_show_propagation_stamp_evidence() {
        let message = message_with_fields(BTreeMap::from([
            ("native_lxmf_propagation_stamp_cost".into(), "16".into()),
            ("native_lxmf_propagation_stamp_value".into(), "17".into()),
            (
                "native_lxmf_propagation_stamp_attempts".into(),
                "654".into(),
            ),
        ]));

        let lines = lxmf_stamp_status_lines(&message);

        assert_eq!(
            lines[0],
            "propagation stamp: cost 16, value 17, attempts 654"
        );
    }
}
