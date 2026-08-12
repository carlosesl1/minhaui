# Windows Native Dock

Windows Native Dock, currently branded **Obsidian Glass**, is a local-first
shell companion for Windows 10 22H2 and Windows 11 on x64 hardware. The app
provides a native dock, top bar, popovers, settings, portable themes, safe mode,
and a separate launcher/watchdog.

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
|   `-- shell-watchdog/          Launcher, supervision, and recovery boundary
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
| `shell-watchdog` | Launcher, heartbeat/crash-loop supervision, and recovery boundary |

`shell-core`, `shell-config`, and `shell-app` forbid unsafe Rust. The workspace
denies unsafe code by default. `shell-watchdog` keeps its journal, supervision,
and recovery planner safe; ADR-0011 permits `unsafe` only inside its private,
Windows-only taskbar recovery adapter. Native adapters must isolate every unsafe
operation behind a safe API, document its invariant, and add a dedicated Miri
or Windows integration proof before review.

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
symbols. Normal packaged startup enters `shell-watchdog`, which launches the
sibling `shell-app`, monitors its heartbeat, applies bounded restart backoff,
and makes one safe-mode attempt after a crash loop.

## Run the app

```powershell
cargo build --release -p shell-app -p shell-watchdog
.\target\x86_64-pc-windows-msvc\release\shell-watchdog.exe
```

The watchdog is the normal entry point for packaged and local release runs. It
accepts an alternate child through `--child <path>` and forwards shell arguments
only after `--`, for example
`.\shell-watchdog.exe -- --liquid-glass`. Running `shell-app.exe` directly is a
diagnostic path and bypasses crash-loop supervision.

Release builds use the Windows GUI subsystem, so packaged startup does not leave
a console window open. Debug builds retain the console for diagnostics; the
watchdog heartbeat remains a private pipe between the two release processes.

`--safe-mode` disables
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

## Settings and personalization

Open **Settings** from the top-bar system menu, or choose **Edit controls** in
Quick Settings. The native, resizable Settings window currently supports:

- Dock size, spacing, alignment, magnification, and auto-hide;
- top-bar density and module visibility;
- Quick Controls visibility and keyboard reordering; and
- shared surface opacity and corner radius with live preview.

Changes remain in a draft until **Apply** is selected (`Ctrl+Enter` also
applies). **Cancel**, closing the window, or dismissing the draft restores the
last committed runtime configuration. A successful commit is broadcast to all
monitor slots while preserving the latest pinned Dock layout. If another
monitor commits while a local draft is open, that draft is preserved and
Settings shows an explicit review warning instead of silently replacing it.
Sections whose Windows/runtime contract is not complete remain visibly
unavailable instead of showing controls that do nothing.

The Settings window now exposes its logical navigation and controls through a
server-side Windows UI Automation provider, including Invoke, Toggle, and
RangeValue patterns. Native Narrator/Accessibility Insights validation and
snapshot-diffed focus/property/structure notifications remain release gates;
the Dock, top bar, previews, and popovers do not yet expose equivalent providers.

The **Apps** module in the top bar lists active applications from read-only
per-user notification registrations and the current process snapshot. The
registration location is treated as a capability-probed compatibility fallback,
not as a guaranteed public Windows API.
It refreshes only when opened and uses no background polling. Stable builds do
not inject code into Explorer, install a helper at sign-in, or read Explorer's
private toolbar memory. Selecting a row focuses an eligible existing window or
opens the already-running application. If registrations are unavailable, the
popover reports that background apps are unavailable and the rest of the shell
keeps running.

An unsupported Explorer tray bridge remains source-gated behind the
`experimental-tray-bridge` Cargo feature for isolated research only. It is not
compiled or executed by normal builds and must not be enabled for release,
Store, or end-user packages.

Stable builds also leave the Windows taskbar untouched, including when an old
configuration contains the legacy `hide` policy. The watchdog now provides real
process/heartbeat supervision, a bounded/versioned write-ahead journal, and a
fail-closed native `--restore-only` path for a compatible pending journal. The
native recovery path rediscovers Explorer taskbars, restores the supported V1
state, verifies it, and preserves the journal whenever recovery is uncertain.
Arming the journal before mutation and validating crash recovery on real Windows
are still release gates. Explorer replacement therefore remains source-gated
behind `experimental-taskbar-replacement`, requires the explicit unsafe
validation token `MINHA_UI_UNSAFE_TASKBAR_REPLACEMENT=I_ACCEPT_NO_NATIVE_RECOVERY`,
and is intended only for a disposable, controlled Windows validation image.

Night Light opens the documented `ms-settings:nightlight` Windows page. Stable
builds do not edit private CloudStore records or depend on an unlicensed Git
package to simulate a direct toggle.

## Build distributable layouts

```powershell
.\packaging\msix\Build-Msix.ps1
.\packaging\steam\Build-SteamLayout.ps1
```

The MSIX output is unsigned by default and ready for a publisher certificate.
Signing secrets are accepted only at build time. Steam VDF files are templates;
replace the application and depot identifiers in the release pipeline.
