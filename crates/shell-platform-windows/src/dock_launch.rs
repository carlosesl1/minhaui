#![deny(unsafe_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use shell_core::{AppId, DockItemId, ShellState};

use crate::DockControllerError;

pub(crate) fn initial_launch_targets(state: &ShellState) -> HashMap<DockItemId, String> {
    state
        .dock_items()
        .iter()
        .map(|item| (item.id(), item.app().as_str().to_owned()))
        .collect()
}

pub(crate) fn dropped_launch_target(path: &str) -> Result<(AppId, String), DockControllerError> {
    let source = Path::new(path);
    let file_name = source
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(DockControllerError::MissingFileName)?;
    let identity = parse_safe_identity(file_name);
    let target = canonical_or_absolute(source);
    Ok((AppId::parse(&identity)?, path_to_string(target)))
}

fn parse_safe_identity(file_name: &str) -> String {
    let mut identity = String::new();
    let mut previous_dash = false;
    for character in file_name.chars().flat_map(char::to_lowercase) {
        let next = if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
            previous_dash = character == '-';
            Some(character)
        } else if previous_dash {
            None
        } else {
            previous_dash = true;
            Some('-')
        };
        if let Some(character) = next {
            identity.push(character);
        }
        if identity.len() == 128 {
            break;
        }
    }
    let trimmed = identity.trim_matches('-').to_owned();
    if trimmed.is_empty() {
        "dropped-app".to_owned()
    } else {
        trimmed
    }
}

fn canonical_or_absolute(source: &Path) -> PathBuf {
    if let Ok(canonical) = std::fs::canonicalize(source) {
        return canonical;
    }
    if source.is_absolute() {
        return source.to_path_buf();
    }
    std::env::current_dir()
        .map(|current| current.join(source))
        .unwrap_or_else(|_| source.to_path_buf())
}

fn path_to_string(path: PathBuf) -> String {
    path.to_string_lossy().into_owned()
}
