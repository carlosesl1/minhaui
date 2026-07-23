# Windows Native Dock

Windows Native Dock, currently branded **Obsidian Glass**, is a local-first
shell companion for Windows 10 22H2 and Windows 11 on x64 hardware. The app
provides a native dock, top bar, popovers, settings, portable themes, safe mode,
and a separate recovery watchdog.

## Repository layout

This directory is the single canonical checkout of the project. Persistent
sibling or nested Git worktrees are not part of the project structure.

```text
Minha UI APP/
|-- crates/
|   |-- shell-app/               Main executable and dependency composition
|   |-- shell-config/            Settings, migrations, and portable themes
|   |-- shell-core/              Pure domain state and reducers
|   |-- shell-platform-windows/  Windows integration and native input
|   |-- shell-renderer/          D3D11 and DirectComposition rendering
|   `-- shell-watchdog/          Recovery and taskbar restoration
|-- docs/                        Product references, specifications, and plans
|-- packaging/
|   |-- msix/                    Microsoft Store and sideloading layout
|   `-- steam/                   Steam distribution layout
|-- .cargo/                      Workspace target and toolchain configuration
|-- .github/                     Continuous-integration workflows
|-- Cargo.toml                   Rust workspace definition
|-- DESIGN.md                    Visual and interaction design system
`-- README.md                    Setup, quality gates, and project map
```

Generated artifacts stay in `target/` or the packaging `out/` directories and
must not be mixed with source files. If a temporary worktree is ever required,
remove it after its changes are integrated into this canonical checkout.

## Workspace architecture

The architecture governance source of truth lives in
[`docs/architecture/`](docs/architecture/README.md). It defines dependency
direction, delivery rules, quality gates, architectural decision records, and
the current stabilization baseline. This README remains a concise project map.

| Package | Responsibility |
| --- | --- |
| `shell-core` | Pure domain types and reducers |
| `shell-config` | Versioned configuration and portable theme bundles |
| `shell-platform-windows` | Documented Windows API adapters |
| `shell-renderer` | D3D11 and DirectComposition rendering boundary |
| `shell-app` | Main process and dependency composition |
| `shell-watchdog` | Recovery and taskbar-restoration process |

`shell-core`, `shell-config`, `shell-app`, and `shell-watchdog` forbid unsafe
Rust. The workspace denies unsafe code by default. A future native adapter may
remove workspace lint inheritance only in `shell-platform-windows` or
`shell-renderer`; that change must isolate each unsafe operation behind a safe
API, document its safety invariant, and add the dedicated Miri or Windows
integration proof before review.

## Prerequisites

- Windows 10 version 22H2 or Windows 11, x64
- Visual Studio 2022 Build Tools or Community with Desktop development with C++
- Windows 10/11 SDK
- Rustup; the checked-in toolchain file installs Rust 1.97.0 for
  `x86_64-pc-windows-msvc` with rustfmt and Clippy

Rustup may require a new terminal before `%USERPROFILE%\.cargo\bin` appears in
`PATH`.

## Quality gates

Run from the repository root:

```powershell
powershell -NoProfile -File scripts/Check-Architecture.ps1
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo build --workspace --release
cargo metadata --format-version 1 --no-deps
cargo audit
cargo deny check advisories bans licenses sources
```

Tests that require installed Windows packages, an interactive desktop, DWM, or
real AppBar registration are intentionally separate from the deterministic
workspace gate:

```powershell
cargo test -p shell-platform-windows --features native-validation --lib
cargo test -p shell-app --features native-validation --test showcase_startup
```

Run native validation only on the controlled `obsidian-native-validation`
Windows image. It must include Calculator and Windows Terminal, have DWM active,
and have no existing Obsidian Glass instance or conflicting AppBar registration.

Release builds use abort-on-panic, fat LTO, one codegen unit, and stripped
symbols. The two process skeletons currently print an explicit bootstrap status
and exit successfully; lifecycle and crash recovery arrive in later milestones.

## Run the app

```powershell
cargo run --release --bin shell-app
cargo run --release --bin shell-watchdog
```

The normal app launch opens the interactive shell. `--safe-mode` disables
expensive visual effects and enables reduced motion while keeping the normal
hardware-first renderer with automatic WARP fallback. Use `--force-warp` only
to explicitly require software rendering. `--high-contrast` and
`--reduced-motion` remain available as accessibility switches.

The lightweight Liquid Glass dock prototype is disabled by default. Enable it
for comparison with:

```powershell
cargo run --release --bin shell-app -- --liquid-glass
```

It affects only the dock material and is suppressed by safe mode and high
contrast. WARP and reduced-motion launches use its static fallback.

The **Apps** module in the top bar lists active applications registered in the
Windows notification area. It refreshes only when opened, reads Explorer's
per-user registration data without changing it, and uses no background polling.
Selecting a row focuses an existing eligible window or opens the already-running
application; it does not reproduce private tray-icon callbacks. If a future
Windows version removes the Explorer registry contract, the popover reports
that background apps are unavailable and the rest of the shell keeps running.

## Build distributable layouts

```powershell
.\packaging\msix\Build-Msix.ps1
.\packaging\steam\Build-SteamLayout.ps1
```

The MSIX output is unsigned by default and ready for a publisher certificate.
Signing secrets are accepted only at build time. Steam VDF files are templates;
replace the application and depot identifiers in the release pipeline.
