# Architecture Baseline Stabilization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `test-driven-development`,
> `systematic-debugging`, and `verification-before-completion`. Execute inline in
> the canonical checkout because the repository forbids persistent worktrees and
> the baseline includes uncommitted user work. Do not stage, commit, move, delete,
> or revert pre-existing files.

**Goal:** Establish a reproducible deterministic gate, a separate opt-in native
Windows validation gate, executable dependency governance, and a classified
inventory of the current checkout without losing existing work.

**Architecture:** A small PowerShell policy Module validates Cargo dependency
edges and time-limited exceptions from explicit data. Cargo features classify
machine-dependent tests without hiding them. CI runs deterministic verification
on every change and exposes native validation as an explicit Windows workflow.

**Tech Stack:** Rust 1.97, Cargo features, PowerShell 7/Windows PowerShell,
GitHub Actions, JSON policy data, Markdown governance documents.

---

### Task 1: Executable architecture policy

**Files:**
- Create: `scripts/ArchitecturePolicy.psm1`
- Create: `scripts/Check-Architecture.ps1`
- Create: `scripts/tests/ArchitecturePolicy.Tests.ps1`
- Create: `scripts/tests/fixtures/invalid-workspace-metadata.json`
- Create: `scripts/tests/fixtures/valid-workspace-metadata.json`
- Create: `docs/architecture/exceptions.json`

- [x] Write a PowerShell test that imports `ArchitecturePolicy.psm1`, proves the
  valid fixture has no violations, proves the invalid fixture reports the
  forbidden edge, and proves an expired exception is rejected.
- [x] Run `powershell -NoProfile -File scripts/tests/ArchitecturePolicy.Tests.ps1` and
  verify RED because the policy Module does not exist.
- [x] Implement `Test-WorkspaceDependencyPolicy` and
  `Test-ArchitectureExceptionPolicy` with deterministic inputs.
- [x] Implement `Check-Architecture.ps1` to load `cargo metadata`, the exception
  registry, and fail on any violation.
- [x] Run the PowerShell test and then `powershell -NoProfile -File
  scripts/Check-Architecture.ps1`; both must exit 0.

### Task 2: Explicit temporary Clippy exceptions

**Files:**
- Modify: `crates/shell-platform-windows/src/win32_owner.rs`
- Modify: `crates/shell-platform-windows/src/win32_popover_render.rs`
- Modify: `docs/architecture/exceptions.json`

- [x] Use the existing strict Clippy command as RED and preserve its six exact
  `too_many_arguments` failures as evidence.
- [x] Register the six failures and the one preexisting allow with symbol, owner,
  risk, compensation,
  removal plan, and expiration `2026-07-29`.
- [x] Add one function-level `#[allow(clippy::too_many_arguments)]` per registered
  symbol; do not change runtime behavior.
- [x] Run the architecture checker to prove every allow is registered and
  unexpired.
- [x] Run strict Clippy and require exit 0.

### Task 3: Separate hermetic and native Windows tests

**Files:**
- Modify: `crates/shell-platform-windows/Cargo.toml`
- Modify: `crates/shell-app/Cargo.toml`
- Modify: `crates/shell-app/tests/showcase_startup.rs`
- Modify: `crates/shell-platform-windows/src/win32_app_identity.rs`
- Modify: `crates/shell-platform-windows/src/win32_package_icon.rs`

- [x] Preserve RED evidence from `cargo test --workspace --all-features` and
  `cargo test -p shell-platform-windows --all-features`.
- [x] Add an opt-in `native-validation` feature to the platform crate and forward
  it from the app crate.
- [x] Compile the AppBar startup test only with `native-validation`.
- [x] Compile the environment-dependent package and executable tests only with
  `native-validation`; retain deterministic parsing and identity tests normally.
- [x] Run `cargo test --workspace` and require exit 0.
- [x] Run `cargo test -p shell-platform-windows --features native-validation
  --lib` and confirm that the native environment still reports its real missing
  prerequisites rather than hiding them.

### Task 4: CI gate split

**Files:**
- Modify: `.github/workflows/ci.yml`
- Modify: `README.md`
- Modify: `docs/architecture/delivery.md`

- [x] Add the architecture checker to the required Windows quality job.
- [x] Change the required test command to the hermetic `cargo test --workspace`.
- [x] Add `workflow_dispatch` input `run_native_validation` and an opt-in native
  job that runs the feature-gated platform and startup validations.
- [x] Document exact local deterministic and native commands.
- [x] Parse the workflow as YAML when a local parser is available; otherwise
  inspect the complete workflow and run every embedded local command that does
  not require GitHub services.

### Task 5: Classify the baseline

**Files:**
- Create: `docs/architecture/audits/2026-07-15-change-classification.md`
- Modify: `docs/architecture/audits/2026-07-15-baseline.md`

- [x] Capture `git status --porcelain=v1 -uall` and group paths by top-level
  ownership and durable/generated classification.
- [x] Record counts, disposition recommendation, and required human approval for
  each class without deleting or moving anything.
- [x] Record the post-stabilization gate results separately from the original
  snapshot so historical evidence is not rewritten.

### Task 6: Fresh verification

**Files:** none beyond prior tasks.

- [x] Run `cargo fmt --all --check`.
- [x] Run `powershell -NoProfile -File scripts/tests/ArchitecturePolicy.Tests.ps1`.
- [x] Run `powershell -NoProfile -File scripts/Check-Architecture.ps1`.
- [x] Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [x] Run `cargo test --workspace`.
- [x] Run `cargo build --workspace --release`.
- [x] Run Markdown link and trailing-whitespace checks for
  `docs/architecture/`.
- [x] Confirm `git diff --cached --name-only` is empty and report native
  validation failures separately from deterministic gate status.
