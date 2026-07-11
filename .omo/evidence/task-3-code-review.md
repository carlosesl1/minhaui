# Task 3 Code Review

reviewed: 2026-07-11 America/Sao_Paulo
scope: commit `deb52bd` plus the persistence gate correction in `crates/shell-core/**`, `crates/shell-config/**`, root dependency manifests, and task-3 artifacts
result: PASS, no blocking findings

## Behavior lock and changed-files review

- The original behavior is locked by reducer, schema, migration, theme, property, persistence, and release `config_matrix` tests.
- The gate correction was test-first. `.omo/evidence/task-3-fix-red.txt` records two failures against the prior writer: unpublished `.bak.tmp` recovered as defaults and a sole valid `.tmp` was discarded.
- Four focused real-filesystem tests cover missing-primary rollback restoration, unpublished backup reconciliation, valid-primary precedence over stale rollback, and sole-valid-temp promotion. They assert usable config values, backup provenance, staging cleanup, and successful subsequent persistence rather than implementation call counts.
- Reviewed correction files: `crates/shell-config/src/lib.rs`, `persistence.rs`, `recovery.rs`, and `tests/persistence.rs`. Reviewed supporting evidence/notepad changes and the independent gate report. No other crate implementation changed.

## `omo:programming` review

- Type safety: `AtomicPaths`, `ConfigLoad`, `ConfigSource`, `PersistenceError`, schema versions, IDs, reducer events/effects, and theme payloads remain typed; no stringly state machine was introduced.
- Trust boundaries: `read_valid` enforces the byte bound and typed decode before any staging candidate is promoted. Invalid candidates are never treated as preserved configuration.
- Error handling: filesystem errors propagate as typed `PersistenceError`; no swallowed I/O error, `unwrap`, `expect`, `panic!`, `todo!`, or `unimplemented!` exists in scope.
- Exhaustiveness: owned enums are matched explicitly; no wildcard was added to domain transitions.
- Allocation/cost: recovery caches destination/backup validity and reads each primary candidate once per phase; this is startup/persist-path I/O, not a hot loop.
- Purity/boundaries: all new filesystem behavior remains in `shell-config`; `shell-core` stays safe, deterministic, and I/O-free.
- TDD: failing crash-window tests preceded the implementation, then passed without weakening assertions.
- Tool discipline: `check-no-excuse-rules.py` passed all 20 Rust source/test files.

## `omo:remove-ai-slops` review

- Deletion ladder: the behavior cannot be deleted or replaced by a safe std single-call Windows API. The smallest viable correction is a focused startup reconciliation module behind the existing writer seam.
- Obvious comments: retained comments are public API contracts, the Windows atomicity debt explanation, or Given/When/Then BDD markers. No section-divider, commented-out, or restatement-only production comment was added.
- Over-defensive code: each existence/validity check corresponds to a distinct crash artifact and is exercised by a real filesystem fixture. No duplicate boundary validation was removed.
- Excessive complexity: reconciliation uses guard-style phases and cached booleans; no deep nesting, variant `if` chain, long parameter list, or god function was added.
- Needless abstraction: `recovery.rs` owns one cohesive responsibility and reuses the existing `AtomicWriter` seam. No factory/pass-through/interface was introduced.
- Boundary violations: none; recovery does not leak into the pure reducer or theme/schema layers.
- Dead code/debug output: none found by Clippy/forbidden-pattern scan. `println!` remains confined to the required QA example.
- Duplication: config validity parsing was moved, not copied; test path construction is centralized in a fixture helper.
- Performance equivalence: repeated primary/backup reads were collapsed into cached validity flags. No algorithmic behavior changed.
- Missing tests/overfit: tests use actual files and serialized configs, stage externally observable crash states, and prove recovery values/source plus cleanup. They do not mock internal calls, pin constants without behavior, or assert deletion alone.
- Oversized modules: the Rust no-excuse checker passed. Pure LOC counts: `shell-core` 18-250; `shell-config/lib.rs` 21, `persistence.rs` 214, `recovery.rs` 45, `schema.rs` 228, `theme.rs` 161. No source file exceeds 250 pure LOC.

## Forbidden and adversarial scan

- Forbidden-pattern scan: no unsafe block, unwrap, expect, panic, todo, unimplemented, or allow attribute.
- Malformed/truncated/future/oversized config and tampered/forbidden theme cases remain covered.
- Interrupted write, backup-temp publication, rollback restoration, stale primary/rollback precedence, temp-only recovery, corrupt primary fallback, and repeated test execution are covered.
- Prompt injection: N/A because these crates consume no model/browser/network instructions.

## Remaining risk/debt

- Strict single-operation Windows replacement remains deferred because std does not expose `ReplaceFileW`; the public writer seam is the integration point.
- File contents are flushed/synced, but std does not provide a portable directory metadata sync contract on Windows.
- If later platform integration adds native replacement/FFI, it belongs outside these safe crates and requires the project unsafe/Miri review process.
