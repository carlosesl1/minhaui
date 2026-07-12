# manualQa

## surfaceEvidence
| scenario id | criterion reference | surface | exact invocation | verdict | artifactRefs |
|---|---|---|---|---|---|
| S1 | mixed-DPI signed-coordinate topology simulation | Rust test harness | `cargo test -p shell-platform-windows --test multimonitor_placement --test window_previews` | PASS | A1 |
| S2 | fullscreen per-monitor suppression | Rust test harness | `cargo test -p shell-platform-windows --test multimonitor_placement --test window_previews` | PASS | A1 |
| S3 | preview available/restricted states and focus/close actions | Rust test harness | `cargo test -p shell-platform-windows --test multimonitor_placement --test window_previews` | PASS | A1 |
| S4 | manual desktop QA with physical monitor count | Windows display enumeration plus desktop GUI smoke | `Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.Screen]::AllScreens; Get-CimInstance -Namespace root\wmi -ClassName WmiMonitorBasicDisplayParams`; `$env:MINHA_UI_QA_TRACE=1; target/x86_64-pc-windows-msvc/release/shell-app.exe --window-smoke --force-warp --simulate-lifecycle-events --qa-exit-ms 4000` | PASS | A2, A3, A4 |
| S5 | cleanup | Windows process table | `Get-Process shell-app -ErrorAction SilentlyContinue` | PASS | A5 |

## adversarialCases
| scenario id | criterion reference | adversarial class | expected behavior | verdict | artifactRefs |
|---|---|---|---|---|---|
| ADV1 | mixed-DPI signed-coordinate topology simulation | secondary monitor left of origin with 144 DPI | placement keeps signed negative coordinates and per-monitor DPI sizing | PASS | A1 |
| ADV2 | fullscreen per-monitor suppression | fullscreen window on one monitor in multi-monitor topology | only covered monitor is suppressed; other monitor remains visible | PASS | A1 |
| ADV3 | preview available/restricted states | capture restricted window | renderer receives explicit restricted preview state, not a fake available bitmap | PASS | A1 |
| ADV4 | preview focus/close actions | focus and close selected from preview | actions are queued as platform effects without blocking | PASS | A1 |
| ADV5 | physical desktop topology | two active physical monitors with negative-coordinate secondary | monitor enumeration reports two active displays and smoke log emits two monitor placements | PASS | A2, A3, A4 |
| ADV6 | cleanup after auto-exit | release binary should terminate after QA timer | no `shell-app` process remains | PASS | A5 |

## artifactRefs
| id | kind | description | path |
|---|---|---|---|
| A1 | terminal transcript | Reproduced targeted Rust tests for placement/fullscreen/previews. | `.omo/evidence/task-6-multimonitor/qa-review/qa-rust-targeted-tests.txt` |
| A2 | terminal transcript | Independent Windows Forms and WMI monitor enumeration, including two active displays and negative X secondary. | `.omo/evidence/task-6-multimonitor/qa-review/qa-monitor-enumeration.txt` |
| A3 | terminal transcript | Redacted release binary desktop smoke trace with two `MONITOR_PLACEMENT` lines, window creation, lifecycle resources, and exit code 0. | `.omo/evidence/task-6-multimonitor/qa-review/qa-desktop-window-smoke.txt` |
| A4 | privacy inspection note | Screenshot removed after privacy inspection; use redacted text/manual QA logs for native window evidence. | `.omo/evidence/task-6-multimonitor/qa-review/qa-screenshot-inspection.txt` |
| A5 | terminal transcript | Cleanup check showing no `shell-app` process remains. | `.omo/evidence/task-6-multimonitor/qa-review/qa-cleanup.txt` |
| A6 | artifact index | File sizes and SHA-256 hashes for QA review artifacts. | `.omo/evidence/task-6-multimonitor/qa-review/qa-artifact-index.txt` |
