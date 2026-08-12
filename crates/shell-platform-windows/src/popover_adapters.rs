#![deny(unsafe_code)]

use shell_core::Popover;

use crate::background_apps::BackgroundAppEntry;
use crate::{
    PopoverAction, PopoverDataError, PopoverDataProvider, PopoverItem, PopoverLoadState,
    PopoverPayload, ProjectionMode, SessionAction, SystemRoute, TopbarSnapshot, WeatherAccess,
    WeatherItem, WeatherProvider,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct OfflineWeatherProvider;

impl WeatherProvider for OfflineWeatherProvider {
    fn forecast(&self) -> Result<WeatherItem, PopoverDataError> {
        Err(PopoverDataError::Adapter("weather provider is offline"))
    }
}

pub(crate) fn background_app_items(entries: &[BackgroundAppEntry]) -> Vec<PopoverItem> {
    entries
        .iter()
        .map(|entry| {
            PopoverItem::new(
                entry.label(),
                "",
                true,
                Some(PopoverAction::OpenBackgroundApp(entry.id())),
            )
            .with_icon_source(Some(entry.icon_source().to_owned()))
        })
        .collect()
}

#[derive(Clone, Copy, Debug)]
pub struct DefaultPopoverDataProvider<W> {
    weather: W,
    weather_access: WeatherAccess,
}

impl Default for DefaultPopoverDataProvider<OfflineWeatherProvider> {
    fn default() -> Self {
        Self::offline()
    }
}

impl DefaultPopoverDataProvider<OfflineWeatherProvider> {
    #[must_use]
    pub const fn offline() -> Self {
        Self {
            weather: OfflineWeatherProvider,
            weather_access: WeatherAccess::Offline,
        }
    }
}

impl<W: WeatherProvider> DefaultPopoverDataProvider<W> {
    #[must_use]
    #[expect(
        dead_code,
        reason = "opt-in weather remains an adapter seam while offline is the product default"
    )]
    pub const fn with_weather(weather: W, weather_access: WeatherAccess) -> Self {
        Self {
            weather,
            weather_access,
        }
    }
}

impl<W: WeatherProvider> PopoverDataProvider for DefaultPopoverDataProvider<W> {
    fn load(
        &self,
        kind: Popover,
        snapshot: &TopbarSnapshot,
        calendar_offset: i16,
    ) -> Result<PopoverPayload, PopoverDataError> {
        let state = match kind {
            Popover::SystemMenu => PopoverLoadState::Ready(system_rows()),
            Popover::Calendar => self.calendar_rows(snapshot, calendar_offset)?,
            Popover::Network => PopoverLoadState::Ready(network_rows(snapshot)),
            Popover::Volume => PopoverLoadState::Ready(audio_rows(snapshot)),
            Popover::Power => PopoverLoadState::Ready(power_rows(snapshot)),
            Popover::Notifications => PopoverLoadState::Ready(control_center_rows()),
            Popover::QuickSettings => PopoverLoadState::Empty,
            Popover::BackgroundApps => PopoverLoadState::Loading,
        };
        Ok(PopoverPayload::new(kind, state))
    }
}

impl<W: WeatherProvider> DefaultPopoverDataProvider<W> {
    fn calendar_rows(
        &self,
        snapshot: &TopbarSnapshot,
        calendar_offset: i16,
    ) -> Result<PopoverLoadState, PopoverDataError> {
        let today = snapshot.local_date();
        let month = shell_core::CalendarMonth::new(today.year(), today.month())
            .unwrap_or_default()
            .shifted(calendar_offset);
        let mut rows = vec![
            PopoverItem::new(
                "Previous month",
                "",
                true,
                Some(PopoverAction::CalendarPrevious),
            ),
            PopoverItem::new(
                month_name(month.month()),
                &month.year().to_string(),
                false,
                None,
            ),
        ];
        for week in month.cells().chunks(7) {
            let label = week
                .iter()
                .map(|day| day.map_or("  ".to_owned(), |day| format!("{day:>2}")))
                .collect::<Vec<_>>()
                .join(" ");
            rows.push(PopoverItem::new(&label, "", false, None));
        }
        rows.extend([
            PopoverItem::new(
                "Today",
                format!("{} {}", month_name(today.month()), today.day()).as_str(),
                true,
                Some(PopoverAction::CalendarToday),
            ),
            PopoverItem::new("Next month", "", true, Some(PopoverAction::CalendarNext)),
            PopoverItem::new(
                "Date & time settings",
                "",
                true,
                Some(PopoverAction::OpenSystemRoute(SystemRoute::DateTime)),
            ),
        ]);
        match self.weather_access {
            WeatherAccess::Offline => {
                rows.push(PopoverItem::new(
                    "Weather",
                    "Offline by default",
                    false,
                    None,
                ));
                Ok(PopoverLoadState::Offline(rows))
            }
            WeatherAccess::OptIn => {
                let weather = self.weather.forecast()?;
                rows.push(PopoverItem::new(
                    &weather.label,
                    &weather.detail,
                    false,
                    None,
                ));
                Ok(PopoverLoadState::Ready(rows))
            }
        }
    }
}

fn system_rows() -> Vec<PopoverItem> {
    vec![
        PopoverItem::new(
            "Settings",
            "Ctrl+,",
            true,
            Some(PopoverAction::OpenSettings),
        )
        .with_icon_glyph("\u{E713}"),
        PopoverItem::new(
            "Task Manager",
            "",
            true,
            Some(PopoverAction::OpenTaskManager),
        )
        .with_icon_glyph("\u{E9D9}"),
        PopoverItem::new(
            "Lock",
            "Win+L",
            true,
            Some(PopoverAction::ConfirmSession(SessionAction::Lock)),
        )
        .with_icon_glyph("\u{E72E}")
        .with_section_start(),
        PopoverItem::new(
            "Sleep",
            "Requires confirmation",
            true,
            Some(PopoverAction::ConfirmSession(SessionAction::Sleep)),
        )
        .with_icon_glyph("\u{E708}"),
        PopoverItem::new(
            "Sign out",
            "Requires confirmation",
            true,
            Some(PopoverAction::ConfirmSession(SessionAction::SignOut)),
        )
        .with_icon_glyph("\u{E8AC}"),
        PopoverItem::new(
            "Restart",
            "Requires confirmation",
            true,
            Some(PopoverAction::ConfirmSession(SessionAction::Restart)),
        )
        .with_icon_glyph("\u{E777}")
        .with_section_start(),
        PopoverItem::new(
            "Shut down",
            "Requires confirmation",
            true,
            Some(PopoverAction::ConfirmSession(SessionAction::ShutDown)),
        )
        .with_icon_glyph("\u{E7E8}"),
    ]
}

fn network_rows(snapshot: &TopbarSnapshot) -> Vec<PopoverItem> {
    let network = snapshot.network();
    vec![
        PopoverItem::new(
            "Connection",
            network.label(),
            true,
            Some(PopoverAction::OpenSystemRoute(SystemRoute::Network)),
        ),
        PopoverItem::new(
            "Received",
            &format!("{} KiB/s", network.received_kib_s()),
            false,
            None,
        ),
        PopoverItem::new(
            "Sent",
            &format!("{} KiB/s", network.sent_kib_s()),
            false,
            None,
        ),
        PopoverItem::new(
            "Wi-Fi settings",
            "",
            true,
            Some(PopoverAction::OpenSystemRoute(SystemRoute::Wifi)),
        ),
    ]
}

fn audio_rows(snapshot: &TopbarSnapshot) -> Vec<PopoverItem> {
    vec![
        PopoverItem::new(
            "Volume down",
            &format!("{}%", snapshot.volume_percent()),
            true,
            Some(PopoverAction::VolumeDown),
        ),
        PopoverItem::new("Mute / unmute", "", true, Some(PopoverAction::ToggleMute)),
        PopoverItem::new("Volume up", "", true, Some(PopoverAction::VolumeUp)),
        PopoverItem::new(
            "Sound settings",
            "Default output",
            true,
            Some(PopoverAction::OpenSystemRoute(SystemRoute::Sound)),
        ),
        PopoverItem::new(
            "Previous",
            "Media",
            true,
            Some(PopoverAction::MediaPrevious),
        ),
        PopoverItem::new(
            "Play/Pause",
            "Media",
            true,
            Some(PopoverAction::MediaPlayPause),
        ),
        PopoverItem::new("Next", "Media", true, Some(PopoverAction::MediaNext)),
    ]
}

fn power_rows(snapshot: &TopbarSnapshot) -> Vec<PopoverItem> {
    let mut rows = vec![
        PopoverItem::new("Battery", &snapshot.battery().label(), false, None),
        PopoverItem::new(
            "Power settings",
            "",
            true,
            Some(PopoverAction::OpenSystemRoute(SystemRoute::Power)),
        ),
        PopoverItem::new(
            "Sleep",
            "Requires confirmation",
            true,
            Some(PopoverAction::ConfirmSession(SessionAction::Sleep)),
        ),
        PopoverItem::new(
            "Sign out",
            "Requires confirmation",
            true,
            Some(PopoverAction::ConfirmSession(SessionAction::SignOut)),
        ),
        PopoverItem::new(
            "Restart",
            "Requires confirmation",
            true,
            Some(PopoverAction::ConfirmSession(SessionAction::Restart)),
        ),
        PopoverItem::new(
            "Shut down",
            "Requires confirmation",
            true,
            Some(PopoverAction::ConfirmSession(SessionAction::ShutDown)),
        ),
    ];
    rows.shrink_to_fit();
    rows
}

fn control_center_rows() -> Vec<PopoverItem> {
    vec![
        PopoverItem::new(
            "Quick Settings",
            "Win+A",
            true,
            Some(PopoverAction::OpenQuickSettings),
        ),
        PopoverItem::new(
            "Wi-Fi",
            "Windows settings",
            true,
            Some(PopoverAction::OpenSystemRoute(SystemRoute::Wifi)),
        ),
        PopoverItem::new(
            "Bluetooth",
            "Windows settings",
            true,
            Some(PopoverAction::OpenSystemRoute(SystemRoute::Bluetooth)),
        ),
        PopoverItem::new(
            "Do not disturb",
            "Windows settings",
            true,
            Some(PopoverAction::OpenSystemRoute(SystemRoute::Focus)),
        ),
        PopoverItem::new(
            "Display",
            "Windows settings",
            true,
            Some(PopoverAction::OpenSystemRoute(SystemRoute::Display)),
        ),
        PopoverItem::new(
            "Somente tela do PC",
            "Projetar",
            true,
            Some(PopoverAction::SetProjectionMode(ProjectionMode::Internal)),
        ),
        PopoverItem::new(
            "Duplicar",
            "Projetar",
            true,
            Some(PopoverAction::SetProjectionMode(ProjectionMode::Duplicate)),
        ),
        PopoverItem::new(
            "Estender",
            "Projetar",
            true,
            Some(PopoverAction::SetProjectionMode(ProjectionMode::Extend)),
        ),
        PopoverItem::new(
            "Somente segunda tela",
            "Projetar",
            true,
            Some(PopoverAction::SetProjectionMode(ProjectionMode::External)),
        ),
        PopoverItem::new(
            "Sound",
            "Windows settings",
            true,
            Some(PopoverAction::OpenSystemRoute(SystemRoute::Sound)),
        ),
        PopoverItem::new("Empty notifications", "No accounts", false, None),
    ]
}

const fn month_name(month: u8) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "Month",
    }
}
