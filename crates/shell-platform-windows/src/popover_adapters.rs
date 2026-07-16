#![deny(unsafe_code)]

use shell_core::Popover;

use crate::{
    PopoverAction, PopoverDataError, PopoverDataProvider, PopoverItem, PopoverLoadState,
    PopoverPayload, SessionAction, WeatherAccess, WeatherItem, WeatherProvider,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct OfflineWeatherProvider;

impl WeatherProvider for OfflineWeatherProvider {
    fn forecast(&self) -> Result<WeatherItem, PopoverDataError> {
        Err(PopoverDataError::Adapter("weather provider is offline"))
    }
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
    fn load(&self, kind: Popover) -> Result<PopoverPayload, PopoverDataError> {
        let state = match kind {
            Popover::SystemMenu => PopoverLoadState::Ready(system_rows()),
            Popover::Calendar => self.calendar_rows()?,
            Popover::Network => PopoverLoadState::Ready(network_rows()),
            Popover::Volume => PopoverLoadState::Ready(audio_rows()),
            Popover::Power => PopoverLoadState::Ready(power_rows()),
            Popover::Notifications => PopoverLoadState::Ready(control_center_rows()),
        };
        Ok(PopoverPayload::new(kind, state))
    }
}

impl<W: WeatherProvider> DefaultPopoverDataProvider<W> {
    fn calendar_rows(&self) -> Result<PopoverLoadState, PopoverDataError> {
        let mut rows = vec![
            PopoverItem::new("Today", "Local calendar", false, None),
            PopoverItem::new("Next event", "No events", false, None),
        ];
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
        ),
        PopoverItem::new("Control Center", "", true, None),
        PopoverItem::new(
            "Lock",
            "Win+L",
            true,
            Some(PopoverAction::ConfirmSession(SessionAction::Lock)),
        ),
    ]
}

fn network_rows() -> Vec<PopoverItem> {
    vec![
        PopoverItem::new(
            "Connection",
            "Online details",
            true,
            Some(PopoverAction::NetworkDetails),
        ),
        PopoverItem::new("Received", "0 KiB/s", false, None),
        PopoverItem::new("Sent", "0 KiB/s", false, None),
    ]
}

fn audio_rows() -> Vec<PopoverItem> {
    vec![
        PopoverItem::new("Volume", "42%", true, Some(PopoverAction::SetVolume(42))),
        PopoverItem::new(
            "Output",
            "Default device",
            true,
            Some(PopoverAction::SelectAudioDevice("default".to_owned())),
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

fn power_rows() -> Vec<PopoverItem> {
    vec![
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
    ]
}

fn control_center_rows() -> Vec<PopoverItem> {
    vec![
        PopoverItem::new(
            "Do not disturb",
            "Toggle intent",
            true,
            Some(PopoverAction::ControlCenterToggle("dnd")),
        ),
        PopoverItem::new(
            "Bluetooth",
            "Toggle intent",
            true,
            Some(PopoverAction::ControlCenterToggle("bluetooth")),
        ),
        PopoverItem::new("Empty notifications", "No accounts", false, None),
    ]
}
