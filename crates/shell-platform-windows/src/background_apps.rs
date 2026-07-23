use std::collections::{HashMap, HashSet};
use std::path::Path;

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
}

impl BackgroundAppEntry {
    fn from_match(registration: &NotificationRegistration, process: &RunningProcess) -> Self {
        Self {
            id: BackgroundAppId::new(fnv1a(normalized_path(&process.executable).as_bytes())),
            process_id: process.process_id,
            label: safe_label(
                &registration.tooltip,
                &process.executable,
                &process.description,
            ),
            executable: process.executable.clone(),
            icon_source: process.executable.clone(),
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
            let process = live.get(&name)?;
            seen.insert(name)
                .then(|| BackgroundAppEntry::from_match(registration, process))
        })
        .collect::<Vec<_>>();

    result.sort_by(|left, right| {
        left.label
            .to_ascii_lowercase()
            .cmp(&right.label.to_ascii_lowercase())
            .then_with(|| left.id.value().cmp(&right.id.value()))
    });
    result.truncate(MAX_BACKGROUND_APPS);
    result
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

#[cfg(test)]
mod tests {
    use super::{
        MAX_BACKGROUND_APPS, NotificationRegistration, RunningProcess, build_background_apps,
        safe_label,
    };

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
}
