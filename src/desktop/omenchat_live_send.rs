use super::{hex_bytes, DesktopApp};

impl DesktopApp {
    pub(in crate::desktop) fn send_omenchat_outgoing_frames(
        &mut self,
        link_id: [u8; 16],
        frames: Vec<Vec<u8>>,
    ) {
        if frames.is_empty() {
            return;
        }
        let runtime = self.app.runtime.clone();
        let frame_count = frames.len();
        self.app.status.task = format!("OMENchat sending {frame_count} frame(s)");
        tokio::spawn(async move {
            for frame in frames {
                let byte_len = frame.len();
                let frame_summary = Self::omenchat_frame_summary(&frame);
                match runtime.send_omenchat_frame(link_id, frame).await {
                    Ok(()) => {
                        if !Self::omenchat_frame_summary_is_heartbeat(&frame_summary) {
                            tracing::debug!(
                                link_id = %hex_bytes(&link_id),
                                bytes = byte_len,
                                frame = %frame_summary,
                                "OMENchat sent Link frame"
                            );
                        }
                    }
                    Err(error) => {
                        tracing::warn!(
                            link_id = %hex_bytes(&link_id),
                            bytes = byte_len,
                            frame = %frame_summary,
                            error = %error,
                            "OMENchat Link frame send failed"
                        );
                    }
                }
            }
        });
    }

    pub(in crate::desktop) fn send_omenchat_outgoing_resources(
        &mut self,
        link_id: [u8; 16],
        resources: Vec<(String, Vec<u8>)>,
    ) {
        if resources.is_empty() {
            return;
        }
        let runtime = self.app.runtime.clone();
        let count = resources.len();
        self.app.status.task = format!("OMENchat sending {count} resource(s)");
        tokio::spawn(async move {
            for (resource_id, payload) in resources {
                let byte_len = payload.len();
                match runtime
                    .send_omenchat_resource(link_id, resource_id.clone(), payload)
                    .await
                {
                    Ok(()) => tracing::debug!(
                        link_id = %hex_bytes(&link_id),
                        resource_id,
                        bytes = byte_len,
                        "OMENchat sent Link resource"
                    ),
                    Err(error) => tracing::warn!(
                        link_id = %hex_bytes(&link_id),
                        resource_id,
                        bytes = byte_len,
                        error = %error,
                        "OMENchat Link resource send failed"
                    ),
                }
            }
        });
    }

    pub(in crate::desktop) fn omenchat_frame_summary(frame: &[u8]) -> String {
        crate::chat::codec::decode_frame(frame)
            .map(|decoded| {
                let body = match &decoded.body {
                    crate::chat::protocol::FrameBody::Empty => "empty".to_string(),
                    crate::chat::protocol::FrameBody::Text(text) => {
                        format!("text:{}", text.len())
                    }
                    crate::chat::protocol::FrameBody::Fields(fields) => {
                        format!("fields:{}", fields.len())
                    }
                };
                format!(
                    "{:?} seq={} room={:?} body={}",
                    decoded.op, decoded.seq, decoded.room_id, body
                )
            })
            .unwrap_or_else(|error| format!("decode_error {error}"))
    }

    pub(in crate::desktop) fn omenchat_frame_summary_is_heartbeat(summary: &str) -> bool {
        summary.starts_with("Ping ") || summary.starts_with("Pong ")
    }
}
