use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::native_tray::{NativeTrayIdentity, NativeTrayKey, NativeTraySelector};

pub(crate) const MAX_NOTIFICATION_REGISTRATIONS: usize = 256;
pub(crate) const MAX_BACKGROUND_APPS: usize = 32;
const MAX_LABEL_CHARS: usize = 160;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct BackgroundAppId(u64);

impl BackgroundAppId {
    pub(crate) const fn new(value: u64) -> Self {
        Self(value)
    }

    pub(crate) const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NotificationRegistration {
    executable: String,
    tooltip: String,
}

impl NotificationRegistration {
    pub(crate) fn new(executable: &str, tooltip: &str) -> Self {
        Self {
            executable: executable.to_owned(),
            tooltip: tooltip.to_owned(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RunningProcess {
    process_id: u32,
    executable: String,
    description: String,
}

impl RunningProcess {
    pub(crate) fn new(process_id: u32, executable: &str, description: &str) -> Self {
        Self {
            process_id,
            executable: executable.to_owned(),
            description: description.to_owned(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BackgroundAppEntry {
    id: BackgroundAppId,
    process_id: u32,
    label: String,
    executable: String,
    icon_source: String,
    origin: BackgroundAppOrigin,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BackgroundAppOrigin {
    #[allow(
        dead_code,
        reason = "native tray origin is consumed by the Windows tray adapter incrementally"
    )]
    Native(NativeTrayIdentity),
    RegistryFallback,
}

impl BackgroundAppEntry {
    fn from_match(registration: &NotificationRegistration, process: &RunningProcess) -> Self {
        Self::registry_fallback(
            process.process_id,
            safe_label(
                &registration.tooltip,
                &process.executable,
                &process.description,
            ),
            process.executable.clone(),
            process.executable.clone(),
        )
    }

    pub(crate) fn registry_fallback(
        process_id: u32,
        label: impl Into<String>,
        executable: impl Into<String>,
        icon_source: impl Into<String>,
    ) -> Self {
        let executable = executable.into();
        Self {
            id: BackgroundAppId::new(fnv1a(normalized_path(&executable).as_bytes())),
            process_id,
            label: label.into(),
            executable,
            icon_source: icon_source.into(),
            origin: BackgroundAppOrigin::RegistryFallback,
        }
    }

    #[allow(
        dead_code,
        reason = "native tray constructor is consumed by the adapter incrementally"
    )]
    pub(crate) fn native(
        identity: NativeTrayIdentity,
        label: impl Into<String>,
        executable: impl Into<String>,
        icon_source: impl Into<String>,
    ) -> Self {
        let executable = executable.into();
        Self {
            id: BackgroundAppId::new(native_stable_id(identity)),
            process_id: identity.owner_process_id(),
            label: label.into(),
            executable,
            icon_source: icon_source.into(),
            origin: BackgroundAppOrigin::Native(identity),
        }
    }

    pub(crate) const fn id(&self) -> BackgroundAppId {
        self.id
    }

    pub(crate) const fn process_id(&self) -> u32 {
        self.process_id
    }

    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    pub(crate) fn executable(&self) -> &str {
        &self.executable
    }

    pub(crate) fn icon_source(&self) -> &str {
        &self.icon_source
    }

    #[allow(
        dead_code,
        reason = "native tray origin accessor is consumed by the adapter incrementally"
    )]
    pub(crate) fn origin(&self) -> &BackgroundAppOrigin {
        &self.origin
    }
}

pub(crate) fn build_background_apps(
    registrations: &[NotificationRegistration],
    processes: &[RunningProcess],
) -> Vec<BackgroundAppEntry> {
    let live = processes
        .iter()
        .filter_map(|process| executable_name(&process.executable).map(|name| (name, process)))
        .collect::<HashMap<_, _>>();
    let mut seen = HashSet::new();
    let mut result = registrations
        .iter()
        .take(MAX_NOTIFICATION_REGISTRATIONS)
        .filter_map(|registration| {
            let name = executable_name(&registration.executable)?;
            if registry_fallback_is_excluded(&name) {
                return None;
            }
            let process = live.get(&name)?;
            seen.insert(name)
                .then(|| BackgroundAppEntry::from_match(registration, process))
        })
        .collect::<Vec<_>>();

    sort_and_bound(&mut result);
    result
}

/// Combines native tray observations with the registry/process fallback.
/// Native entries win by normalized executable path, while distinct native
/// identities from one executable remain visible.
#[allow(
    dead_code,
    reason = "native tray merge is consumed by the Windows tray adapter incrementally"
)]
pub(crate) fn merge_background_apps(
    native: impl AsRef<[BackgroundAppEntry]>,
    fallback: impl AsRef<[BackgroundAppEntry]>,
) -> Vec<BackgroundAppEntry> {
    let native = native.as_ref();
    let fallback = fallback.as_ref();
    let mut result = Vec::with_capacity(native.len().saturating_add(fallback.len()));
    let mut native_paths = HashSet::new();
    let mut native_identities = HashSet::<NativeTrayKey>::new();

    for entry in native.iter().chain(fallback) {
        if background_app_is_excluded(entry) {
            continue;
        }
        let identity = match entry.origin() {
            BackgroundAppOrigin::Native(identity) => identity,
            BackgroundAppOrigin::RegistryFallback => continue,
        };
        if native_identities.insert(identity.logical_key()) {
            native_paths.insert(normalized_path(entry.executable()));
            result.push(entry.clone());
        }
    }

    let mut fallback_paths = HashSet::new();
    for entry in native.iter().chain(fallback) {
        if background_app_is_excluded(entry) {
            continue;
        }
        if !matches!(entry.origin(), BackgroundAppOrigin::RegistryFallback) {
            continue;
        }
        let path = normalized_path(entry.executable());
        if native_paths.contains(&path) || !fallback_paths.insert(path) {
            continue;
        }
        result.push(entry.clone());
    }

    sort_and_bound(&mut result);
    result
}

fn sort_and_bound(entries: &mut Vec<BackgroundAppEntry>) {
    entries.sort_by(|left, right| {
        left.label
            .to_ascii_lowercase()
            .cmp(&right.label.to_ascii_lowercase())
            .then_with(|| left.id.value().cmp(&right.id.value()))
    });
    entries.truncate(MAX_BACKGROUND_APPS);
}

pub(crate) fn safe_label(tooltip: &str, executable: &str, description: &str) -> String {
    let candidate = tooltip
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .or_else(|| (!description.trim().is_empty()).then(|| description.trim()))
        .unwrap_or_else(|| executable_stem(executable));

    candidate.chars().take(MAX_LABEL_CHARS).collect()
}

fn executable_name(path: &str) -> Option<String> {
    Path::new(path.trim())
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_ascii_lowercase)
}

fn registry_fallback_is_excluded(executable_name: &str) -> bool {
    shell_component_is_excluded(executable_name)
        || matches!(
            executable_name,
            "vivaldi.exe"
                | "cmd.exe"
                | "conhost.exe"
                | "powershell.exe"
                | "pwsh.exe"
                | "cscript.exe"
                | "wscript.exe"
                | "mshta.exe"
                | "rundll32.exe"
                | "dllhost.exe"
                | "node.exe"
                | "python.exe"
                | "pythonw.exe"
                | "java.exe"
                | "javaw.exe"
        )
}

fn background_app_is_excluded(entry: &BackgroundAppEntry) -> bool {
    let Some(executable_name) = executable_name(entry.executable()) else {
        return true;
    };
    match entry.origin() {
        BackgroundAppOrigin::Native(_) => shell_component_is_excluded(&executable_name),
        BackgroundAppOrigin::RegistryFallback => registry_fallback_is_excluded(&executable_name),
    }
}

fn shell_component_is_excluded(executable_name: &str) -> bool {
    matches!(
        executable_name,
        "explorer.exe"
            | "shell-app.exe"
            | "obsidian_tray_bridge_host.exe"
            | "shellexperiencehost.exe"
            | "startmenuexperiencehost.exe"
            | "searchhost.exe"
            | "textinputhost.exe"
            | "runtimebroker.exe"
            | "backgroundtaskhost.exe"
    )
}

fn executable_stem(path: &str) -> &str {
    Path::new(path.trim())
        .file_stem()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("Aplicativo")
}

fn normalized_path(path: &str) -> String {
    path.trim().replace('/', "\\").to_ascii_lowercase()
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[allow(
    dead_code,
    reason = "native tray stable IDs are consumed by the Windows tray adapter incrementally"
)]
fn native_stable_id(identity: NativeTrayIdentity) -> u64 {
    let key = identity.logical_key();
    let mut bytes = Vec::with_capacity(1 + 8 + 16);
    bytes.extend_from_slice(&key.owner_window().value().to_le_bytes());
    match key.selector() {
        NativeTraySelector::Guid(guid) => {
            bytes.push(1);
            bytes.extend_from_slice(&guid);
        }
        NativeTraySelector::IconId(icon_id) => {
            bytes.push(0);
            bytes.extend_from_slice(&icon_id.to_le_bytes());
        }
    }
    fnv1a(&bytes)
}

#[cfg(test)]
mod tests {
    use super::{
        BackgroundAppEntry, BackgroundAppOrigin, MAX_BACKGROUND_APPS, NotificationRegistration,
        RunningProcess, build_background_apps, merge_background_apps, safe_label,
    };
    use crate::native_event_route::NativeWindowId;
    use crate::native_tray::NativeTrayIdentity;

    fn native(window: isize, icon: u32, executable: &str, label: &str) -> BackgroundAppEntry {
        native_with_identity(
            NativeTrayIdentity::new(NativeWindowId::new(window), 42, icon, 0x8001, 4, None, 1)
                .expect("valid identity"),
            executable,
            label,
        )
    }

    fn native_with_identity(
        identity: NativeTrayIdentity,
        executable: &str,
        label: &str,
    ) -> BackgroundAppEntry {
        BackgroundAppEntry::native(identity, label, executable, executable)
    }

    #[test]
    fn catalog_keeps_only_live_registered_executables() {
        let registrations = vec![
            NotificationRegistration::new(
                r"{6D809377-6AF0-444B-8957-A3773F02200E}\AMD\RadeonSoftware.exe",
                "",
            ),
            NotificationRegistration::new(r"C:\Old\Discord.exe", ""),
            NotificationRegistration::new(r"C:\Apps\Steam.exe", "Steam"),
            NotificationRegistration::new(r"C:\Apps\Steam.exe", "Steam duplicate"),
        ];
        let processes = vec![
            RunningProcess::new(
                10,
                r"C:\Program Files\AMD\RadeonSoftware.exe",
                "AMD Software",
            ),
            RunningProcess::new(11, r"D:\Steam\Steam.exe", "Steam"),
        ];

        let catalog = build_background_apps(&registrations, &processes);
        let labels = catalog
            .iter()
            .map(|entry| entry.label())
            .collect::<Vec<_>>();

        assert_eq!(labels, ["AMD Software", "Steam"]);
        assert_eq!(catalog[0].process_id(), 10);
        assert_eq!(
            catalog[0].icon_source(),
            r"C:\Program Files\AMD\RadeonSoftware.exe"
        );
        assert_eq!(catalog[1].executable(), r"D:\Steam\Steam.exe");
        assert_ne!(catalog[0].id(), catalog[1].id());
    }

    #[test]
    fn registry_fallback_excludes_shell_browser_and_shared_script_hosts() {
        let registrations = vec![
            NotificationRegistration::new(r"C:\Windows\explorer.exe", ""),
            NotificationRegistration::new(r"C:\Apps\Vivaldi\vivaldi.exe", ""),
            NotificationRegistration::new(
                r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
                "Obsidian Bridge Persistence QA",
            ),
            NotificationRegistration::new(r"C:\Apps\Realtek\RtkNGUI64.exe", "Realtek"),
            NotificationRegistration::new(r"C:\Windows\System32\SecurityHealthSystray.exe", ""),
        ];
        let processes = vec![
            RunningProcess::new(10, r"C:\Windows\explorer.exe", ""),
            RunningProcess::new(11, r"C:\Apps\Vivaldi\vivaldi.exe", ""),
            RunningProcess::new(
                12,
                r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
                "",
            ),
            RunningProcess::new(13, r"C:\Apps\Realtek\RtkNGUI64.exe", ""),
            RunningProcess::new(14, r"C:\Windows\System32\SecurityHealthSystray.exe", ""),
        ];

        assert_eq!(
            build_background_apps(&registrations, &processes)
                .iter()
                .map(|entry| entry.label())
                .collect::<Vec<_>>(),
            ["Realtek", "SecurityHealthSystray"]
        );
    }

    #[test]
    fn native_merge_excludes_shell_owned_icons_without_hiding_real_apps() {
        let native = vec![
            native(7, 100, r"C:\Windows\explorer.exe", "Fone: 22%"),
            native(8, 1, r"C:\Apps\AMD\RadeonSoftware.exe", "AMD Software"),
        ];

        assert_eq!(
            merge_background_apps(&native, &[])
                .iter()
                .map(|entry| entry.label())
                .collect::<Vec<_>>(),
            ["AMD Software"]
        );
    }

    #[test]
    fn tooltip_is_bounded_to_its_first_nonempty_line() {
        assert_eq!(
            safe_label(" Zoom - Signed in\r\nPrivate detail ", "Zoom.exe", "Zoom"),
            "Zoom - Signed in"
        );
        assert_eq!(safe_label("", "Discord.exe", ""), "Discord");
    }

    #[test]
    fn catalog_is_sorted_and_bounded() {
        let registrations = (0..40)
            .map(|index| {
                NotificationRegistration::new(
                    &format!(r"C:\Apps\app{index:02}.exe"),
                    &format!("App {index:02}"),
                )
            })
            .collect::<Vec<_>>();
        let processes = (0..40)
            .map(|index| {
                RunningProcess::new(
                    index + 1,
                    &format!(r"D:\Live\app{index:02}.exe"),
                    &format!("App {index:02}"),
                )
            })
            .collect::<Vec<_>>();

        let catalog = build_background_apps(&registrations, &processes);

        assert_eq!(catalog.len(), MAX_BACKGROUND_APPS);
        assert_eq!(catalog.first().map(|entry| entry.label()), Some("App 00"));
        assert_eq!(catalog.last().map(|entry| entry.label()), Some("App 31"));
    }

    #[test]
    fn native_entries_win_over_fallback_for_the_same_normalized_executable() {
        let native = vec![native(7, 1, r"C:\Apps\Discord.exe", "Discord native")];
        let fallback = vec![BackgroundAppEntry::registry_fallback(
            42,
            "Discord fallback",
            r"c:/apps/discord.exe",
            r"c:/apps/discord.exe",
        )];

        let merged = merge_background_apps(&native, &fallback);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].label(), "Discord native");
        assert!(matches!(merged[0].origin(), BackgroundAppOrigin::Native(_)));
    }

    #[test]
    fn native_entries_from_a_secondary_capture_are_not_discarded() {
        let toolbar = vec![native(7, 1, r"C:\Apps\Discord.exe", "Discord toolbar")];
        let bridge = vec![native(8, 2, r"C:\Apps\RadeonSoftware.exe", "AMD Software")];

        let merged = merge_background_apps(&toolbar, &bridge);

        assert_eq!(merged.len(), 2);
        assert!(merged.iter().any(|entry| entry.label() == "AMD Software"));
        assert!(
            merged
                .iter()
                .all(|entry| matches!(entry.origin(), BackgroundAppOrigin::Native(_)))
        );
    }

    #[test]
    fn native_icons_from_one_executable_remain_distinct() {
        let native = vec![
            native(7, 1, r"C:\Apps\Chat.exe", "Chat one"),
            native(7, 2, r"C:\Apps\Chat.exe", "Chat two"),
            native(8, 1, r"C:\Apps\Chat.exe", "Chat three"),
        ];

        let merged = merge_background_apps(&native, &[]);

        assert_eq!(merged.len(), 3);
        assert_ne!(merged[0].id(), merged[1].id());
        assert_ne!(merged[1].id(), merged[2].id());
    }

    #[test]
    fn exact_duplicate_native_identity_is_emitted_once() {
        let entry = native(7, 1, r"C:\Apps\Chat.exe", "Chat");
        let merged = merge_background_apps(&[entry.clone(), entry], &[]);
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn guid_precedence_deduplicates_metadata_changes_and_keeps_id_stable() {
        let guid = Some([0x11; 16]);
        let first = native_with_identity(
            NativeTrayIdentity::new(NativeWindowId::new(7), 42, 1, 0x8001, 1, guid, 10)
                .expect("valid identity"),
            r"C:\Apps\Chat.exe",
            "Chat first",
        );
        let updated = native_with_identity(
            NativeTrayIdentity::new(NativeWindowId::new(7), 99, 999, 0x9001, 2, guid, 11)
                .expect("valid identity"),
            r"C:\Apps\Chat.exe",
            "Chat updated",
        );

        assert_eq!(first.id(), updated.id());
        let merged = merge_background_apps(&[first, updated], &[]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].label(), "Chat first");
    }

    #[test]
    fn distinct_guids_remain_distinct_native_icons() {
        let first = native_with_identity(
            NativeTrayIdentity::new(
                NativeWindowId::new(7),
                42,
                1,
                0x8001,
                1,
                Some([0x11; 16]),
                1,
            )
            .expect("valid identity"),
            r"C:\Apps\Chat.exe",
            "Chat one",
        );
        let second = native_with_identity(
            NativeTrayIdentity::new(
                NativeWindowId::new(7),
                42,
                1,
                0x8001,
                1,
                Some([0x22; 16]),
                1,
            )
            .expect("valid identity"),
            r"C:\Apps\Chat.exe",
            "Chat two",
        );

        assert_ne!(first.id(), second.id());
        assert_eq!(merge_background_apps(&[first, second], &[]).len(), 2);
    }

    #[test]
    fn merge_is_sorted_and_bounded() {
        let native_entries = (0..20)
            .map(|index| {
                native(
                    index + 1,
                    index as u32,
                    &format!(r"C:\Native\n{index}.exe"),
                    &format!("Native {index:02}"),
                )
            })
            .collect::<Vec<_>>();
        let fallback_entries = (0..20)
            .map(|index| {
                BackgroundAppEntry::registry_fallback(
                    index + 1,
                    format!("Fallback {index:02}"),
                    format!(r"C:\Fallback\f{index}.exe"),
                    format!(r"C:\Fallback\f{index}.exe"),
                )
            })
            .collect::<Vec<_>>();

        let merged = merge_background_apps(&native_entries, &fallback_entries);

        assert_eq!(merged.len(), MAX_BACKGROUND_APPS);
        assert!(merged.windows(2).all(|pair| {
            pair[0].label().to_ascii_lowercase() <= pair[1].label().to_ascii_lowercase()
        }));
    }
}
