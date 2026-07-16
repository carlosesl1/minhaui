use std::path::{Path, PathBuf};

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};
use windows::Win32::Storage::Packaging::Appx::{
    GetPackagePathByFullName, GetPackagesByPackageFamily,
};
use windows::core::{PCWSTR, PWSTR};

trait PackageCatalog {
    fn package_roots(&self, family: &str) -> Vec<PathBuf>;
}

struct WindowsPackageCatalog;

impl PackageCatalog for WindowsPackageCatalog {
    fn package_roots(&self, family: &str) -> Vec<PathBuf> {
        package_roots(family)
    }
}

pub(crate) fn package_icon_source(aumid: &str) -> Option<String> {
    package_icon_source_with(&WindowsPackageCatalog, aumid)
}

fn package_icon_source_with(catalog: &impl PackageCatalog, aumid: &str) -> Option<String> {
    let (family, app_id) = aumid.split_once('!')?;
    catalog
        .package_roots(family)
        .into_iter()
        .find_map(|root| manifest_logo(&root, app_id))
        .map(|path| format!("image:{}", path.to_string_lossy()))
}

pub(crate) fn package_icon_source_for_executable(executable: &str) -> Option<String> {
    crate::win32_installed_package_icons::icon_source_for_executable(executable)
}

fn package_roots(family: &str) -> Vec<PathBuf> {
    package_full_names(family)
        .into_iter()
        .filter_map(|name| package_path(&name))
        .collect()
}

fn package_full_names(family: &str) -> Vec<String> {
    let wide = family.encode_utf16().chain([0]).collect::<Vec<_>>();
    let mut count = 0_u32;
    let mut buffer_length = 0_u32;
    // SAFETY: Category 8 (FFI boundary). Null outputs request exact buffer sizes.
    let sizing = unsafe {
        GetPackagesByPackageFamily(
            PCWSTR(wide.as_ptr()),
            &mut count,
            None,
            &mut buffer_length,
            None,
        )
    };
    if sizing.0 != 122 || count == 0 || buffer_length == 0 {
        return Vec::new();
    }
    let mut names = vec![PWSTR::null(); count as usize];
    let mut buffer = vec![0_u16; buffer_length as usize];
    // SAFETY: Category 8 (FFI boundary). Pointer and UTF-16 buffers use the sizes
    // returned by the first call and remain live while Windows fills them.
    let result = unsafe {
        GetPackagesByPackageFamily(
            PCWSTR(wide.as_ptr()),
            &mut count,
            Some(names.as_mut_ptr()),
            &mut buffer_length,
            Some(PWSTR(buffer.as_mut_ptr())),
        )
    };
    if result.0 != 0 {
        return Vec::new();
    }
    names
        .into_iter()
        .take(count as usize)
        .filter_map(|name| {
            // SAFETY: Category 8 (FFI boundary). Each pointer targets the live
            // null-terminated buffer populated by GetPackagesByPackageFamily.
            unsafe { name.to_string().ok() }
        })
        .collect()
}

fn package_path(full_name: &str) -> Option<PathBuf> {
    let wide = full_name.encode_utf16().chain([0]).collect::<Vec<_>>();
    let mut length = 0_u32;
    // SAFETY: Category 8 (FFI boundary). Null output requests the path length.
    let sizing = unsafe { GetPackagePathByFullName(PCWSTR(wide.as_ptr()), &mut length, None) };
    if sizing.0 != 122 || length == 0 {
        return None;
    }
    let mut buffer = vec![0_u16; length as usize];
    // SAFETY: Category 8 (FFI boundary). The buffer matches the requested length.
    let result = unsafe {
        GetPackagePathByFullName(
            PCWSTR(wide.as_ptr()),
            &mut length,
            Some(PWSTR(buffer.as_mut_ptr())),
        )
    };
    if result.0 != 0 {
        return None;
    }
    let content_length = buffer
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(buffer.len());
    Some(PathBuf::from(String::from_utf16_lossy(
        &buffer[..content_length],
    )))
}

fn manifest_logo(root: &Path, app_id: &str) -> Option<PathBuf> {
    let manifest = std::fs::read_to_string(root.join("AppxManifest.xml")).ok()?;
    let logo = manifest_logo_attribute(&manifest, app_id)?;
    qualified_logo(root, Path::new(&logo))
}

fn manifest_logo_attribute(manifest: &str, app_id: &str) -> Option<String> {
    let mut reader = Reader::from_str(manifest);
    reader.config_mut().trim_text(true);
    let mut matching_application = false;
    loop {
        match reader.read_event().ok()? {
            Event::Start(node) if node.local_name().as_ref() == b"Application" => {
                matching_application = attribute(&reader, &node, b"Id")
                    .is_some_and(|id| id.eq_ignore_ascii_case(app_id));
            }
            Event::Start(node) | Event::Empty(node)
                if matching_application && node.local_name().as_ref() == b"VisualElements" =>
            {
                return attribute(&reader, &node, b"Square44x44Logo");
            }
            Event::End(node) if node.local_name().as_ref() == b"Application" => {
                matching_application = false;
            }
            Event::Eof => return None,
            _ => {}
        }
    }
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

pub(crate) fn qualified_logo(root: &Path, relative: &Path) -> Option<PathBuf> {
    let base = root.join(relative);
    let parent = base.parent()?;
    let stem = base.file_stem()?.to_str()?;
    let extension = base.extension()?.to_str()?;
    [
        format!("{stem}.targetsize-64_altform-unplated.{extension}"),
        format!("{stem}.targetsize-64.{extension}"),
        base.file_name()?.to_str()?.to_owned(),
        format!("{stem}.scale-200.{extension}"),
    ]
    .into_iter()
    .map(|name| parent.join(name))
    .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::PathBuf;

    #[cfg(feature = "native-validation")]
    use std::path::Path;

    use super::{PackageCatalog, manifest_logo_attribute, package_icon_source_with};
    #[cfg(feature = "native-validation")]
    use super::{
        manifest_logo, package_full_names, package_icon_source, package_icon_source_for_executable,
        package_path,
    };

    #[test]
    fn matches_manifest_application_ids_case_insensitively() {
        let manifest = r#"<Package><Applications><Application Id="App"><uap:VisualElements Square44x44Logo="Assets/Logo.png" /></Application></Applications></Package>"#;

        assert_eq!(
            manifest_logo_attribute(manifest, "app").as_deref(),
            Some("Assets/Logo.png")
        );
    }

    struct FixedPackageCatalog {
        roots: Vec<PathBuf>,
        requested_families: RefCell<Vec<String>>,
    }

    impl PackageCatalog for FixedPackageCatalog {
        fn package_roots(&self, family: &str) -> Vec<PathBuf> {
            self.requested_families.borrow_mut().push(family.to_owned());
            self.roots.clone()
        }
    }

    #[test]
    fn package_icon_resolution_uses_the_injected_catalog() {
        let root = std::env::temp_dir().join(format!(
            "obsidian-package-icon-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let assets = root.join("Assets");
        std::fs::create_dir_all(&assets).expect("fixture assets should be created");
        std::fs::write(
            root.join("AppxManifest.xml"),
            r#"<Package><Applications><Application Id="App"><uap:VisualElements Square44x44Logo="Assets/Logo.png" /></Application></Applications></Package>"#,
        )
        .expect("fixture manifest should be written");
        let logo = assets.join("Logo.targetsize-64.png");
        std::fs::write(&logo, b"fixture").expect("fixture logo should be written");
        let catalog = FixedPackageCatalog {
            roots: vec![root.clone()],
            requested_families: RefCell::new(Vec::new()),
        };

        let source = package_icon_source_with(&catalog, "Example.Family!App");

        assert_eq!(source, Some(format!("image:{}", logo.to_string_lossy())));
        assert_eq!(catalog.requested_families.into_inner(), ["Example.Family"]);
        std::fs::remove_dir_all(root).expect("fixture directory should be removed");
    }

    #[cfg(feature = "native-validation")]
    #[test]
    fn installed_chatgpt_resolves_its_declared_package_logo() {
        let names = package_full_names("OpenAI.Codex_2p2nqsd0c76g0");
        let Some(name) = names.first() else {
            return;
        };
        let root = package_path(name).unwrap_or_else(|| panic!("package path did not resolve"));
        let logo = manifest_logo(&root, "app")
            .unwrap_or_else(|| panic!("manifest logo did not resolve from {}", root.display()));
        assert!(logo.is_file(), "missing {}", logo.display());
        let source = package_icon_source("OpenAI.Codex_2p2nqsd0c76g0!app")
            .unwrap_or_else(|| panic!("installed ChatGPT package logo did not resolve"));
        let path = source
            .strip_prefix("image:")
            .unwrap_or_else(|| panic!("unexpected source {source}"));
        assert!(Path::new(path).is_file(), "missing {path}");
    }

    #[cfg(feature = "native-validation")]
    #[test]
    fn installed_package_executables_resolve_their_declared_windows_icons() {
        for executable in ["WindowsTerminal.exe", "TextInputHost.exe", "ChatGPT.exe"] {
            let source = package_icon_source_for_executable(executable)
                .unwrap_or_else(|| panic!("package icon did not resolve for {executable}"));
            let path = source
                .strip_prefix("image:")
                .unwrap_or_else(|| panic!("unexpected source for {executable}: {source}"));
            assert!(Path::new(path).is_file(), "missing {path}");
        }
    }
}
