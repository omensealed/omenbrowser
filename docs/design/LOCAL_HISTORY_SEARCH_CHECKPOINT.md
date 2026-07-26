# Local history search checkpoint

Status: bounded read-only domain slice implemented; UI integration pending  
Release target: `v0.9.6-4`  
Storage baseline: LXMF bounded JSON threads; OMENchat identity-scoped SQLite

## Current-state findings

LXMF and OMENchat do not share one durable store:

- LXMF conversations are retained as bounded JSON thread files. Each thread is
  limited to 4,096 messages/8 MiB; discovery and aggregate storage also have
  explicit count and byte ceilings.
- OMENchat room events are retained in the browser identity's `chat.sqlite`
  and projected into `ChatClient` sessions. A resident session retains at most
  1,024 events/8 MiB.
- omenchatd has a separate SQLite database and identity. Client-local search
  must not open, index, or depend on that server database.

The canonical desktop product enables `portable-sqlite`, and both product
manifests use the bundled SQLite source. `libsqlite3-sys 0.35.0` enables
`SQLITE_ENABLE_FTS5`; a product-profile runtime test now creates and queries a
temporary FTS5 table. This proves FTS5 is available in the packaged desktop
build, but it does not by itself justify an index.

## First-slice decision

The first slice uses no schema or persistent index. `history_search` provides a
project-owned, read-only reducer over already-retained `Conversation` and
`ChatSessionView` values:

- query strings: at most 256 bytes and eight text terms;
- work: at most 8,192 examined messages/events per request;
- output: at most 128 results;
- every copied display field: at most 256 bytes with UTF-8-safe truncation;
- matching: allocation-free ASCII-insensitive byte comparison;
- ordering: newest first with deterministic public-text tie breakers;
- visible result keys: typed ephemeral routing keys, not searchable opaque
  network identifiers.

The reducer supports public text, sender, room, inclusive date bounds,
attachment-only filtering, source filtering, and LXMF delivery state. OMENchat
does not fabricate a delivery state because its room-event model does not
contain LXMF delivery evidence.

The following are intentionally excluded from searchable text:

- peer and server destination hashes;
- message IDs and OMENchat resource IDs;
- arbitrary LXMF extension fields;
- private attachment paths;
- idempotency, correlation, ticket, stamp, or authentication material.

## UI and ownership checkpoint

The reducer is not called from the Iced update or view path in this slice. UI
activation requires one owned search task with:

- one current generation; a newer query cancels or supersedes the old result;
- at most one in-flight scan;
- immutable bounded input snapshots created without filesystem or SQLite work
  in `view`;
- explicit result/error delivery through the existing task-result boundary;
- no recurring timer, polling subscription, or background indexer;
- an honest `scan limit reached`/`result limit reached` indicator;
- jump actions that validate the typed result key against current retained
  state before changing selection or scroll position.

Search input should be submitted explicitly or after a bounded debounce. Every
keystroke must not synchronously scan the maximum resident history.

## FTS5 disposition

FTS5 remains available but deferred. It becomes justified only if measurement
shows the bounded resident reducer cannot meet interactive latency targets.
Any later index must be a rebuildable, non-authoritative derivative:

- never replace LXMF JSON or OMENchat `room_events` as authoritative history;
- remain identity scoped;
- version and validate its tokenizer/schema;
- bound rows and bytes to the authoritative stores;
- update transactionally with OMENchat event persistence;
- use bounded incremental rebuild/pruning work;
- be removable without deleting messages;
- provide copy/rollback tests before activation.

A unified persistent FTS table spanning JSON and SQLite would require a new
index owner and reconciliation lifecycle. It is not admitted for this release
without measurement.

## Compatibility and rollback

This slice changes no wire operation, capability, application protocol,
database schema, configuration, identity path, message format, or server
behavior. Removing `history_search` and its tests is a complete rollback.

## Test matrix

- LXMF public body/title/sender/attachment matching.
- OMENchat public body/sender/room/date/attachment matching.
- source and LXMF delivery filters.
- opaque IDs, arbitrary fields, and private paths are not searchable.
- query, term, scan, result, and copied-text boundaries.
- UTF-8-safe excerpts.
- deterministic newest-first ordering.
- packaged SQLite compile-option and functional FTS5 probe.

## Next gate

An ignored deterministic measurement exercises the full 8,192-item reducer
scan over 64 MiB of retained LXMF message text. It reports full-miss and
128-result hit timings without imposing a hardware-specific pass/fail
threshold:

```bash
cargo test --release --locked --no-default-features \
  --features desktop-product \
  history_search::tests::measure_maximum_bounded_lxmf_search \
  -- --ignored --exact --nocapture
```

To record process resident memory as well as elapsed reducer time, first warm
the release build and then prefix the same command with `/usr/bin/time -v`.
Cargo/build time must not be reported as reducer latency; the test prints the
timed reducer operations separately.

On 2026-07-26, the optimized reducer scanned 8,192 messages containing exactly
67,108,864 bytes of retained message text in approximately 133 ms for a full
miss and 112 ms for a result-producing scan that exceeded and truncated to the
128-result ceiling on the available Linux host. These are single observations,
not portable thresholds. Peak RSS was not collected
because GNU `time` was not installed (`/usr/bin/time` was absent; the usual host
package is `time`). This evidence rejects synchronous Iced update/view
execution and cloning the maximum resident history for each query. The search
must run as owned, bounded blocking work against isolated store data.

OMENchat now exposes an explicit `SqliteChatStore::open_read_only` foundation
for that worker. It opens only an existing database, never creates parent
directories, never runs migrations, and enables SQLite `query_only` as a
fail-closed second layer. Focused tests prove existing history remains readable,
mutating store calls fail, and a missing database/path is not created. The
method does not yet enumerate or search events.

The same read-only handle now provides a newest-first history loader with both
row and cumulative SQLite byte budgets. Admission first uses the `room_events`
rowid index to restrict work to the requested number of most recently persisted
rows; timestamp ordering and byte admission happen only inside that bounded
window. The SQL byte window accounts for retained payload, identity-routing,
actor, server-label, room-label, and metadata bytes before Rust materializes
rows. This prevents the 8,192-item scan ceiling from becoming either an
unbounded database scan or a multi-gigabyte allocation when legal
protocol-sized message bodies are present. Opaque server IDs remain routing
data; only joined display names are intended for the search reducer. The loader
hard-clamps callers to 8,192 rows and 64 MiB, does not interpret query text, and
does not change the database.

LXMF now has a corresponding `MessageStore::list_threads_read_only` loader.
It retains the existing inventory limits (256 threads, 64 MiB aggregate, 8 MiB
per thread), validates every decoded thread, and preserves recent-first
ordering. Unlike the normal recovery loader, malformed JSON returns an error
without creating or pruning corrupt-file backups. Search therefore cannot
mutate message storage merely by reading it.

`search_persisted_local_history` now combines the loaders behind one
UI-independent operation. A source-specific query receives the full 8,192-item
scan budget. A combined query reserves 4,096 items for each source so a large
LXMF thread cannot starve OMENchat, or vice versa. Sources are loaded and
reduced sequentially, results share the existing 128-item cap and deterministic
newest-first ordering, and opaque peer/server keys remain typed routing data
rather than searchable or presented text. A temporary-root test covers both
stores and proves public labels/text remain separate from routing identifiers.

Next, add the owned one-in-flight store-backed blocking task and a compact
search surface without persistence or schema changes. UI activation is not
complete until stale-result, cancellation/supersession, focus, jump, and
isolated-root tests pass.
