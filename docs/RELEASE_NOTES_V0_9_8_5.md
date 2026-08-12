# OMENbrowser_rs and omenchatd v0.9.8-5 release notes

Status: final

Reticulum/LXMF crate train: exact official crates.io `0.9.8`.

## OMENchat desktop reliability and presentation

- Live reaction and message-revision updates remain visible without reopening
  a room, and reaction/edit/reply controls use a stable hover surface that does
  not move the timeline.
- Nickname-colour selection has an explicit, visible Apply action, initializes
  from the saved preference, and stays in a stable section above the user list.
- OMENchat pane and restore labels use the known directory/server name rather
  than a generic title where that metadata is available.
- Identity-scoped OMENchat storage now creates its exact managed parent before
  opening the database. A saved Browser + OMENchat workspace therefore restores
  without substituting a blank LXMF pane on a fresh identity-scoped root.
- OMENchat scrolling remains anchored like a chat during history, media, and
  attachment-view updates.

## Workspace and LXMF presentation

- Workspace presets and local-history search use compact toolbar controls.
  Local search is collapsed by default and has an explicit close control.
- Minimizing an LXMF conversation no longer leaves the local-history search
  panel occupying the workspace.
- An elapsed local LXMF receipt-observation window is shown as
  `Receipt window expired; peer delivery unconfirmed`, not as authoritative
  delivery failure. It does not use the red failure-card presentation. Genuine
  rejection and failure states remain distinct.

## Compatibility and rollback

- OMENchat wire protocol remains version 1, the shared protocol crate remains
  API version 0.2.0, and omenchatd schema remains 14.
- There is no database, configuration, cache, identity, destination, ticket,
  upload-content, message-format, or Reticulum-storage migration in this
  revision.
- No automatic retry, replay, primitive fallback, backend switch, or second
  dispatch was added. Receipt-window expiry remains explicitly uncertain.
- Routed multi-hop Resource attachments remain unqualified on upstream 0.9.8,
  and the independent maximum-UDP Resource sentinel remains visible.
- Rollback to v0.9.8-4 is binary-only because this revision changes no
  persistent schema or format.
