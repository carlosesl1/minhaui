use std::ffi::OsStr;
use std::fs;

#[cfg(windows)]
use std::path::Path;
#[cfg(windows)]
use std::process::{Command, Stdio};
#[cfg(windows)]
use std::thread;
#[cfg(windows)]
use std::time::{Duration, Instant};

use shell_app::{InstanceOwnership, acquire_instance_lock, instance_lock_path};

#[test]
fn lock_path_is_scoped_to_the_windows_session() {
    let root = std::env::temp_dir();
    let session_three = instance_lock_path(&root, 3);
    let session_seven = instance_lock_path(&root, 7);

    assert_ne!(session_three, session_seven);
    assert_eq!(
        session_seven.file_name(),
        Some(OsStr::new("shell-app-session-7.lock"))
    );
    assert_eq!(
        session_seven.parent().and_then(|parent| parent.file_name()),
        Some(OsStr::new("Minha UI"))
    );
    assert_eq!(instance_lock_path(&root, 7), session_seven);
}

#[test]
fn a_second_owner_is_rejected_until_the_primary_exits() -> Result<(), Box<dyn std::error::Error>> {
    let root =
        std::env::temp_dir().join(format!("minha-ui-single-instance-{}", std::process::id()));
    let path = root.join("nested").join("shell.lock");

    let first = acquire_instance_lock(&path)?;
    assert!(matches!(first, InstanceOwnership::Primary(_)));
    assert!(path.exists());
    assert!(matches!(
        acquire_instance_lock(&path)?,
        InstanceOwnership::Existing
    ));

    drop(first);
    assert!(matches!(
        acquire_instance_lock(&path)?,
        InstanceOwnership::Primary(_)
    ));
    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(windows)]
#[test]
fn a_cross_process_owner_is_rejected_and_abnormal_exit_releases_the_lock()
-> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!(
        "minha-ui-single-instance-process-{}",
        std::process::id()
    ));
    let path = root.join("shell.lock");
    let ready = root.join("owner.ready");
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    fs::create_dir_all(&root)?;

    let mut owner = KillOnDrop(
        Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "cross_process_instance_lock_owner_helper",
                "--nocapture",
            ])
            .env("MINHA_UI_TEST_INSTANCE_LOCK", &path)
            .env("MINHA_UI_TEST_INSTANCE_READY", &ready)
            .stdin(Stdio::null())
            .spawn()?,
    );

    wait_for_helper_ready(&mut owner.0, &ready, Duration::from_secs(10))?;
    assert!(matches!(
        acquire_instance_lock(&path)?,
        InstanceOwnership::Existing
    ));

    owner.0.kill()?;
    let status = owner.0.wait()?;
    assert!(!status.success(), "the helper must be terminated abruptly");

    let ownership = acquire_primary_after_exit(&path, Duration::from_secs(5))?;
    assert!(matches!(ownership, InstanceOwnership::Primary(_)));
    drop(ownership);
    fs::remove_dir_all(root)?;
    Ok(())
}

/// Subprocess entry point used by the cross-process Windows lock test.
///
/// It is intentionally a normal test instead of a shipped helper binary. A
/// regular test run has none of the private environment variables and returns
/// immediately.
#[cfg(windows)]
#[test]
fn cross_process_instance_lock_owner_helper() -> Result<(), Box<dyn std::error::Error>> {
    let Some(path) = std::env::var_os("MINHA_UI_TEST_INSTANCE_LOCK") else {
        return Ok(());
    };
    let ready = std::env::var_os("MINHA_UI_TEST_INSTANCE_READY")
        .ok_or_else(|| std::io::Error::other("missing helper readiness path"))?;
    let ownership = acquire_instance_lock(Path::new(&path))?;
    assert!(matches!(ownership, InstanceOwnership::Primary(_)));
    fs::write(ready, b"ready")?;

    // The parent intentionally terminates this process to prove that Windows
    // releases the kernel-backed file lock after an abnormal process exit.
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

#[cfg(windows)]
struct KillOnDrop(std::process::Child);

#[cfg(windows)]
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[cfg(windows)]
fn wait_for_helper_ready(
    child: &mut std::process::Child,
    path: &Path,
    timeout: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = Instant::now() + timeout;
    while !path.exists() {
        if let Some(status) = child.try_wait()? {
            return Err(
                format!("instance-lock helper exited before readiness with {status}").into(),
            );
        }
        if Instant::now() >= deadline {
            return Err(format!("timed out waiting for {}", path.display()).into());
        }
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

#[cfg(windows)]
fn acquire_primary_after_exit(
    path: &Path,
    timeout: Duration,
) -> Result<InstanceOwnership, Box<dyn std::error::Error>> {
    let deadline = Instant::now() + timeout;
    loop {
        let ownership = acquire_instance_lock(path)?;
        if ownership.is_primary() {
            return Ok(ownership);
        }
        if Instant::now() >= deadline {
            return Err("the instance lock remained owned after process termination".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
}
