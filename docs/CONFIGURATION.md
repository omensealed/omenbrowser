# Configuration

## Browser Storage

Default root:

```text
~/.config/OMENbrowser_rs/
```

Use `--app-root` for isolated testing:

```bash
omenbrowser_rs --desktop --app-root /tmp/omenbrowser-rs-test
```

Each root owns identities, Reticulum config/storage, messages, caches, plugin
state, and pane layout.

On Unix, the exact managed directory tree is created and repaired as `0700`,
and known sensitive managed files are `0600`; see
[`PRIVATE_STORAGE.md`](PRIVATE_STORAGE.md). This metadata-only policy does not
recursively chmod import/export trees or arbitrary custom ancestors. Other
platforms retain their native filesystem semantics.

When the native OMENchat client is compiled, desktop startup owns one random
16-byte client-instance identifier per active identity-scoped storage root at
`omenchat/client-instance-id`. It is created through a same-directory,
create-without-replacement atomic publication and stored in an owner-only file
inside an owner-only directory on Unix. Concurrent first starts converge on the
same published identifier. A wrong-size file, symbolic link, special file, or
permissive Unix mode fails closed and remains untouched; the application does
not silently regenerate it. The identifier is groundwork for negotiated
durable mutations, not an authentication secret or a capability by itself, and
the current release does not transmit it or advertise that capability.

Application settings are stored in `settings.json`. The file must be a regular,
non-symbolic-link file no larger than 8 MiB; the loader reads at most 8 MiB plus
one detection byte before JSON parsing. Missing settings use defaults, and a
malformed file within the limit retains the existing corruption-backup/default
recovery. The backup contains the exact bounded bytes already read, is written
through a unique same-directory staging file, and is owner-only on Unix; the
source path is not reopened after parsing fails. Backup publication is
synchronized and refuses destination collisions before defaults are returned.
At most the newest four regular corruption backups and 32 MiB are retained.
Retention inspects no more than 4,096 sibling entries and ignores symbolic links
and special files. It runs before and after publication: a crash in between can
leave at most one additional bounded backup, which the next recovery trims
before creating another. Scan saturation fails explicitly without publishing a
new backup. Oversized and unsafe paths fail explicitly and remain untouched.
Saving rejects output above the same limit before creating a temporary file, so
the application cannot deliberately publish settings its next startup refuses.
Accepted output is written to a unique create-new same-directory file, flushed
and synchronized, then atomically replaces the target through the platform
replacement primitive; Unix also synchronizes the parent directory. Temporary
files are owner-only on Unix. Existing symbolic-link and non-regular targets are
refused unchanged, and a pre-replacement failure preserves the previous bytes
and removes staging.

Before Serde creates owned settings values, the bounded raw bytes pass an
allocation-free structural scan backed by a fixed stack. JSON accepts at most
48 nested containers, 262,144 structural tokens, 8,192 items in any one array
or object, and 4 MiB of raw bytes in one string token. The container, depth,
and token ceilings sit above the corresponding retained-state limits below;
the raw-string ceiling independently bounds any one decoded setting. Together
they prevent a compact file from expanding into an excessive number of vectors,
maps, or values during deserialization. Full JSON grammar validation remains
Serde's responsibility.
Structural rejection uses the same exact-byte backup/default recovery as
malformed JSON, and save applies the same scan before staging.

After JSON parsing, settings are admitted as one retained-state unit rather
than partially restored. The persisted workspace accepts at most 128 browser
tabs, 128 conversation tabs, 4,096 bookmarks, 4,096 deleted-conversation
tombstones, and 256 pane descriptors. Each conversation draft retains at most
64 attachment paths. Trusted and enabled plugin-ID lists each accept 256
entries. Restored browser histories reuse the live-session limits of 512 URLs
and 1 MiB per tab; bookmark/tab URLs, titles, focused controls, and focused-link
fields reuse their browser/Micron limits. A pane-layout tree accepts at most
511 nodes and depth 32 with finite split ratios. Flattened future settings
accept 256 top-level fields, 4,096 items per container, 16,384 value nodes, and
depth 32. A syntactically valid file outside these limits follows the same
exact-byte corruption-backup/default recovery as malformed JSON; no subset is
restored. Save performs the same validation before serialization or staging.

The desktop Appearance settings include a persisted **Reduce motion** control.
It is off by default for compatibility. When enabled, animated media previews
are withheld from the Iced widget tree and shown as static images instead;
hidden panes use the same no-animation boundary.

Saved/discovered directory state is stored in `directory.json`. The file is
limited to 8 MiB and 4,096 retained entries; live announcements remain capped
at 256 visible records and 1,024 transient entries. Destination and associated
hash strings are limited to 1 KiB and display names to 16 KiB before live state
mutation. Loading admits only a stable regular non-symlink file. Oversized and
special paths fail without read, backup, or mutation. Malformed or semantically
excessive admitted bytes remain in place after an exact owner-only backup is
synchronized. Only four current-namespace backups/32 MiB are retained under a
4,096-entry scan ceiling, and legacy names remain untouched. Saves use private
synchronized same-directory atomic replacement. Trust, saved-entry, delivery,
identify, and clear-live actions restore their prior in-memory state if
publication fails.

LXMF conversation threads are stored as schema-compatible JSON below the
identity-scoped messages root. Each file is limited to 8 MiB and 4,096
messages. Discovery retains at most 256 regular thread files/64 MiB while
scanning at most 4,096 directory entries; symbolic links are never followed.
Peer keys that are unsafe as portable filenames use a deterministic contained
hash filename while retaining their original value in JSON. Message titles,
bodies, IDs, fields, attachments,
labels, and reply-ticket metadata have additional retained item/string budgets.
Existing single-component legacy filenames remain readable and update in place
on hosts where that filename is valid; imports and new threads use the portable
mapping.
Writes use owner-only synchronized same-directory staging and atomic
replacement. Bounded malformed files are backed up from the admitted byte
snapshot under `omen-message.corrupt.*.bak`; only the newest four recognized
backups/32 MiB are retained. Legacy and ambiguous backup names are untouched.

For standalone `omenchatd`, the selected home is resolved to a clean absolute
policy root before an existing `config.toml` is parsed. The config must be a
real regular file; symlinks and non-regular objects fail before path-bearing
TOML is accepted. Config content is read through a stable bounded handle.
`identity_path`, `database_path`, and `reticulum_config_path` reject every `..`
component. Relative paths retain their existing current-working-directory
meaning but are resolved deterministically for the managed/custom decision.
Managed suffixes are walked without following symlinks. Clean custom paths are
supported only under operator-controlled parents; `omenchatd` does not chmod or
recursively create their unrelated ancestors.

Native LXMF file attachments are stored below the identity-scoped
`attachments` root. Outbound attachment sources must be regular non-symlink
files and are capped at 64 items, 8 MiB per file, and 16 MiB in aggregate;
missing paths retain the established skip behavior. Inbound LXMF attachments
use the same item and byte limits plus a 4 KiB filename limit. Accepted files
are published through owner-only synchronized same-directory staging into an
owner-only message directory. Long stored path components use a deterministic
bounded hash suffix. Replaying the same message replaces its deterministic
attachment path atomically instead of creating duplicate files.
Unsafe storage directories or destination links fail closed without touching
their referents. These are local admission/storage rules and do not change the
LXMF attachment field encoding.

The native LXMF delivered-transient cache is stored as
`reticulum/storage/lxmf/local_deliveries_rs.json` within the selected isolated
application/identity root. It retains the six-month duplicate-suppression
policy, at most 65,536 IDs, and at most 8 MiB on disk. The cache must be a
regular non-symlink file. Oversized or special paths fail closed without being
read, moved, or copied. Malformed bounded files remain in place after an exact
owner-only backup is durably published; only the four newest application-owned
backups (32 MiB total) are retained. Cache replacement is synchronized and
atomic, and neither legacy backup names nor unrelated sibling files are pruned.

Browser form restoration is stored separately in `browser_form_state.json` and
retains the existing configurable age policy (14 days by default). The store is
limited to 512 newest pages, 128 fields per page, 2 KiB page URLs, 256-byte
field names, 64 KiB field values, and 4 MiB serialized data. Oversized input is
not read or restored. The file must be regular and non-symlink; oversized and
special paths fail without read, backup, or mutation. Malformed admitted bytes
remain in place after an exact owner-only backup is synchronized. Only four
current-namespace backups/16 MiB are retained under a 4,096-entry scan ceiling,
and legacy names are untouched. Saves use an owner-only create-new
same-directory temporary file, file synchronization, atomic replacement,
cleanup on failure, and parent-directory synchronization on Unix. Forget/prune
actions restore their prior in-memory state if publication fails.

Browser structured logs default to 256 KiB rotation files, four retained
rotations, and no historical entries loaded at startup. When historical loading
is enabled, the requested entry count is capped at 4,096. Startup scans at most
4,096 directory entries, selects at most 16 regular files without following
symlinks, reads at most 512 KiB per file and 4 MiB total, and keeps only the
newest requested entries. The live UI log buffer is independently limited to
4,096 entries and 4 MiB of message storage; individual messages are copied into
fresh bounded allocations and UTF-8-safely truncated to 16 KiB. Values above
the startup-entry cap are rejected by Settings, while an older saved value is
safely capped and reported in the log.

The on-disk policy accepts rotation sizes from 4 KiB through 8 MiB and retains
from one through 16 rotated files. A serialized record is truncated again when
needed to fit a deliberately small rotation size, and rotation occurs before an
append would cross that size. Existing settings outside these ranges are
clamped in memory and produce a structured warning; the normalized values are
written the next time settings are saved. Retention maintenance inspects at
most 4,096 directory entries per pass, ignores symbolic links and non-regular
files, and records truncated scans and write/removal failures in internal
counters. The active log path is also refused when it is a symbolic link or
another non-regular file. Pre-existing rotated files are aged out by the
retention count; an older oversized rotation is not destroyed solely because a
new upper bound was introduced.

During an interactive session, JSON serialization and filesystem rotation run
on one dedicated `omenbrowser-log-writer` thread rather than the Iced update
path. Admission never waits for storage: the queue holds at most 256 records
and 2 MiB, and excess records are counted as dropped. Metrics expose queued
items/bytes, exact oldest age, completed/dropped records, rotations, and
write/removal/unsafe-path failures. TUI and desktop shutdown request a bounded
flush; the desktop waits from one shutdown-only bounded blocking task while the
runtime drains in parallel.

Diagnostics and Logs show a snapshot of the worker queue and disk counters.
Reading that snapshot only locks the bounded in-memory accounting state: it
does not scan log files, emit another log record, or create a periodic UI
subscription.

## Server Storage

Default root:

```text
~/.omenchatd/
```

Use `--home` for isolated servers:

```bash
omenchatd init --home /tmp/omenchatd-test
```

Uploads remain beneath the server-owned upload cache. Identity upload
directories must be real directories, not symbolic links. On Unix, newly
committed upload files are created with mode `0600`.

## Interfaces

Configure interfaces in the browser Interfaces panel or through `omenchatd`
commands.

For `omenchatd`:

```bash
omenchatd interfaces tcp-client <gateway-host:port> --home /tmp/omenchatd-test
```

## Tor/SOCKS

OMENbrowser_rs detects common local SOCKS5 Tor ports:

```text
127.0.0.1:9050
127.0.0.1:9150
```

When enabled, clearweb image loading can use SOCKS5. External HTTP/HTTPS links
open through a browser prompt. Use **Copy URL** for Tor Browser and paste into
the already-running Tor Browser window; launch buttons are for regular detected
browsers or browser profiles you configured yourself.
Inline SOCKS media refuses URL credentials, local/single-label/mDNS names,
private/link-local/special-use IP literals, redirects beyond five hops, and
HTTPS-to-HTTP downgrade. DNS remains resolved by the configured SOCKS proxy;
the application cannot independently verify a proxy-resolved hostname's final
address, so only use a trusted local Tor/SOCKS endpoint.

## Identity Safety

Creating a new managed identity creates separate owned storage for that
identity. Do not reuse an app root for independent test clients.

Identity material must be a non-empty regular file, not a symbolic link, and
no larger than 64 KiB. Attach, import, export, managed-identity discovery,
identity-scoped storage selection, and native Reticulum loaders share one
bounded reader; import and backup use the single admitted byte snapshot rather
than reopening a mutable source. Managed discovery examines at most 4,096
directory entries and retains at most 256 identity profiles. A symbolic-link or
non-directory identity root and scan/profile saturation fail explicitly.

Managed identity creation/import and identity backups are published from a
unique same-directory owner-only staging file only after its contents have been
flushed and synchronized. New identities and backups are no-clobber; replacing
an imported managed identity happens atomically only after a synchronized
backup has been published. The managed backup directory retains at most 16 new
application-owned backups and 1 MiB total, scanning at most 4,096 entries.
Retention recognizes only names in the current `omen-identity.backup.*.bak`
namespace. Legacy, custom-export, symlink, and otherwise ambiguous entries are
never removed automatically. If retention cannot be completed, the newly
published backup is preserved and the replacement or deletion is aborted.

The durable OMENchat foundation reserves identity-scoped state under
`omenchat/`. `client-instance-id` is the existing owner-only 16-byte random
client identity. A separate owner-only `mutation-intents.sqlite` store is now
defined for future negotiated mutations, but current startup and send paths do
not open or populate it. Its isolated API caps admission at 4,096 intents,
16 MiB total, and 64 KiB per intent without deleting pending or uncertain work.
On Unix it refuses symlinked or group/world-readable storage.
The inactive storage owner uses one named thread and a 32-request/2-MiB bounded
queue. Oversized payloads and queue saturation are rejected before admission;
shutdown drains admitted requests and joins the owner. This worker is not
started by the application yet.

## IFAC Secret Input

Use one of these CLI sources when temporarily configuring an IFAC-protected TCP
client:

- `--passphrase-file <path>` for a regular file with no group/other access;
- `--passphrase-stdin` for a protected pipe;
- `--passphrase-prompt` for a cross-platform hidden terminal prompt.

Inputs are limited to 4096 UTF-8 bytes, must be non-empty, and may not contain a
NUL byte. Only trailing CR/LF is removed. Multiple sources are rejected. The
legacy `--passphrase <value>` remains temporarily compatible but warns because
argv can be exposed through process listings and shell history.

OMENbrowser stores configured interface profiles in `interfaces.json` and
renders the active Reticulum configuration under `reticulum/config`.
`omenchatd` renders its active configuration under its own `reticulum/config`.
The browser admits `interfaces.json` only as a regular, non-symlink file up to
2 MiB, with at most 64 profiles and 64 peers per profile. Profile text fields
are limited to 16 KiB, passphrases to 4 KiB, and configuration-breaking CR,
LF, and NUL characters are rejected. Gateway presets follow the same safe-file
rule with a 1 MiB/256-preset ceiling. A legacy gateway preset file is validated
before private publication and is retained as the migration source.

An existing managed Reticulum config must be a regular, non-symlink file no
larger than 1 MiB before OMENbrowser will preserve its instance or network
identity and replace it. Unsafe, malformed, or oversized profile/preset inputs
fail closed and are not overwritten. Profile mutations restore their prior
in-memory value when persistence fails. These are admission and durability
limits; the accepted JSON and generated Reticulum configuration formats are
unchanged.

On Unix, these secret-bearing files are created and repaired as owner-readable
and owner-writable only (`0600`). Diagnostic bundles are likewise created under
an owner-only umask and redact passphrase assignments, environment-style
values, and legacy passphrase arguments. Operators should still review a bundle
before sharing it.

Interface summaries display only `configured`, `not set`, or `editing (hidden)`
for the passphrase. Iced input widgets use secure-entry mode, the legacy TUI
does not render its active passphrase buffer, and debug formatting for interface
profiles and command overrides substitutes a redaction marker.
## omenchatd typed configuration

Standalone `omenchatd` configuration uses a versioned, typed TOML document.
New files begin with `version = 1`. Existing version-0 flat keys remain
readable, while the generated sectioned layout is preferred.

Unknown keys, wrong value types, future versions, and changes to documented
fixed policy fields block configuration loading with a path-aware diagnostic.
This is intentional for quota, rate, and security settings: a misspelled limit
must not appear active while being ignored. TOML escaping preserves quotes,
Unicode, backslashes, and embedded newlines.

Config saves reparse the rendered document before touching disk, write an
owner-only same-directory temporary file, flush and synchronize it, retain the
previous valid bytes as `config.toml.bak`, then atomically rename and synchronize
the directory where supported. An invalid existing config is not overwritten.
Native Windows replacement semantics and post-rename power-loss testing remain
release-platform gates.
