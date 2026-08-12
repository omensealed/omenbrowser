# Documentation history

The current tree intentionally contains product guidance, published release
notes, current release evidence, and upstream-ready reports. It does not retain
every intermediate Codex plan or phase-unit transcript.

In August 2026 the documentation set was consolidated because more than one
hundred audit, design-checkpoint, review-unit, and superseded migration files
were obscuring current behavior and causing stale version statements to appear
authoritative.

Removed material remains available through Git history and immutable release
tags:

```bash
git log --all -- docs/
git show v0.9.8-5:docs/TESTING.md
git show <older-tag>:docs/<historical-path>
```

Published release notes remain in the current tree. Historical test outcomes
belong in the tag that produced them; current test commands belong in
[Testing](TESTING.md).

When adding documentation:

1. update an existing current guide when behavior changes;
2. add release-specific outcomes to that release's notes/checklist;
3. keep upstream evidence under `docs/upstream/`;
4. avoid adding per-step progress transcripts to the repository;
5. state whether a claim is current behavior, historical evidence, a known
   limitation, or deferred work.
