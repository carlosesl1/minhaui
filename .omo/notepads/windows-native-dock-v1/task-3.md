# Task 3 durable notes

## Decisions

- `shell-core` remains pure and performs no I/O; reducers return typed effects.
- Config and theme inputs are bounded and parsed into versioned typed values.
- Portable themes are inert JSON envelopes with a deterministic SHA-256 payload checksum.
- Standard-library persistence uses same-directory staging and a public `AtomicWriter` seam.
- Writer startup reconciles crash artifacts before reuse: valid `.previous` restores a missing primary, a valid `.bak.tmp` publishes a missing/invalid backup, and a valid `.tmp` is promoted only when it is the sole usable candidate.
- Staging artifacts are removed only after a valid destination or backup exists. A valid existing destination wins over stale rollback state.

## Accepted debt

- Rust std does not expose Windows `ReplaceFileW`; there remains a multi-rename crash window. The preservation-first recovery path and `.bak` fallback make this explicit and recoverable until a Windows-native writer implements the existing seam.
- Directory metadata is not explicitly synced by std on Windows; file contents are flushed and `sync_all` is called before rename.
- Property generators are intentionally bounded and may be broadened as later tasks add more state/config fields.

## Evidence

- `.omo/evidence/task-3-red.txt`: original feature RED.
- `.omo/evidence/task-3-fix-red.txt`: gate-fix crash-window RED.
- `.omo/evidence/task-3-config-matrix.txt`: release manual QA surface.
- `.omo/evidence/task-3-temp-cleanup.txt`: exact stale directory before/after cleanup.
- `.omo/evidence/task-3-verification.txt`: full toolchain and adversarial verification.
- `.omo/evidence/task-3-code-review.md`: programming and AI-slop review.
- `.omo/evidence/task-3-gate-review.md`: independent rejection that triggered the correction.
