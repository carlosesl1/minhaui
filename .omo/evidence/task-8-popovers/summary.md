# Task 8 Popovers Evidence

DoneClaim: token-driven native popovers implemented for calendar/weather-offline, network details, audio/media, control center, system menu, and power/session intents.

## Scenarios

- Shared popover state/navigation/failure containment: `cargo test -p shell-platform-windows --test popover_controller` -> 3 passed, 0 failed.
- Workspace formatting: `cargo fmt --all -- --check` -> exit 0.
- Workspace lint: `cargo clippy --workspace --all-targets --all-features -- -D warnings` -> exit 0.
- Workspace tests: `cargo test --workspace --all-features` -> exit 0.
- Release build: `cargo build --workspace --release` -> exit 0.
- Native 3s smoke: `MINHA_UI_QA_TRACE=1 target\x86_64-pc-windows-msvc\release\shell-app.exe --window-smoke --qa-exit-ms 3000` -> exit 0.

## Binary Observables

- Smoke printed `WINDOW role=popover ... rect=2190,72,360,420 dpi=96 visible=false renderer=hardware`.
- Smoke printed `WINDOW role=popover ... rect=-370,72,360,420 dpi=96 visible=false renderer=hardware`.
- Focused test verified offline weather and adapter failure become `Offline`/`Error` scenes without host crash.
- Focused test verified power/session activation requires confirmation before typed intent emission.
- Focused test verified popover anchoring remains inside a scaled monitor work area.

## Runtime Audit

- Hypothesis: adapter failure can crash host. Evidence: failing provider test returned an error scene and passed.
- Hypothesis: popover window path is missing. Evidence: native smoke printed `role=popover` HWND lines for monitored slots.
- Hypothesis: monitor/DPI anchoring can escape work area. Evidence: focused anchor test passed and smoke printed per-monitor popover rects.

## Notes

- LSP diagnostics could not run because the configured Rust toolchain lacks `rust-analyzer.exe`; compiler, clippy, tests, release build, and native smoke passed.
- New modules/tests and the touched render split are under 250 pure LOC.
