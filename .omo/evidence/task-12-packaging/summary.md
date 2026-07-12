# Task 12 packaging evidence

## Delivered

- MSIX x64 layout and manifest for Windows 10 22H2 / Windows 11, produced unsigned and ready for a publisher certificate.
- SteamPipe-compatible content layout plus VDF templates without application IDs or credentials.
- Portable Windows x64 ZIP for immediate local testing.
- Update/uninstall recovery entry point and explicit Keep/Export/Remove settings workflow.
- Local-first privacy, support, and dependency licensing notices.
- Normal `shell-app.exe` launch now opens the functional native shell; `--bootstrap` remains an explicit diagnostic mode.

## Verification performed on 2026-07-12

- `cargo fmt --all --check`: PASS
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: PASS
- `cargo test --workspace --all-features`: PASS
- `cargo build --workspace --release`: PASS
- PowerShell parser check for all packaging scripts: PASS
- Steam/portable layout build: PASS
- Windows SDK `MakeAppx.exe` package validation/build: PASS
- Recovery and settings-preservation dry run against the staged watchdog: PASS
- Packaged `shell-app.exe --qa-exit-ms 1200`: PASS using hardware rendering on two enumerated monitors

## Generated local artifacts

- `packaging/msix/out/ObsidianGlass-0.1.0.0-x64.msix`
- `packaging/msix/out/SHA256SUMS.txt`
- `packaging/steam/out/content/`
- `packaging/steam/out/ObsidianGlass-0.1.0-portable-win-x64.zip`

The MSIX intentionally contains no certificate or private key and therefore must be signed before normal installation. Dedicated clean-install/update/uninstall runs on separate Windows 10 and Windows 11 VMs remain part of the final compatibility wave; the local package, manifest, recovery dry run, and executable smoke passed.
