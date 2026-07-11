# Task 3 Gate Review

recommendation: REJECT

## originalIntent

Deliver plan checkbox 3 for `windows-native-dock-v1`: a versioned Rust domain model, pure reducer, configuration persistence, migration/recovery behavior, and checksum-aware inert `.docktheme` theme import/export for a local-first Windows shell companion.

## desiredOutcome

The user should receive a trustworthy foundation for later Windows platform/rendering work: typed validated state transitions with explicit effects, safe default configuration, V0-to-V1 migration, bounded malformed/future recovery without panic, backup-preserving persistence with an interruption seam, deterministic tamper detection for portable themes, and focused tests/property tests proving the behavior.

## checkedArtifacts

- `.omo/plans/windows-native-dock-v1.md`
- `.omo/evidence/task-3-red.txt`
- `.omo/evidence/task-3-verification.txt`
- `.omo/evidence/task-3-config-matrix.txt`
- `Cargo.toml`, `deny.toml`, `clippy.toml`, `rust-toolchain.toml`
- `crates/shell-core/src/{events.rs,ids.rs,model.rs,reducer.rs,state.rs,lib.rs}`
- `crates/shell-config/src/{schema.rs,persistence.rs,theme.rs,lib.rs}`
- `crates/shell-core/tests/{reducer.rs,properties.rs}`
- `crates/shell-config/tests/{config.rs,persistence.rs,properties.rs,theme.rs,fixtures/v0.json}`
- `crates/shell-config/examples/config_matrix.rs`
- Required skills consulted: `omo:remove-ai-slops`, `omo:programming`, plus Rust programming references.

## reproducedChecks

- PASS `cargo fmt --all -- --check`
- PASS `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- PASS `cargo test --workspace` twice
- PASS `cargo build --workspace --release`
- PASS `cargo deny check` with unmatched-license allowance warnings only
- PASS `cargo run --release -p shell-config --example config_matrix`: `DEFAULT`, `VALID`, `MIGRATED`, `PRESERVATION`, `CORRUPT`, `FUTURE`, `TAMPERED_THEME`, `CLEANUP`; exit `0`, 8 PASS rows
- PASS worktree clean before this report artifact; no cargo/rustc/test/config_matrix processes remained

## userOutcomeReview

The implementation largely matches the technical outcome: `shell-core` exposes typed events/effects and a pure reducer; `shell-config` has bounded decode, V1 defaults, V0 migration, future-version recovery, backup fallback, and deterministic SHA-256 theme envelopes with traversal/external reference rejection. The documented non-atomic standard-library writer fallback is acceptable as Task 3 integration debt because `load` can recover from a missing/corrupt primary via `.bak`, and Task 4/platform work can replace the writer with a Windows-native `ReplaceFileW` adapter.

Approval is blocked because the gate package is incomplete and one executor claim is unsupported by direct artifacts.

## blockers

1. Missing required review artifacts. I found no code review report and no notepad path in the supplied input or `.omo/evidence/`. The required code-review report therefore cannot be verified for `omo:programming` and `omo:remove-ai-slops` coverage, including overfit/slop criteria. Direct slop review did not find deletion-only, tautological, or implementation-mirroring tests severe enough to block on its own, but the required independent report coverage is absent.

2. Unsupported stale cleanup claim. `.omo/evidence/task-3-verification.txt` claims stale same-directory temp/rollback paths are removed by the standard writer. I found no focused test for stale `.tmp`, `.bak.tmp`, or `.previous` cleanup. Source inspection shows `persistence.rs` removes `.tmp` unconditionally, but `.bak.tmp` and `.previous` cleanup occur only inside the `destination.exists()` branch. A pre-existing `%TEMP%\shell-config-backup-34060` directory contained `settings.json`, `settings.json.tmp`, and `settings.json.bak.tmp`; the current `config_matrix` cleaned its own process-specific directory, but this stale backup state shows the broader cleanup claim is not artifact-supported.

## evidenceGaps

- No code review report artifact with explicit `remove-ai-slops` overfit/slop pass and `programming` criteria coverage.
- No notepad path was provided.
- No test/probe simulates the writer crash windows after backup-temp creation or after destination-to-rollback rename.
- No test proves stale `.bak.tmp` or `.previous` cleanup when the primary destination is missing.

## slopAndScopeReview

- No `unsafe`, `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!`, or broad `#[allow]` was found in the changed Rust files. `println!` appears only in the CLI QA example.
- File-size pass: `reducer.rs` is exactly 250 pure LOC; `persistence.rs` and `schema.rs` are in the 200-250 warning band. No file exceeds the hard 250 pure LOC ceiling.
- Tests are behavior-oriented and not deletion-only. Property tests exist, though reducer/config generators are narrow and should be broadened in later hardening.

## exactEvidence

- `crates/shell-config/src/persistence.rs:63-85`: std fallback removes `.tmp`, copies `.bak.tmp`, renames backup, moves destination to rollback, renames temp into destination, and restores rollback on rename failure.
- `crates/shell-config/src/persistence.rs:151-177`: load order is primary, backup, defaults.
- `crates/shell-config/src/schema.rs:173-208`: bounded decode with malformed/future/V0 handling.
- `crates/shell-config/src/theme.rs:124-159`: bounded `.docktheme` import, schema check, payload validation, checksum validation, forbidden reference checks.
- `crates/shell-core/src/reducer.rs:8-58`: pure reducer validates source and next state and returns typed effects/outcome.

