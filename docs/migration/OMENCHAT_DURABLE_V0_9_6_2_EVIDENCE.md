# OMENchat durable mutation v0.9.6-2 evidence

Baseline: `v0.9.6-1` (`7cbb470`)  
Target: `v0.9.6-2`  
Protocol: OMENchat v1 plus explicitly negotiated `durable-mutations-v1`

This matrix records deterministic repository evidence. It does not turn local
tests into a claim about physical radios, public Reticulum routes, or peers that
were not exercised. Long hosted jobs remain deferred to one bundled release
checkpoint.

| Boundary | Deterministic evidence | Result / limitation |
|---|---|---|
| Capability acceptance | `chat::live::tests::durable_session_activation_requires_acceptance_and_is_cleared_on_downgrade`; `session::tests::durable_capability_request_is_explicitly_accepted` | Explicit request and authenticated acceptance are required. Legacy, unsolicited, and downgraded sessions cannot send an envelope. |
| Persist before transport | `desktop::omenchat_mutations::tests::negotiated_room_send_persists_before_transport_and_persists_ack` | `prepared` commits before `sent_uncertain`, which commits before transport. Owner loss fails closed with the draft intact. |
| Lost response | `live::retry_safety_tests::committed_mutation_remains_uncertain_when_the_response_is_lost`; durable executor tests in `session.rs` | Legacy v1 remains uncertain. Negotiated durable replay retains one committed operation. |
| Replacement Link | `live::tests::durable_room_text_replay_on_replacement_link_uses_new_sequence_without_refanout` | Replay uses the new transient sequence for client correlation, retains the stored result body, and emits no second room event. |
| Client restart | `desktop::omenchat_mutations::tests::restart_recovery_is_identity_scoped_visible_and_never_transmits` | Recovery is identity/client-instance scoped, bounded, visible, and sends nothing automatically. |
| Server restart | `session::tests::durable_room_text_replays_after_server_restart_without_new_event` | Persistent replay returns the retained result under the retry sequence and leaves one room event. |
| Exact duplicate | `session::tests::durable_room_text_replays_exact_ack_without_rate_or_broadcast_repetition`; `live::tests::authenticated_durable_binding_routes_once_and_replays_original_ack` | No second rate charge, mutation, or fan-out. |
| Mutation conflict | `session::tests::durable_room_text_rejects_hash_conflict_and_malformed_hash_without_mutation`; `chat::live::tests::durable_terminal_errors_release_only_correlated_pending_echoes` | Different content under one durable identity returns 1013. Only a correlated response persists `conflict` and releases its optimistic echo. |
| Replay expiry | `store::durable_replay::tests::pruned_client_instance_stays_expired_after_restart`; terminal client test above | Retired client instances fail closed before execution, including after restart. Correlated 1014 persists `expired`. |
| Nonterminal errors | `chat::live::tests::nonterminal_or_uncorrelated_durable_errors_preserve_uncertain_work` | Store-busy and uncorrelated terminal-looking errors preserve uncertain state and do not retry. |
| Reconnect cleanup | `chat::live::tests::live_reconnect_removes_retired_durable_echo_and_requires_renegotiation`; `live_reconnect_releases_prior_link_transfer_state` | Link retirement removes only durable optimistic echoes, preserves legacy uncertain echoes, resets transient correlation, and requires capability renegotiation. |
| Retention bounds | durable replay and intent retention/capacity tests in `store/durable_replay.rs` and `chat/mutation_intents.rs` | Item, byte, per-identity, age, and incremental-pruning ceilings are deterministic. Explicit long measurement tests remain ignored by default. |
| Shutdown | `chat::mutation_intent_worker::tests::shutdown_drains_admitted_intents_before_joining`; `reticulum_live::tests::live_runtime_shutdown_is_idempotent_and_joins_owned_workers` | Admitted intent work drains before the named owner joins; server runtime shutdown is joined and idempotent. |
| Wire compatibility | shared protocol fixture tests and `protocol::codec::tests::v0_6_0_1_frame_fixtures_remain_bidirectionally_exact` | Legacy frames are unchanged. The envelope is sent only after negotiation. |
| Published v0.9.6-1 state reopen | `run-mixed-0-6-0-9-omenchat-history.sh` with immutable `7cbb470` | Locally passed all four v0.9.6-1/v0.9.6-2 reopen stages with metadata, order, content, and bidirectional writes preserved. |
| Published v0.9.6-1 live downgrade | `run-mixed-0-6-0-9-omenchat-live.sh` with immutable `7cbb470` | Local isolated loopback passed in both directions. The v0.9.6-1 client also passed orderly v0.9.6-2 server restart with stable destination, reused client state, new Link/session/join, and a post-restart echo. |

## Still required at the bundled release checkpoint

- Native CI on Linux, Windows, Intel macOS, and Apple Silicon macOS.
- Pinned/current Python Reticulum and LXMF interoperability workflows.
- Hosted repetition of the locally passing mixed-version OMENchat process
  smokes with v0.9.6-1 artifacts.
- Native package qualification, including both unsigned DMGs.
- Explicit retention/resource measurements and the documented soak commands.

These jobs are intentionally not dispatched for every small commit because
they are long-running and do not add useful evidence until the local durable
contract and release metadata are stable.
