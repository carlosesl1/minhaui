# Public Interface Reduction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce the default public Interface of `shell-platform-windows` to its executable entrypoint and shrink `shell-renderer` to the renderer contract actually consumed by the platform.

**Architecture:** Existing black-box tests become crate-internal test modules so Implementation details no longer need public reexports. `PlatformEvent` and controller types remain internal. Renderer exports unused by platform code and platform tests become crate-private. Architecture policy snapshots the allowed facade declarations.

**Tech Stack:** Rust 1.97, Cargo, PowerShell architecture policy, TDD.

---

### Task 1: Add the public-facade architecture gate

**Files:**
- Modify: `scripts/tests/ArchitecturePolicy.Tests.ps1`
- Modify: `scripts/ArchitecturePolicy.psm1`
- Modify: `scripts/Check-Architecture.ps1`

- [x] Add a failing fixture where `shell-platform-windows/src/lib.rs` exposes `PlatformEvent` and internal controller reexports.
- [x] Add a compliant fixture exposing only `run_showcase`, `ShowcaseRunConfig`, and `crate_identity` from the platform facade.
- [x] Run `powershell -NoProfile -File scripts/tests/ArchitecturePolicy.Tests.ps1` and confirm the new violation is not detected.
- [x] Implement `Test-PublicFacadePolicy` using declaration-level scanning, not references inside function bodies.
- [x] Run the policy tests and real checkout; the fixture must pass and the current checkout must fail before the facade is reduced.

### Task 2: Internalize shell-platform-windows tests and facade

**Files:**
- Modify: `crates/shell-platform-windows/Cargo.toml`
- Create: `crates/shell-platform-windows/src/integration_tests.rs`
- Modify: `crates/shell-platform-windows/src/lib.rs`
- Modify: all Rust files in `crates/shell-platform-windows/tests/`

- [x] Set `autotests = false` and include each existing test file from `src/integration_tests.rs` under `#[cfg(test)]`.
- [x] Replace `use shell_platform_windows::...` with `use crate::...` in the included tests.
- [x] Change controller, action, routing, motion, event and test-helper reexports from `pub use` to `pub(crate) use`.
- [x] Make `PlatformEvent`, `LifecycleMessage`, `HitRegion`, translation helpers and hit-test helpers crate-private.
- [x] Keep only `run_showcase`, `ShowcaseRunConfig`, and `crate_identity` public by default.
- [x] Run `cargo test -p shell-platform-windows --lib` and correct only visibility/import failures caused by this task.

### Task 3: Reduce shell-renderer exports

**Files:**
- Modify: `crates/shell-renderer/Cargo.toml`
- Create: `crates/shell-renderer/src/integration_tests.rs`
- Modify: `crates/shell-renderer/src/lib.rs`
- Modify: all Rust files in `crates/shell-renderer/tests/`

- [x] Set `autotests = false`, include renderer tests as unit-test modules, and replace `use shell_renderer::...` with `use crate::...`.
- [x] Make unused facade exports crate-private or `cfg(test)`; preserve laid-out result types that are part of public function signatures.
- [x] Preserve exports referenced by `shell-platform-windows` and preserve the `native` Module used by `NativeSurfaceRuntime`.
- [x] Run `cargo test -p shell-renderer --lib` and `cargo test -p shell-platform-windows --lib`.

### Task 4: Final governance and delivery gates

**Files:**
- Modify: `docs/architecture/adr/0002-workspace-dependency-direction.md`
- Modify: `docs/architecture/audits/2026-07-15-change-classification.md`

- [x] Record the narrowed facade and the reason geometry remains owned by renderer but no longer leaks through the platform facade.
- [x] Run `cargo fmt --all -- --check`.
- [x] Run architecture policy tests and `scripts/Check-Architecture.ps1`.
- [x] Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [x] Run `cargo test --workspace`.
- [x] Run `cargo build --workspace --release` and `git diff --check`.
- [x] Do not stage, commit, or push without explicit user authorization.
