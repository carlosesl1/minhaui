# Windows Native Dock

Windows Native Dock is a local-first shell companion for Windows 10 22H2 and
Windows 11 on x64 hardware. The V1 workspace is intentionally dependency-free
while its native process, persistence, platform, and rendering boundaries are
being established.

## Workspace architecture

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
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --workspace --release
cargo metadata --format-version 1 --no-deps
cargo audit
cargo deny check advisories bans licenses sources
```

Release builds use abort-on-panic, fat LTO, one codegen unit, and stripped
symbols. The two process skeletons currently print an explicit bootstrap status
and exit successfully; lifecycle and crash recovery arrive in later milestones.

## Run the process skeletons

```powershell
cargo run --release --bin shell-app
cargo run --release --bin shell-watchdog
```
