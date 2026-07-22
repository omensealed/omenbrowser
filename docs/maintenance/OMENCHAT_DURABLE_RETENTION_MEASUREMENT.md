# OMENchat durable retention measurement

This measurement qualifies the dormant durable-mutation replay and outbound
intent stores without enabling negotiation, transmission, or retry. It uses
new temporary SQLite databases under the operating-system temporary directory;
it never opens application identities, messages, or omenchatd production data.

Run the release-mode harness from the repository root:

```bash
scripts/measure-durable-mutation-retention.sh /tmp/omen-durable-retention
```

`OMEN_DURABLE_MEASUREMENT_ITEMS` may select 256 through 4,096 fixture items.
The default is 1,024. The output directory contains raw client/server logs,
environment metadata, machine-readable summary lines, and normalized results.

## Hard structural gates

- The server retains exactly half of the inserted replay results under the
  measurement limit.
- Every observed client instance remains recorded, and every pruned half is
  retired.
- The client recovers every prepared intent before terminal transition.
- Client terminal pruning removes every fixture in batches of at most 128.
- The server fixture occupies no more than 16 MiB and the client fixture no
  more than 32 MiB after a WAL checkpoint.

The structural assertions are test failures, independent of timing.

## Release-mode review thresholds

These are broad regression triggers for an isolated local SSD fixture, not
universal hardware promises:

- server new-result commit p95: at most 50 ms;
- server exact replay p95: at most 10 ms;
- server worst observed commit: at most 250 ms;
- client prepared-intent persistence p95: at most 50 ms;
- client worst observed prepare: at most 250 ms;
- recovery of the selected fixture count: at most 2 seconds.

Exceeding a threshold blocks activation pending investigation. Passing does not
prove live Reticulum interoperability or justify running SQLite on the Iced
update path; the bounded storage owner remains required.

## 2026-07-21 baseline

Host: Linux x86_64, rustc 1.97.0. Release mode, 1,024 items.

| Measurement | Result |
|---|---:|
| Server retained / client rows / retired | 512 / 1,024 / 512 |
| Server database bytes | 434,176 |
| Server commit p50 / p95 / max | 439 / 536 / 692 µs |
| Server replay p50 / p95 / max | 30 / 39 / 58 µs |
| Client recovered / pruned / prune calls | 1,024 / 1,024 / 8 |
| Client database bytes | 364,544 |
| Client prepare p50 / p95 / max | 156 / 199 / 400 µs |
| Client recovery | 41,839 µs |
| Client terminal transitions total | 139,439 µs |
| Client pruning total | 3,175 µs |

All structural and timing gates passed. Raw evidence for this run was written
to `/tmp/omen-durable-retention-current`; that path is local evidence and is not
part of the repository.

The maximum 4,096-item fixture was also run on the same host. It retained
2,048 server results, retired 2,048 of 4,096 registered instances, recovered
and pruned all 4,096 client intents in 32 bounded calls, and passed every gate.
The server/client databases used 1,282,048/1,388,544 bytes. Server commit
p50/p95/max was 650/1,390/1,615 µs; exact replay was 31/40/74 µs. Client
prepare p50/p95/max was 241/360/431 µs and recovery took 139,768 µs. Raw local
evidence is at `/tmp/omen-durable-retention-4096`.

Live mixed-version, disconnect, restart, and uncertain-result tests remain
separate activation gates.
