# 11 — Porting Checklist

Use this as the implementation checklist for maintainers.

## Phase 0 — Scaffold

- [ ] Create Cargo workspace.
- [ ] Add crates or modules for core, renderer, runtime, services, UI, plugins.
- [ ] Add `tracing` logging.
- [ ] Add app error types.
- [ ] Add README with build/run commands.
- [ ] Add mock mode CLI option.
- [ ] Add docs from this folder.

## Phase 1 — Core models

- [ ] Port identity profile.
- [ ] Port runtime status.
- [ ] Port browser page/download/address.
- [ ] Port message and attachment summaries.
- [ ] Port conversation thread.
- [ ] Port directory entry/trust/delivery enums.
- [ ] Port plugin manifest.
- [ ] Port interface profile.
- [ ] Add browser tab state.
- [ ] Add conversation tab state.
- [ ] Add global app state.
- [ ] Add serialization tests.

## Phase 2 — Paths/settings/identity

- [ ] Implement `AppPaths`.
- [ ] Implement directory creation.
- [ ] Implement settings defaults.
- [ ] Implement robust settings load/save.
- [ ] Backup corrupted settings.
- [ ] Implement create managed identity.
- [ ] Implement attach existing identity.
- [ ] Implement import identity copy.
- [ ] Implement export backup.
- [ ] Implement safe download path helper.
- [ ] Add tests.

## Phase 3 — Renderer

- [ ] Port parser state/style model.
- [ ] Parse plain Micron.
- [ ] Parse colors/styles/reset.
- [ ] Parse alignment.
- [ ] Parse links.
- [ ] Parse request links/fields.
- [ ] Parse controls.
- [ ] Render fixed-width cell rows.
- [ ] Preserve half-block art.
- [ ] Add focus/hit metadata.
- [ ] Add snapshot fixtures.
- [ ] Port MicronPlus transform as built-in module.
- [ ] Add tests.

## Phase 4 — Mock runtime

- [ ] Define `RuntimeAdapter` trait.
- [ ] Define runtime event channel.
- [ ] Implement mock status.
- [ ] Implement mock page fetch.
- [ ] Implement mock download.
- [ ] Implement mock messages.
- [ ] Implement mock direct/propagated send.
- [ ] Implement mock directory candidates.
- [ ] Implement mock propagation status.
- [ ] Add tests.

## Phase 5 — Browser service

- [ ] Implement address parsing.
- [ ] Implement relative URL resolution.
- [ ] Implement cache.
- [ ] Implement open/reload/back/forward.
- [ ] Implement field state.
- [ ] Implement link activation.
- [ ] Implement download.
- [ ] Implement partial descriptor parsing.
- [ ] Implement partial composition.
- [ ] Add per-tab generation cancellation.
- [ ] Add tests.

## Phase 6 — Messaging and directory

- [ ] Implement message store.
- [ ] Implement messaging service.
- [ ] Implement outbound status update.
- [ ] Implement pending reconciliation.
- [ ] Implement directory service.
- [ ] Implement trust/saved/delivery/identify state.
- [ ] Implement filters and sorting.
- [ ] Add tests.

## Phase 7 — Interface config and diagnostics

- [ ] Implement interface profile service.
- [ ] Render managed Reticulum config.
- [ ] Add gateway presets.
- [ ] Add I2P router detection if desired.
- [ ] Implement diagnostics snapshot.
- [ ] Implement redacted diagnostics export.
- [ ] Add tests.

## Phase 8 — UI shell

- [ ] Initialize terminal.
- [ ] Draw top-level sections.
- [ ] Draw status bar.
- [ ] Add input event routing.
- [ ] Add command shortcuts.
- [ ] Add log buffer.
- [ ] Add tick handling.

## Phase 9 — Browser workspace UI

- [ ] Add browser tab bar.
- [ ] Add new/close/duplicate tab.
- [ ] Add address bar.
- [ ] Add browser controls.
- [ ] Add Micron view widget.
- [ ] Add scroll/focus/link activation.
- [ ] Add load cancellation.
- [ ] Add partial refresh indicators.
- [ ] Add independent history per tab.

## Phase 10 — Messages workspace UI

- [ ] Add thread list.
- [ ] Add conversation tab bar.
- [ ] Add conversation view.
- [ ] Add composer.
- [ ] Add direct/propagated send controls.
- [ ] Add attachment controls.
- [ ] Add unread indicators.
- [ ] Integrate directory peer action.

## Phase 11 — Remaining panels

- [ ] Directory panel.
- [ ] Interfaces panel.
- [ ] Settings panel.
- [ ] Diagnostics panel.
- [ ] Logs panel.
- [ ] Plugins panel.

## Phase 12 — Reticulum/LXMF bridge

- [ ] Define bridge protocol.
- [ ] Implement process/socket management.
- [ ] Implement status.
- [ ] Implement attach identity.
- [ ] Implement page fetch.
- [ ] Implement downloads.
- [ ] Implement message send/list.
- [ ] Implement events.
- [ ] Implement path request/warm.
- [ ] Implement propagation sync/status.
- [ ] Add failure recovery.
- [ ] Add integration tests where possible.

## Phase 13 — Packaging

- [ ] Linux package/script.
- [ ] Windows package/script.
- [ ] macOS package/script.
- [ ] Include resources.
- [ ] Document app data locations.
- [ ] Smoke test fresh install.

## Final acceptance

- [ ] `cargo fmt --all` passes.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes.
- [ ] `cargo test --workspace` passes.
- [ ] Mock mode works without Reticulum.
- [ ] Multiple browser tabs work.
- [ ] Multiple conversation tabs work.
- [ ] Micron half-block fixtures render correctly.
- [ ] Settings/identity safety is tested.
- [ ] Live runtime bridge is isolated behind trait.
- [ ] Docs match implementation.
