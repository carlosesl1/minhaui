use std::ffi::OsStr;
use std::fs;

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
