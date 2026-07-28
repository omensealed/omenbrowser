# OMENchat slow-mode GUI qualification

Date: 2026-07-28

Baseline: `release/v0.9.6-4` at `d3fb3d0`, plus this qualification unit

Verdict: the explicit current/current qualification build presents a live
slow-mode room delta truthfully in the native Linux Iced GUI, admits the first
room message, rejects an immediate second message with the typed
`SlowModeActive` result, and preserves the exact rejected draft. This closes
the local GUI observation gate. It is not Windows/macOS or physical-GPU
evidence.

## Isolated harness

Run from the repository root:

```bash
bash scripts/run-omenchat-slow-mode-gui-qualification.sh \
  --evidence /tmp/omenchat-slow-mode-gui-evidence
```

The harness builds the root and standalone server independently with their
non-product `omenchat-slow-mode-qualification` features. It creates fresh
temporary browser and server roots, generates separate identities, configures
one no-IFAC loopback TCP client, and launches the desktop at 1400x900 under
Xvfb/i3 with software rendering. It never reads the maintainer's identity,
interface, message, cache, or server state.

The qualification-only desktop target is a non-secret
`omenchat://<destination>` URI. The target is retained by the desktop and
consumed exactly once from the existing internal-event subscription after the
normal runtime status becomes connected. It uses the ordinary validated
quick-open path. There is no sleep-based startup guess, new polling
subscription, production setting, worker, queue, or retry loop.

The server's existing one-shot qualification transition commits lobby slow
mode at 30 seconds after the connected client negotiates and joins. The
existing bounded live loop sends the authoritative room delta.

## Automated assertions and observed behavior

The automated run requires:

- the server log to record the committed 30-second transition;
- the first GUI submission to reach SQLite as
  `qualification-first-message`;
- the immediate second GUI submission not to create a second room-message
  row;
- the composer clipboard value after rejection to equal
  `qualification-second-message`; and
- Alt-F4 to complete the normal ordered desktop shutdown.

The final isolated database observation was:

```json
{
  "room_message_count": 1,
  "messages": [[1, "qualification-first-message"]]
}
```

The connected and rejected-state screenshots were inspected locally. They
showed the static `Slow mode · 30s` indicator, first-message acceptance, and
the server slow-mode rejection text. The input was horizontally scrolled
because the test uses the deliberately narrow three-pane preset, so the
harness additionally selects and copies the composer content and compares the
exact bytes. This is independent proof that the rejected draft remained
recoverable rather than being inferred from the visible suffix. No OCR tool
was installed, so the text presentation remains an explicitly recorded visual
observation rather than a machine text assertion.

Screenshots and isolated logs remain under the caller-selected temporary
evidence directory; binary evidence is not committed.

## Validation

Completed locally:

```text
bash -n scripts/run-omenchat-slow-mode-gui-qualification.sh
passed

shellcheck scripts/run-omenchat-slow-mode-gui-qualification.sh
passed

cargo test --locked --no-default-features \
  --features desktop-product,omenchat-slow-mode-qualification \
  qualification_target_waits_for_runtime_then_uses_normal_open_path \
  --lib -- --nocapture
1 passed

bash scripts/run-omenchat-slow-mode-gui-qualification.sh \
  --evidence /tmp/omenchat-slow-mode-gui-final
passed
```

The script itself performs the exact draft, database, runtime transition, and
orderly-shutdown assertions. Its screenshots were also inspected locally.

## Compatibility, resource, and rollback impact

Canonical product aliases still exclude the qualification feature. No wire
shape, capability name, error number, schema, production feature, identity
path, or persistent product configuration changed. The UI hook contains one
bounded `Option<String>` only in the qualification build and is consumed once;
it owns no payload cache or background task.

Rollback is a source revert of the harness and feature-gated hook. No stored
data needs migration or repair.

## Limitations and next gate

- Xvfb/software rendering proves native Linux Iced layout and input routing,
  not physical GPU behavior.
- Native Windows and macOS presentation remain part of release-candidate
  packaging qualification.
- This run covers one connected member and one room. Moderator bypass and
  expiry readmission remain covered by the real-Link process/session gates.
- CPU, RSS, link, queue, and shutdown measurements remain the next local
  slow-mode activation gate.

No hosted CI, Python interoperability, package build, public-network peer,
physical interface, or physical-GPU result is claimed by this unit.
