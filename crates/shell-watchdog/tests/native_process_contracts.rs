#![cfg(windows)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use shell_watchdog::recovery_journal_path;

#[test]
fn restore_only_cli_is_idempotent_and_preserves_invalid_authority() -> Result<(), Box<dyn Error>> {
    let root = TempRoot::create()?;

    let empty = restore_check(root.path(), "update")?;
    assert!(
        empty.status.success(),
        "empty restore check failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&empty.stdout),
        String::from_utf8_lossy(&empty.stderr)
    );
    assert!(String::from_utf8_lossy(&empty.stdout).contains("no pending taskbar recovery journal"));

    let journal = recovery_journal_path(&root.path().join("Minha UI").join("watchdog"));
    fs::create_dir_all(journal.parent().expect("journal has parent"))?;
    let invalid_authority = b"{ this is not a recovery journal }";
    fs::write(&journal, invalid_authority)?;

    let invalid = restore_check(root.path(), "uninstall")?;
    assert!(
        !invalid.status.success(),
        "a corrupt authoritative journal must fail closed"
    );
    assert_eq!(fs::read(journal)?, invalid_authority);
    Ok(())
}

#[test]
fn restore_holder_stays_alive_until_its_stdin_authority_closes() -> Result<(), Box<dyn Error>> {
    // The native holder test requires the Windows session authority used by
    // the production lifecycle gate. It is exercised by the Windows-native
    // validation job; non-Windows hosts cannot emulate named mutex semantics.
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;

    let root = TempRoot::create()?;
    let mut child = Command::new(env!("CARGO_BIN_EXE_shell-watchdog"))
        .args([
            "--restore-only",
            "--hook",
            "update",
            "--hold-until-stdin-eof",
            "--test-state-root",
            root.path().to_str().ok_or("test root is not Unicode")?,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    let mut output = BufReader::new(child.stdout.take().ok_or("missing holder stdout")?);
    let mut line = String::new();
    loop {
        line.clear();
        if output.read_line(&mut line)? == 0 {
            return Err("holder exited before READY".into());
        }
        if line.trim_end() == "MINHA_UI_LIFECYCLE_READY v1" {
            break;
        }
    }
    assert!(child.try_wait()?.is_none());
    drop(child.stdin.take());
    assert!(child.wait()?.success());
    Ok(())
}

fn restore_check(root: &Path, hook: &str) -> std::io::Result<std::process::Output> {
    Command::new(env!("CARGO_BIN_EXE_shell-watchdog"))
        .args([
            "--restore-only",
            "--hook",
            hook,
            "--check-only",
            "--test-state-root",
        ])
        .arg(root)
        .output()
}

struct TempRoot(PathBuf);

static NEXT_TEMP_ROOT: AtomicU64 = AtomicU64::new(1);

impl TempRoot {
    fn create() -> std::io::Result<Self> {
        let root = std::env::temp_dir().join(format!(
            "minha-ui-watchdog-native-contracts-{}-{}",
            std::process::id(),
            NEXT_TEMP_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::remove_dir_all(&root) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        fs::create_dir_all(&root)?;
        Ok(Self(root))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
