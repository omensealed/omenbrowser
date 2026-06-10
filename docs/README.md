# OMENbrowser_rs Documentation

This directory contains public documentation for OMENbrowser_rs, OMENchat, and
`omenchatd`.

Start here:

- [`../README.md`](../README.md) - build, run, package, and safety overview.
- [`../TESTERS.md`](../TESTERS.md) - shortest private-alpha tester sheet.
- [`27-alpha-test-runbook.md`](27-alpha-test-runbook.md) - practical alpha test
  flow with isolated roots.
- [`28-alpha-handoff.md`](28-alpha-handoff.md) - concise alpha handoff note.
- [`25-omenchat-plugin-server-plan.md`](25-omenchat-plugin-server-plan.md) and
  [`26-omenchat-protocol-v0.1.md`](26-omenchat-protocol-v0.1.md) - OMENchat
  client/server design and protocol.

Architecture and implementation reference:

- `00` through `11` describe the original Rust port plan and module boundaries.
- `12` through `22` document the live runtime, UI, plugin, security, testing,
  and release work.
- `24` is a native live-network validation runbook.
- `RUST_RETICULUM_LXMF.md` captures Reticulum/LXMF integration notes.

Not included in the public repository:

- local agent handoff notes;
- implementation scratch logs;
- local source archives;
- runtime data, identities, caches, debug output, and package artifacts.
