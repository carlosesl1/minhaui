use std::fs;
use std::io;
use std::path::Path;

use crate::persistence::{AtomicPaths, remove_if_exists};
use crate::{ConfigLoad, decode_config};

pub(crate) fn reconcile_staging(paths: &AtomicPaths) -> io::Result<()> {
    let mut destination_valid = read_valid(&paths.destination).is_some();
    let mut backup_valid = read_valid(&paths.backup).is_some();

    if !paths.destination.exists() && read_valid(&paths.rollback).is_some() {
        fs::rename(&paths.rollback, &paths.destination)?;
        destination_valid = true;
    }

    if !backup_valid && read_valid(&paths.backup_temporary).is_some() {
        remove_if_exists(&paths.backup)?;
        fs::rename(&paths.backup_temporary, &paths.backup)?;
        backup_valid = true;
    }

    if !destination_valid
        && !backup_valid
        && !paths.destination.exists()
        && read_valid(&paths.temporary).is_some()
    {
        fs::rename(&paths.temporary, &paths.destination)?;
        destination_valid = true;
    }

    if destination_valid || backup_valid {
        remove_if_exists(&paths.temporary)?;
        remove_if_exists(&paths.backup_temporary)?;
        remove_if_exists(&paths.rollback)?;
    }
    Ok(())
}

pub(crate) fn read_valid(path: &Path) -> Option<ConfigLoad> {
    let metadata = fs::metadata(path).ok()?;
    let size = usize::try_from(metadata.len()).ok()?;
    if size > crate::MAX_CONFIG_BYTES {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    match decode_config(&bytes) {
        load @ (ConfigLoad::Current(_) | ConfigLoad::Migrated { .. }) => Some(load),
        ConfigLoad::Recovered { .. } => None,
    }
}
