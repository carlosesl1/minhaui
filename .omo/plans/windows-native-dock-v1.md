# Windows Native Dock V1 — Execution Plan

## Objective

Deliver a sellable, local-first Windows shell companion for Windows 10 22H2 and Windows 11 x64: a stable, low-resource, highly customizable dock and top bar inspired by macOS interaction language while using original branding, assets, tokens, and implementation.

## Guardrails

- Rust owns process lifecycle, state, persistence, Windows integration, rendering host, and recovery.
- Use documented Windows APIs; undocumented notification-area/tray mirroring is explicitly excluded from V1.
- No accounts, telemetry by default, cloud backend, or server dependency. Themes are portable files.
- Native Windows taskbar changes are opt-in, reversible, and restored after failure/uninstall.
- UI work starts only after `DESIGN.md`; every visual milestone gets real desktop screenshots.
- Tests are failing-first where a seam exists. Each task requires independent verification before completion.

## Architecture

Cargo workspace: `crates/shell-core` (pure domain/reducers), `shell-config` (versioned persistence/theme bundles), `shell-platform-windows` (documented Win32 adapters), `shell-renderer` (D3D11/DirectComposition), `shell-app` (composition/process), `shell-watchdog` (recovery), plus `tools/` and `packaging/`. Unsafe code is isolated in platform/renderer boundaries and documented per block.

## TODOs

- [x] 1. Preserve product references and define the design contract
  - Files: `docs/references/mydockfinder/*.png`, `docs/reference-annex.md`, `DESIGN.md`, `.omo/frontend-design/state.md`.
  - Copy all eight supplied screenshots into documentation-only assets; map dock/previews, topbar/network, weather, audio/media, tray reference, control center, calendar, and system menu. Record that they are reference-only and not shipped.
  - Define original visual tokens, typography (Windows-native), spacing, glass/elevation, primitives/states, motion/reduced-motion, DPI rules, keyboard/focus behavior, personas, and accepted debt.
  - Verify: links resolve and design-token/state tables are complete. Manual QA: contact sheet at `.omo/evidence/task-1-reference-contact-sheet.png`.

- [x] 2. Bootstrap reproducible Rust workspace and quality gates
  - Depends on: 1. Files: `Cargo.toml`, `rust-toolchain.toml`, `.cargo/config.toml`, `rustfmt.toml`, `clippy.toml`, `.gitignore`, `README.md`, `crates/*/Cargo.toml`, `.github/workflows/ci.yml`.
  - Pin stable Rust/MSVC target; deny warnings; configure format, clippy, tests, dependency/license audit, panic/crash policy, and Win10 minimum contract.
  - Verify: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. Manual QA: toolchain transcript `.omo/evidence/task-2-toolchain.txt`.

- [x] 3. Build versioned domain model, reducer, configuration, and theme bundle
  - Depends on: 2. Files: `crates/shell-core/**`, `crates/shell-config/**`, fixtures and unit/property tests.
  - Model monitors, dock items, running indicators, topbar modules, popovers, autohide, taskbar policy, accessibility and performance presets. Use atomic writes, schema migrations, validation, safe defaults, and checksum-aware local theme import/export.
  - Verify malformed/truncated/old/future configs recover without panic; round trips and migrations pass. Manual QA: CLI fixture report `.omo/evidence/task-3-config-matrix.txt`.

- [x] 4. Prove a native transparent shell window and renderer vertical slice
  - Depends on: 2, 3. Files: `crates/shell-renderer/**`, `crates/shell-platform-windows/src/windowing/**`, `crates/shell-app/**`.
  - Create per-monitor DPI-aware, click-correct transparent dock/topbar windows using documented Win32 plus D3D11/DirectComposition; handle device loss, display changes, Explorer restart, sleep/resume, and software-safe fallback.
  - Render a token-driven primitive showcase before feature UI. Verify unit seams plus Windows integration build. Manual QA via real desktop: `.omo/evidence/task-4-native-window.png` and metadata.

- [x] 5. Implement the functional dock interaction loop
  - Depends on: 4. Files: dock scene/controller plus platform process/window adapters.
  - Pinned and running apps, launch/focus/minimize, reorder, separators, running indicators, context menu, drag/drop pinning, magnification, configurable alignment/size/spacing, autohide and reveal hit zone.
  - Ensure animation cannot block input/state thread. Manual QA: scripted desktop scenarios and screenshots in `.omo/evidence/task-5-dock/`.

- [x] 6. Add multi-monitor placement, fullscreen suppression, and window previews
  - Depends on: 5. Support independent monitor placement/DPI, taskbar edge awareness, fullscreen hide policy, preview thumbnails and close/focus actions with graceful degradation where capture is restricted.
  - Verify mixed-DPI topology simulations and hot-plug recovery. Manual QA: `.omo/evidence/task-6-multimonitor/`.

- [x] 7. Implement the top bar shell and reliable base modules
  - Depends on: 4. Files: topbar scene/controllers and documented Windows adapters.
  - App/system menu, clock/date, network throughput/status, volume, power, notification entry point and module ordering/visibility. Use polling budgets and event-driven updates where available.
  - Manual QA: `.omo/evidence/task-7-topbar/` across compact/comfortable density and narrow monitors.

- [x] 8. Implement token-driven popovers
  - Depends on: 7. Calendar, weather (provider abstraction with explicit opt-in/network error state), audio device/volume, media controls, control center, network details, and system/session/power menu.
  - Every popover has keyboard navigation, loading/empty/error/offline states and cannot crash the host when an adapter fails.
  - Manual QA: reference comparison matrix and screenshots `.omo/evidence/task-8-popovers/`.

- [x] 9. Build the settings and live customization experience
  - Depends on: 5, 7, 8. Implement a native settings surface using the same renderer/tokens: dock, topbar, modules, behavior, performance, accessibility, startup, taskbar and advanced recovery.
  - Live preview changes are transactional with Apply/Cancel/Reset; invalid values never reach runtime state. Include original presets plus local theme import/export/share-by-file.
  - Manual QA: `.omo/evidence/task-9-settings/` including corrupt theme rejection and restart persistence.

- [x] 10. Add taskbar policy, watchdog, single-instance, diagnostics, and safe mode
  - Depends on: 3, 4. Implement off/auto-hide/hide policy with explicit consent and an always-available recovery shortcut; restore Explorer taskbar after crash, forced termination, failed startup, update, or uninstall.
  - Separate watchdog process, heartbeat, bounded restart/backoff, crash loop safe mode, local redacted logs and user-exportable diagnostics.
  - Manual QA: kill/crash/Explorer-restart matrix `.omo/evidence/task-10-recovery/` proving taskbar restoration.

- [ ] 11. Harden resource ownership, concurrency, accessibility, and security boundaries
  - Depends on: 5-10. Audit Win32 handles/COM/thread apartments/device resources, cancellation and shutdown. Add keyboard-only paths, focus visuals, screen-reader names where supported, high contrast and reduced motion.
  - Fuzz/negative-test config/theme parsing; reject traversal/oversized payloads and never execute imported theme content.
  - Manual QA: accessibility and long-command interruption artifacts `.omo/evidence/task-11-hardening/`.

- [ ] 12. Package, update, license, and document the commercial V1
  - Depends on: 10, 11. Files: `packaging/msix/**`, `packaging/steam/**`, installer/uninstaller recovery hooks, licenses, privacy and support docs.
  - Produce signed-ready MSIX and Steam-friendly package layouts without embedding credentials; clean uninstall restores Windows state and preserves/export settings by explicit choice.
  - Manual QA: clean install/update/uninstall artifacts `.omo/evidence/task-12-packaging/` on Win10 and Win11 VMs.

## Final Verification Wave

- [ ] F1. Run the complete automated and compatibility matrix
  - Fresh `fmt`, clippy, tests, property tests, audits, release builds and Win10 22H2/Win11 x64 install/smoke tests. Artifact: `.omo/evidence/f1-matrix/`.

- [ ] F2. Prove visual quality and interaction fidelity
  - Capture dock, topbar, every popover and settings at 100/125/150/200% DPI, light/dark wallpaper, high contrast, reduced motion and multi-monitor layouts; review against `DESIGN.md` and the reference annex. Artifact: `.omo/evidence/f2-visual-qa/`.

- [ ] F3. Prove stability, recovery, and performance budgets
  - Run an 8-hour soak plus 200 hide/reveal cycles, 100 popover cycles, repeated monitor hot-plug/display-change simulation, Explorer restart and forced-crash recovery. Targets: idle CPU median <=0.5%, working set <=120 MB default, no unbounded handle/GDI growth, input p95 <=50 ms, animation frame p95 <=16.7 ms on reference hardware. Artifact: `.omo/evidence/f3-performance/`.

- [ ] F4. Complete independent code, architecture, security, scope, and release review
  - All review lanes must PASS; run a runtime debugging audit with at least three plausible failure hypotheses and distinguish them against the actual package. Artifact: `.omo/evidence/f4-release-gate/`.

## Reference Annex Index

1. `codex-clipboard-eeab7336-204b-40fb-bba9-1b1cb70dc7e7.png` — dock, magnification, window/media previews.
2. `codex-clipboard-45e1122d-2092-4252-8463-e1904a46db62.png` — full topbar and network panel.
3. `codex-clipboard-dfb2d294-5b74-4400-a226-a1556d7fea44.png` — weather popover.
4. `codex-clipboard-9cd80f21-c7dd-476c-8544-5c360b3e93f4.png` — audio/media popover.
5. `codex-clipboard-6116c616-8e5c-48ca-9380-567bcf59ad14.png` — tray visual reference only.
6. `codex-clipboard-9ca1fb7d-c71f-4796-a0c3-5b87e30a1105.png` — control center.
7. `codex-clipboard-340e582c-a281-4835-8a54-24c55cc956b9.png` — calendar.
8. `codex-clipboard-f4fc8293-689d-4374-a5f7-8dd75d69f320.png` — system menu.
