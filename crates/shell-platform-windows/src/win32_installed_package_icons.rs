#![deny(unsafe_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};
use windows::Management::Deployment::PackageManager;
use windows::core::HSTRING;

static INSTALLED_PACKAGE_ICONS: OnceLock<HashMap<String, String>> = OnceLock::new();

pub(super) fn icon_source_for_executable(executable: &str) -> Option<String> {
    let executable = executable_file_name(executable)?.to_ascii_lowercase();
    installed_package_icons().get(&executable).cloned()
}

fn installed_package_icons() -> &'static HashMap<String, String> {
    INSTALLED_PACKAGE_ICONS.get_or_init(|| {
        installed_package_roots()
            .into_iter()
            .flat_map(|root| manifest_icon_entries(&root))
            .collect()
    })
}

fn installed_package_roots() -> Vec<PathBuf> {
    let Some(packages) = PackageManager::new()
        .and_then(|manager| manager.FindPackagesByUserSecurityId(&HSTRING::new()))
        .ok()
    else {
        return Vec::new();
    };
    packages
        .into_iter()
        .filter_map(|package| package.InstalledLocation().ok())
        .filter_map(|location| location.Path().ok())
        .map(|path| PathBuf::from(path.to_string()))
        .collect()
}

fn manifest_icon_entries(root: &Path) -> Vec<(String, String)> {
    let Some(manifest) = std::fs::read_to_string(root.join("AppxManifest.xml")).ok() else {
        return Vec::new();
    };
    manifest_executable_logos(&manifest)
        .into_iter()
        .filter_map(|(executable, logo)| {
            let executable = executable_file_name(&executable)?.to_ascii_lowercase();
            let source = crate::win32_package_icon::qualified_logo(root, Path::new(&logo))?;
            Some((executable, format!("image:{}", source.to_string_lossy())))
        })
        .collect()
}

fn manifest_executable_logos(manifest: &str) -> Vec<(String, String)> {
    let mut reader = Reader::from_str(manifest);
    reader.config_mut().trim_text(true);
    let mut executable = None;
    let mut entries = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(node)) if node.local_name().as_ref() == b"Application" => {
                executable = attribute(&reader, &node, b"Executable");
            }
            Ok(Event::Start(node) | Event::Empty(node))
                if executable.is_some() && node.local_name().as_ref() == b"VisualElements" =>
            {
                if let (Some(executable), Some(logo)) = (
                    executable.take(),
                    attribute(&reader, &node, b"Square44x44Logo"),
                ) {
                    entries.push((executable, logo));
                }
            }
            Ok(Event::End(node)) if node.local_name().as_ref() == b"Application" => {
                executable = None;
            }
            Ok(Event::Eof) | Err(_) => return entries,
            Ok(_) => {}
        }
    }
}

fn executable_file_name(executable: &str) -> Option<String> {
    Path::new(executable.replace('/', "\\").as_str())
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
}

fn attribute(reader: &Reader<&[u8]>, node: &BytesStart<'_>, name: &[u8]) -> Option<String> {
    node.attributes().flatten().find_map(|attribute| {
        (attribute.key.local_name().as_ref() == name)
            .then(|| {
                attribute
                    .decoded_and_normalized_value(XmlVersion::Implicit1_0, reader.decoder())
                    .ok()
            })
            .flatten()
            .map(|value| value.into_owned())
    })
}
