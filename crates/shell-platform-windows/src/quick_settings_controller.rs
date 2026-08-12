use shell_config::QuickSettingsSettings;
use shell_core::QuickControlKind;
use shell_renderer::{
    DipPoint, DipRect, QuickSettingsAudioOutput, QuickSettingsAudioOutputId,
    QuickSettingsAudioPanel, QuickSettingsAudioSession, QuickSettingsAudioSessionId,
    QuickSettingsChoice, QuickSettingsChoiceId, QuickSettingsDisplay, QuickSettingsEnergy,
    QuickSettingsFocus, QuickSettingsHit, QuickSettingsMediaAction, QuickSettingsMediaArtwork,
    QuickSettingsMediaChoice, QuickSettingsMediaPlayer, QuickSettingsMediaSessionId,
    QuickSettingsPlaybackState, QuickSettingsScene, QuickSettingsSlider, QuickSettingsSound,
    QuickSettingsSubmenu, QuickSettingsTile, layout_quick_settings,
};

use crate::brightness_coordinator::{
    BrightnessApplyError, BrightnessApplyResult, BrightnessCommandCoordinator,
};
use crate::media_session_controller::MediaSessionController;
use crate::media_session_types::{
    MediaPlaybackState, MediaSessionId, MediaSessionSnapshot, MediaTransportAction,
    MediaTransportResult,
};
use crate::night_light_coordinator::{
    NightLightApplyError, NightLightApplyResult, NightLightCommandCoordinator,
};
use crate::{
    AudioOutputId, AudioPanelSnapshot, AudioSessionId, DoNotDisturbMode, QueuedQuickSettingsAction,
    QuickControlAvailability, QuickControlCapability, QuickSettingsCapabilities,
    QuickSettingsIntent, QuickSettingsKey,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum QuickSettingsPage {
    #[default]
    Main,
    DoNotDisturb,
    Projection,
    MediaSessions,
    Audio,
}

pub struct QuickSettingsController {
    preferences: QuickSettingsSettings,
    capabilities: QuickSettingsCapabilities,
    open: bool,
    focused: Option<QuickSettingsFocus>,
    hovered: Option<QuickSettingsHit>,
    pressed: Option<QuickSettingsHit>,
    live_slider: Option<(QuickControlKind, u8)>,
    live_audio_slider: Option<(AudioSessionId, u8)>,
    page: QuickSettingsPage,
    scroll_offset: f32,
    last_error: String,
    media: MediaSessionController,
    night_light: NightLightCommandCoordinator,
    brightness: BrightnessCommandCoordinator,
    audio: AudioPanelSnapshot,
    audio_root: bool,
}

impl QuickSettingsController {
    #[must_use]
    pub fn new(
        preferences: QuickSettingsSettings,
        capabilities: QuickSettingsCapabilities,
    ) -> Self {
        Self {
            preferences,
            capabilities,
            open: false,
            focused: None,
            hovered: None,
            pressed: None,
            live_slider: None,
            live_audio_slider: None,
            page: QuickSettingsPage::Main,
            scroll_offset: 0.0,
            last_error: String::new(),
            media: MediaSessionController::default(),
            night_light: NightLightCommandCoordinator::new(false, 0),
            brightness: BrightnessCommandCoordinator::new(0),
            audio: AudioPanelSnapshot::default(),
            audio_root: false,
        }
    }

    pub fn open(&mut self, capabilities: QuickSettingsCapabilities) {
        self.capabilities = capabilities;
        self.sync_night_light_confirmed();
        self.sync_brightness_confirmed();
        self.open = true;
        self.page = QuickSettingsPage::Main;
        self.audio_root = false;
        self.focused = self.focus_order().first().copied();
        self.hovered = None;
        self.pressed = None;
        self.live_slider = None;
        self.live_audio_slider = None;
        self.scroll_offset = 0.0;
        self.last_error.clear();
    }

    pub fn dismiss(&mut self) {
        self.open = false;
        self.hovered = None;
        self.pressed = None;
        self.live_slider = None;
        self.live_audio_slider = None;
        self.page = QuickSettingsPage::Main;
        self.audio_root = false;
    }
    pub fn open_audio(
        &mut self,
        capabilities: QuickSettingsCapabilities,
        audio: AudioPanelSnapshot,
    ) {
        self.open(capabilities);
        self.replace_audio_panel(audio);
        self.page = QuickSettingsPage::Audio;
        self.audio_root = true;
        self.focused = Some(QuickSettingsFocus::Control(QuickControlKind::Volume));
    }
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open
    }
    #[must_use]
    pub const fn capabilities(&self) -> &QuickSettingsCapabilities {
        &self.capabilities
    }
    pub fn replace_capabilities(&mut self, value: QuickSettingsCapabilities) {
        self.capabilities = value;
        self.sync_audio_output_label();
        self.sync_night_light_confirmed();
        self.sync_brightness_confirmed();
        if !matches!(self.pressed, Some(QuickSettingsHit::Slider { .. })) {
            self.live_slider = None;
        }
        self.focused = self.focus_order().first().copied();
    }
    pub fn replace_audio_panel(&mut self, value: AudioPanelSnapshot) {
        self.audio = value;
        self.sync_audio_output_label();
        self.live_audio_slider = None;
    }

    fn sync_audio_output_label(&mut self) {
        let Some(output) = self.audio.outputs().iter().find(|output| output.selected()) else {
            return;
        };
        let Some(volume) = self.capabilities.get(QuickControlKind::Volume).cloned() else {
            return;
        };
        self.capabilities.replace(QuickControlCapability::new(
            QuickControlKind::Volume,
            volume.availability(),
            volume.label(),
            output.label(),
            volume.value(),
        ));
    }
    pub fn apply_capability_update(&mut self, value: QuickControlCapability) {
        let kind = value.kind();
        self.capabilities.replace(value);
        if kind == QuickControlKind::NightLight {
            self.sync_night_light_confirmed();
        }
        if kind == QuickControlKind::Brightness {
            self.sync_brightness_confirmed();
        }
        if !matches!(self.pressed, Some(QuickSettingsHit::Slider { kind: pressed, .. }) if pressed == kind)
        {
            self.live_slider = None;
        }
        self.last_error.clear();
    }
    pub fn set_preferences(&mut self, value: QuickSettingsSettings) {
        self.preferences = value.normalized();
    }
    pub fn set_error(&mut self, value: &str) {
        self.last_error = value.to_owned();
    }

    pub(crate) fn apply_media_snapshot(&mut self, value: MediaSessionSnapshot) -> bool {
        self.media.apply_snapshot(value)
    }

    pub(crate) fn complete_media_transport(&mut self, value: MediaTransportResult) -> bool {
        self.media.complete_transport(value)
    }

    pub(crate) fn complete_night_light(
        &mut self,
        request: u64,
        result: Result<NightLightApplyResult, NightLightApplyError>,
    ) -> Vec<QueuedQuickSettingsAction> {
        let applied = result.as_ref().ok().copied();
        let follow_up = self.night_light.complete(request, result);
        if let Some(applied) = applied
            && let Some(previous) = self.capabilities.get(QuickControlKind::NightLight).cloned()
        {
            self.capabilities.replace(QuickControlCapability::new(
                QuickControlKind::NightLight,
                QuickControlAvailability::Available {
                    active: applied.active,
                },
                previous.label(),
                self.night_light.detail(),
                previous.value(),
            ));
        }
        let mut actions = vec![QueuedQuickSettingsAction::Redraw];
        if let Some(request) = follow_up {
            actions.push(QueuedQuickSettingsAction::NightLight(request));
        }
        actions
    }

    pub(crate) fn complete_brightness(
        &mut self,
        request: u64,
        result: Result<BrightnessApplyResult, BrightnessApplyError>,
    ) -> Vec<QueuedQuickSettingsAction> {
        let applied = result.as_ref().ok().copied();
        let follow_up = self.brightness.complete(request, result);
        if let Some(applied) = applied
            && let Some(previous) = self.capabilities.get(QuickControlKind::Brightness).cloned()
        {
            self.capabilities.replace(QuickControlCapability::new(
                QuickControlKind::Brightness,
                QuickControlAvailability::Available { active: false },
                previous.label(),
                self.brightness.detail(),
                Some(applied.value),
            ));
        }
        self.live_slider = follow_up.map(|_| {
            (
                QuickControlKind::Brightness,
                self.brightness.visible_value(),
            )
        });
        let mut actions = vec![QueuedQuickSettingsAction::Redraw];
        if let Some(request) = follow_up {
            actions.push(QueuedQuickSettingsAction::Brightness(request));
        }
        actions
    }

    #[must_use]
    pub fn scene(&self) -> QuickSettingsScene {
        if self.page == QuickSettingsPage::DoNotDisturb {
            return self.do_not_disturb_scene();
        }
        if self.page == QuickSettingsPage::Projection {
            return self.projection_scene();
        }
        if self.page == QuickSettingsPage::MediaSessions {
            return self.media_sessions_scene();
        }
        if self.page == QuickSettingsPage::Audio {
            return self.audio_scene();
        }
        let ordered = self.visible_capabilities();
        let tiles = ordered
            .iter()
            .filter(|capability| is_grid_kind(capability.kind()))
            .map(|capability| tile_for(capability))
            .collect();
        let brightness = ordered
            .iter()
            .find(|capability| capability.kind() == QuickControlKind::Brightness)
            .and_then(|capability| slider_for(capability, self.live_slider));
        let display_actions = ordered
            .iter()
            .filter(|capability| {
                matches!(
                    capability.kind(),
                    QuickControlKind::DarkMode | QuickControlKind::NightLight
                )
            })
            .map(|capability| {
                if capability.kind() == QuickControlKind::NightLight {
                    self.night_light_tile(capability)
                } else {
                    tile_for(capability)
                }
            })
            .collect::<Vec<_>>();
        let display = (brightness.is_some() || !display_actions.is_empty())
            .then(|| QuickSettingsDisplay::new(brightness, display_actions));
        let sound = ordered
            .iter()
            .find(|capability| capability.kind() == QuickControlKind::Volume)
            .and_then(|capability| {
                slider_for(capability, self.live_slider).map(|slider| {
                    QuickSettingsSound::new(
                        slider,
                        capability.detail(),
                        capability_active(capability),
                    )
                })
            });
        let battery = ordered
            .iter()
            .find(|capability| capability.kind() == QuickControlKind::Battery);
        let saver = ordered
            .iter()
            .find(|capability| capability.kind() == QuickControlKind::EnergySaver)
            .map(|capability| tile_for(capability));
        let energy = battery.map(|battery| {
            QuickSettingsEnergy::new(
                battery.value().unwrap_or(0),
                battery.detail().contains("Charging"),
                saver,
            )
        });
        QuickSettingsScene::new(tiles)
            .with_media(self.media_player())
            .with_display(display)
            .with_sound(sound)
            .with_energy(energy)
            .with_focus(self.focused)
            .with_hover(self.hovered)
            .with_pressed(self.pressed)
            .with_scroll_offset(self.scroll_offset)
            .with_status_text(self.status_text())
    }

    fn audio_scene(&self) -> QuickSettingsScene {
        let master = self
            .capabilities
            .get(QuickControlKind::Volume)
            .and_then(|capability| slider_for(capability, self.live_slider))
            .unwrap_or_else(|| {
                QuickSettingsSlider::new(QuickControlKind::Volume, "Volume", 0, false)
            });
        let sessions = self
            .audio
            .sessions()
            .iter()
            .take(3)
            .map(|session| {
                let volume = self
                    .live_audio_slider
                    .filter(|(id, _)| *id == session.id())
                    .map_or(session.volume(), |(_, value)| value);
                QuickSettingsAudioSession::new(
                    QuickSettingsAudioSessionId::new(session.id().value()),
                    session.label(),
                    session.detail(),
                    volume,
                    session.muted(),
                )
                .with_icon_source(session.icon_source())
            })
            .collect();
        let outputs = self
            .audio
            .outputs()
            .iter()
            .map(|output| {
                QuickSettingsAudioOutput::new(
                    QuickSettingsAudioOutputId::new(output.id().value()),
                    output.label(),
                    output.detail(),
                    output.selected(),
                )
            })
            .collect();
        QuickSettingsScene::new(Vec::new())
            .with_audio_panel(Some(
                QuickSettingsAudioPanel::new(master, sessions, outputs, self.audio.spatial_audio())
                    .with_standalone(self.audio_root),
            ))
            .with_focus(self.focused)
            .with_hover(self.hovered)
            .with_pressed(self.pressed)
            .with_status_text(self.status_text())
    }

    fn do_not_disturb_scene(&self) -> QuickSettingsScene {
        let selected = self
            .capabilities
            .get(QuickControlKind::Focus)
            .and_then(QuickControlCapability::value)
            .unwrap_or(0);
        let choices = [
            (
                QuickSettingsChoiceId::DoNotDisturbOff,
                "Off",
                "Allow all notifications",
                "\u{E7ED}",
                0,
            ),
            (
                QuickSettingsChoiceId::DoNotDisturbPriorityOnly,
                "Priority only",
                "Allow priority notifications",
                "\u{EA8F}",
                1,
            ),
            (
                QuickSettingsChoiceId::DoNotDisturbAlarmsOnly,
                "Alarms only",
                "Silence everything except alarms",
                "\u{E823}",
                2,
            ),
        ]
        .into_iter()
        .map(|(id, label, detail, glyph, value)| {
            QuickSettingsChoice::new(id, label, detail, glyph, selected == value, true)
        })
        .collect();
        QuickSettingsScene::new(Vec::new())
            .with_submenu(Some(QuickSettingsSubmenu::new(
                "Do not disturb",
                "Choose which notifications can interrupt you",
                choices,
            )))
            .with_focus(self.focused)
            .with_hover(self.hovered)
            .with_pressed(self.pressed)
            .with_status_text(&self.last_error)
    }

    fn projection_scene(&self) -> QuickSettingsScene {
        let selected = self
            .capabilities
            .get(QuickControlKind::Projection)
            .and_then(QuickControlCapability::value)
            .unwrap_or(0);
        let choices = [
            (
                QuickSettingsChoiceId::ProjectionInternal,
                "PC screen only",
                "Use only this display",
                "\u{E7F4}",
                0,
            ),
            (
                QuickSettingsChoiceId::ProjectionDuplicate,
                "Duplicate",
                "Show the same content on both",
                "\u{E8B9}",
                1,
            ),
            (
                QuickSettingsChoiceId::ProjectionExtend,
                "Extend",
                "Use both as one workspace",
                "\u{E8A7}",
                2,
            ),
            (
                QuickSettingsChoiceId::ProjectionExternal,
                "Second screen only",
                "Turn off this display",
                "\u{E7F4}",
                3,
            ),
        ]
        .into_iter()
        .map(|(id, label, detail, glyph, value)| {
            QuickSettingsChoice::new(id, label, detail, glyph, selected == value, true)
        })
        .collect();
        QuickSettingsScene::new(Vec::new())
            .with_submenu(Some(QuickSettingsSubmenu::new(
                "Projection",
                "Choose how to use your displays",
                choices,
            )))
            .with_focus(self.focused)
            .with_hover(self.hovered)
            .with_pressed(self.pressed)
            .with_status_text(&self.last_error)
    }

    fn media_sessions_scene(&self) -> QuickSettingsScene {
        let selected = self.media.selected();
        let choices = self
            .media
            .sessions()
            .iter()
            .map(|entry| {
                QuickSettingsMediaChoice::new(
                    QuickSettingsMediaSessionId::new(entry.id.value()),
                    &entry.source,
                    &entry.title,
                    &entry.artist,
                    entry.artwork.as_ref().map(|artwork| {
                        QuickSettingsMediaArtwork::new(artwork.generation, artwork.encoded.clone())
                    }),
                    renderer_playback(entry.playback),
                    selected == Some(entry.id),
                )
            })
            .collect();
        QuickSettingsScene::new(Vec::new())
            .with_media_choices(Some(choices))
            .with_focus(self.focused)
            .with_hover(self.hovered)
            .with_pressed(self.pressed)
            .with_status_text(self.status_text())
    }

    fn media_player(&self) -> Option<QuickSettingsMediaPlayer> {
        Some(self.media.selected_entry().map_or_else(
            || {
                QuickSettingsMediaPlayer::new(
                    QuickSettingsMediaSessionId::new(0),
                    "Media",
                    "Nothing playing",
                    "Start audio in any app",
                    None,
                    QuickSettingsPlaybackState::Stopped,
                    false,
                    false,
                    false,
                    None,
                )
            },
            |entry| {
                QuickSettingsMediaPlayer::new(
                    QuickSettingsMediaSessionId::new(entry.id.value()),
                    &entry.source,
                    &entry.title,
                    &entry.artist,
                    entry.artwork.as_ref().map(|artwork| {
                        QuickSettingsMediaArtwork::new(artwork.generation, artwork.encoded.clone())
                    }),
                    renderer_playback(entry.playback),
                    entry.commands.previous,
                    entry.commands.toggle,
                    entry.commands.next,
                    self.media.pending_action().map(renderer_action),
                )
            },
        ))
    }

    fn status_text(&self) -> &str {
        if self.last_error.is_empty() {
            self.media.last_error()
        } else {
            &self.last_error
        }
    }

    fn night_light_tile(&self, capability: &QuickControlCapability) -> QuickSettingsTile {
        let (detail, active) = match capability.availability() {
            QuickControlAvailability::RouteOnly { active } => {
                (capability.detail(), active.unwrap_or(false))
            }
            _ => (self.night_light.detail(), self.night_light.confirmed()),
        };
        QuickSettingsTile::new(
            QuickControlKind::NightLight,
            capability.label(),
            detail,
            glyph_for(QuickControlKind::NightLight),
            active,
            capability_enabled(capability),
        )
    }

    fn sync_night_light_confirmed(&mut self) {
        if let Some(capability) = self.capabilities.get(QuickControlKind::NightLight) {
            self.night_light
                .sync_confirmed(capability_active(capability));
        }
    }

    fn sync_brightness_confirmed(&mut self) {
        if let Some(value) = self
            .capabilities
            .get(QuickControlKind::Brightness)
            .and_then(QuickControlCapability::value)
        {
            self.brightness.sync_confirmed(value);
        }
    }

    pub fn handle_key(&mut self, key: QuickSettingsKey) -> Vec<QueuedQuickSettingsAction> {
        match key {
            QuickSettingsKey::Next => self.move_focus(1),
            QuickSettingsKey::Previous => self.move_focus(-1),
            QuickSettingsKey::Activate => match self.focused {
                Some(QuickSettingsFocus::Control(QuickControlKind::Focus)) => {
                    self.open_do_not_disturb_page()
                }
                Some(QuickSettingsFocus::Control(QuickControlKind::NightLight)) => {
                    self.activate_night_light()
                }
                Some(QuickSettingsFocus::Control(QuickControlKind::Projection)) => {
                    self.open_projection_page()
                }
                Some(QuickSettingsFocus::SoundDetails) => self.open_audio_page(),
                Some(QuickSettingsFocus::Back) => self.close_page(),
                Some(QuickSettingsFocus::MediaArtwork) => self.open_media_sessions_page(),
                Some(QuickSettingsFocus::MediaAction(action)) => self.activate_media(action),
                Some(QuickSettingsFocus::MediaSession(id)) => self.select_media(id),
                Some(focus) => vec![QueuedQuickSettingsAction::Intent(intent_for_focus(focus))],
                None => Vec::new(),
            },
            QuickSettingsKey::Increase | QuickSettingsKey::Decrease => {
                let Some(QuickSettingsFocus::Control(kind)) = self.focused else {
                    return Vec::new();
                };
                let Some(capability) = self.capabilities.get(kind) else {
                    return Vec::new();
                };
                let Some(value) = capability.value() else {
                    return Vec::new();
                };
                let delta = if key == QuickSettingsKey::Increase {
                    5
                } else {
                    -5
                };
                let value = value.saturating_add_signed(delta).min(100);
                self.live_slider = Some((kind, value));
                self.slider_change_actions(kind, value)
            }
            QuickSettingsKey::Escape
                if self.page == QuickSettingsPage::Audio && self.audio_root =>
            {
                vec![QueuedQuickSettingsAction::Intent(
                    QuickSettingsIntent::Dismiss,
                )]
            }
            QuickSettingsKey::Escape if self.page != QuickSettingsPage::Main => self.close_page(),
            QuickSettingsKey::Escape => vec![QueuedQuickSettingsAction::Intent(
                QuickSettingsIntent::Dismiss,
            )],
        }
    }

    pub fn pointer_move(
        &mut self,
        point: DipPoint,
        surface: DipRect,
    ) -> Vec<QueuedQuickSettingsAction> {
        if point.x == -1.0 && point.y == -1.0 && self.pressed.is_some() {
            return Vec::new();
        }
        let layout = layout_quick_settings(&self.scene(), surface);
        if let Some(QuickSettingsHit::Slider { kind, .. }) = self.pressed {
            let Some(value) = layout.slider_value_at(kind, point.x) else {
                return Vec::new();
            };
            let changed = self.live_slider != Some((kind, value));
            self.live_slider = Some((kind, value));
            self.hovered = Some(QuickSettingsHit::Slider { kind, value });
            self.focused = Some(QuickSettingsFocus::Control(kind));
            if !changed {
                return Vec::new();
            }
            return self.slider_change_actions(kind, value);
        }
        if let Some(QuickSettingsHit::AudioSessionSlider { id, .. }) = self.pressed {
            let Some(value) = layout.audio_session_value_at(id, point.x) else {
                return Vec::new();
            };
            let platform_id = AudioSessionId::new(id.value());
            let changed = self.live_audio_slider != Some((platform_id, value));
            self.live_audio_slider = Some((platform_id, value));
            self.hovered = Some(QuickSettingsHit::AudioSessionSlider { id, value });
            self.focused = Some(QuickSettingsFocus::AudioSession(id));
            if !changed {
                return Vec::new();
            }
            return vec![
                QueuedQuickSettingsAction::Redraw,
                QueuedQuickSettingsAction::Intent(QuickSettingsIntent::SetAudioSessionVolume {
                    id: platform_id,
                    value,
                }),
            ];
        }
        let hit = layout.hit_test(point);
        if self.hovered == hit {
            return Vec::new();
        }
        self.hovered = hit;
        if let Some(hit) = hit {
            self.focused = Some(focus_for_hit(hit));
        }
        vec![QueuedQuickSettingsAction::Redraw]
    }

    pub fn pointer_press(
        &mut self,
        point: DipPoint,
        surface: DipRect,
    ) -> Vec<QueuedQuickSettingsAction> {
        self.pressed = layout_quick_settings(&self.scene(), surface).hit_test(point);
        match self.pressed {
            Some(QuickSettingsHit::Slider { kind, value }) => {
                self.live_slider = Some((kind, value));
                self.slider_change_actions(kind, value)
            }
            Some(QuickSettingsHit::AudioSessionSlider { id, value }) => {
                let platform_id = AudioSessionId::new(id.value());
                self.live_audio_slider = Some((platform_id, value));
                vec![
                    QueuedQuickSettingsAction::Redraw,
                    QueuedQuickSettingsAction::Intent(QuickSettingsIntent::SetAudioSessionVolume {
                        id: platform_id,
                        value,
                    }),
                ]
            }
            Some(_) => vec![QueuedQuickSettingsAction::Redraw],
            None => Vec::new(),
        }
    }

    pub fn pointer_release(
        &mut self,
        point: DipPoint,
        surface: DipRect,
    ) -> Vec<QueuedQuickSettingsAction> {
        let layout = layout_quick_settings(&self.scene(), surface);
        let pressed = self.pressed.take();
        let mut actions = vec![QueuedQuickSettingsAction::Redraw];
        if let Some(QuickSettingsHit::Slider { kind, .. }) = pressed {
            if let Some(value) = layout.slider_value_at(kind, point.x) {
                self.live_slider = Some((kind, value));
                actions.extend(self.slider_change_actions(kind, value).into_iter().skip(1));
            }
            return actions;
        }
        if let Some(QuickSettingsHit::AudioSessionSlider { id, .. }) = pressed {
            if let Some(value) = layout.audio_session_value_at(id, point.x) {
                let platform_id = AudioSessionId::new(id.value());
                self.live_audio_slider = Some((platform_id, value));
                actions.push(QueuedQuickSettingsAction::Intent(
                    QuickSettingsIntent::SetAudioSessionVolume {
                        id: platform_id,
                        value,
                    },
                ));
            }
            return actions;
        }
        let released = layout.hit_test(point);
        if pressed.is_some()
            && pressed.map(hit_identity) == released.map(hit_identity)
            && let Some(hit) = released
        {
            if hit == QuickSettingsHit::Tile(QuickControlKind::Focus) {
                return self.open_do_not_disturb_page();
            }
            if hit == QuickSettingsHit::Tile(QuickControlKind::Projection) {
                return self.open_projection_page();
            }
            if hit == QuickSettingsHit::Tile(QuickControlKind::NightLight) {
                return self.activate_night_light();
            }
            if hit == QuickSettingsHit::Back {
                return self.close_page();
            }
            if hit == QuickSettingsHit::MediaArtwork {
                return self.open_media_sessions_page();
            }
            if hit == QuickSettingsHit::SoundDetails {
                return self.open_audio_page();
            }
            if let QuickSettingsHit::MediaAction(action) = hit {
                return self.activate_media(action);
            }
            if let QuickSettingsHit::MediaSession(id) = hit {
                return self.select_media(id);
            }
            actions.push(QueuedQuickSettingsAction::Intent(intent_for_hit(hit)));
        }
        actions
    }

    pub fn scroll(&mut self, rows: isize) -> Vec<QueuedQuickSettingsAction> {
        let _ = rows;
        if self.scroll_offset == 0.0 {
            return Vec::new();
        }
        self.scroll_offset = 0.0;
        vec![QueuedQuickSettingsAction::Redraw]
    }

    fn visible_capabilities(&self) -> Vec<&QuickControlCapability> {
        self.preferences
            .controls()
            .iter()
            .filter(|placement| placement.visible())
            .filter(|placement| {
                !matches!(
                    placement.kind(),
                    QuickControlKind::NearbySharing | QuickControlKind::Multitasking
                )
            })
            .filter_map(|placement| self.capabilities.get(placement.kind()))
            .filter(|capability| capability.availability() != QuickControlAvailability::Unsupported)
            .collect()
    }

    fn focus_order(&self) -> Vec<QuickSettingsFocus> {
        if self.page == QuickSettingsPage::DoNotDisturb {
            return std::iter::once(QuickSettingsFocus::Back)
                .chain(do_not_disturb_choice_ids().map(QuickSettingsFocus::Choice))
                .collect();
        }
        if self.page == QuickSettingsPage::Projection {
            return std::iter::once(QuickSettingsFocus::Back)
                .chain(projection_choice_ids().map(QuickSettingsFocus::Choice))
                .collect();
        }
        if self.page == QuickSettingsPage::MediaSessions {
            return std::iter::once(QuickSettingsFocus::Back)
                .chain(self.media.sessions().iter().map(|entry| {
                    QuickSettingsFocus::MediaSession(QuickSettingsMediaSessionId::new(
                        entry.id.value(),
                    ))
                }))
                .collect();
        }
        if self.page == QuickSettingsPage::Audio {
            return (!self.audio_root)
                .then_some(QuickSettingsFocus::Back)
                .into_iter()
                .chain(std::iter::once(QuickSettingsFocus::Control(
                    QuickControlKind::Volume,
                )))
                .chain(self.audio.sessions().iter().take(3).map(|entry| {
                    QuickSettingsFocus::AudioSession(QuickSettingsAudioSessionId::new(
                        entry.id().value(),
                    ))
                }))
                .chain(self.audio.outputs().iter().map(|entry| {
                    QuickSettingsFocus::AudioOutput(QuickSettingsAudioOutputId::new(
                        entry.id().value(),
                    ))
                }))
                .chain(
                    self.audio
                        .spatial_audio()
                        .then_some(QuickSettingsFocus::AudioSpatial),
                )
                .chain(std::iter::once(QuickSettingsFocus::AudioSettings))
                .collect();
        }
        let mut order = self
            .visible_capabilities()
            .into_iter()
            .map(|capability| QuickSettingsFocus::Control(capability.kind()))
            .collect::<Vec<_>>();
        if self.media.selected_entry().is_some() {
            order.splice(
                0..0,
                [
                    QuickSettingsFocus::MediaArtwork,
                    QuickSettingsFocus::MediaAction(QuickSettingsMediaAction::Previous),
                    QuickSettingsFocus::MediaAction(QuickSettingsMediaAction::TogglePlayback),
                    QuickSettingsFocus::MediaAction(QuickSettingsMediaAction::Next),
                ],
            );
        }
        order.push(QuickSettingsFocus::EditControls);
        if self.capabilities.get(QuickControlKind::Volume).is_some() {
            order.push(QuickSettingsFocus::SoundDetails);
        }
        order
    }

    fn move_focus(&mut self, delta: isize) -> Vec<QueuedQuickSettingsAction> {
        let order = self.focus_order();
        if order.is_empty() {
            return Vec::new();
        }
        let current = self
            .focused
            .and_then(|focus| order.iter().position(|candidate| *candidate == focus))
            .unwrap_or(0);
        self.focused = Some(order[current.saturating_add_signed(delta).min(order.len() - 1)]);
        vec![QueuedQuickSettingsAction::Redraw]
    }

    fn open_projection_page(&mut self) -> Vec<QueuedQuickSettingsAction> {
        self.page = QuickSettingsPage::Projection;
        self.focused = Some(QuickSettingsFocus::Back);
        self.hovered = None;
        self.pressed = None;
        self.last_error.clear();
        vec![QueuedQuickSettingsAction::Reflow]
    }

    fn open_do_not_disturb_page(&mut self) -> Vec<QueuedQuickSettingsAction> {
        self.page = QuickSettingsPage::DoNotDisturb;
        self.focused = Some(QuickSettingsFocus::Back);
        self.hovered = None;
        self.pressed = None;
        self.last_error.clear();
        vec![QueuedQuickSettingsAction::Reflow]
    }

    fn close_do_not_disturb_page(&mut self) -> Vec<QueuedQuickSettingsAction> {
        self.page = QuickSettingsPage::Main;
        self.focused = Some(QuickSettingsFocus::Control(QuickControlKind::Focus));
        self.hovered = None;
        self.pressed = None;
        self.last_error.clear();
        vec![QueuedQuickSettingsAction::Reflow]
    }

    fn close_projection_page(&mut self) -> Vec<QueuedQuickSettingsAction> {
        self.page = QuickSettingsPage::Main;
        self.focused = Some(QuickSettingsFocus::Control(QuickControlKind::Projection));
        self.hovered = None;
        self.pressed = None;
        self.last_error.clear();
        vec![QueuedQuickSettingsAction::Reflow]
    }

    fn open_media_sessions_page(&mut self) -> Vec<QueuedQuickSettingsAction> {
        if self.media.sessions().is_empty() {
            return Vec::new();
        }
        self.page = QuickSettingsPage::MediaSessions;
        self.focused = Some(QuickSettingsFocus::Back);
        self.hovered = None;
        self.pressed = None;
        self.last_error.clear();
        vec![QueuedQuickSettingsAction::Reflow]
    }

    fn open_audio_page(&mut self) -> Vec<QueuedQuickSettingsAction> {
        self.page = QuickSettingsPage::Audio;
        self.audio_root = false;
        self.focused = Some(QuickSettingsFocus::Back);
        self.hovered = None;
        self.pressed = None;
        self.live_audio_slider = None;
        self.last_error.clear();
        vec![QueuedQuickSettingsAction::Reflow]
    }

    fn close_page(&mut self) -> Vec<QueuedQuickSettingsAction> {
        match self.page {
            QuickSettingsPage::DoNotDisturb => self.close_do_not_disturb_page(),
            QuickSettingsPage::Projection => self.close_projection_page(),
            QuickSettingsPage::MediaSessions => {
                self.page = QuickSettingsPage::Main;
                self.focused = Some(QuickSettingsFocus::MediaArtwork);
                self.hovered = None;
                self.pressed = None;
                vec![QueuedQuickSettingsAction::Reflow]
            }
            QuickSettingsPage::Audio => {
                if self.audio_root {
                    return vec![QueuedQuickSettingsAction::Intent(
                        QuickSettingsIntent::Dismiss,
                    )];
                }
                self.page = QuickSettingsPage::Main;
                self.focused = Some(QuickSettingsFocus::SoundDetails);
                self.hovered = None;
                self.pressed = None;
                self.live_audio_slider = None;
                vec![QueuedQuickSettingsAction::Reflow]
            }
            QuickSettingsPage::Main => Vec::new(),
        }
    }

    fn activate_media(
        &mut self,
        action: QuickSettingsMediaAction,
    ) -> Vec<QueuedQuickSettingsAction> {
        let Some(command) = self.media.begin_transport(platform_action(action)) else {
            return Vec::new();
        };
        vec![
            QueuedQuickSettingsAction::Redraw,
            QueuedQuickSettingsAction::Media(command),
        ]
    }

    fn select_media(&mut self, id: QuickSettingsMediaSessionId) -> Vec<QueuedQuickSettingsAction> {
        if !self.media.select(MediaSessionId::new(id.value())) {
            return Vec::new();
        }
        self.page = QuickSettingsPage::Main;
        self.focused = Some(QuickSettingsFocus::MediaArtwork);
        self.hovered = None;
        self.pressed = None;
        vec![QueuedQuickSettingsAction::Reflow]
    }

    fn activate_night_light(&mut self) -> Vec<QueuedQuickSettingsAction> {
        if self
            .capabilities
            .get(QuickControlKind::NightLight)
            .is_some_and(|capability| {
                matches!(
                    capability.availability(),
                    QuickControlAvailability::RouteOnly { .. }
                )
            })
        {
            return vec![QueuedQuickSettingsAction::Intent(
                QuickSettingsIntent::Activate(QuickControlKind::NightLight),
            )];
        }
        let mut actions = vec![QueuedQuickSettingsAction::Redraw];
        if let Some(request) = self.night_light.request_toggle() {
            actions.push(QueuedQuickSettingsAction::NightLight(request));
        }
        actions
    }

    fn slider_change_actions(
        &mut self,
        kind: QuickControlKind,
        value: u8,
    ) -> Vec<QueuedQuickSettingsAction> {
        let mut actions = vec![QueuedQuickSettingsAction::Redraw];
        if kind == QuickControlKind::Brightness {
            if let Some(request) = self.brightness.request_value(value) {
                actions.push(QueuedQuickSettingsAction::Brightness(request));
            }
        } else {
            actions.push(QueuedQuickSettingsAction::Intent(
                QuickSettingsIntent::SetValue { kind, value },
            ));
        }
        actions
    }
}

const fn renderer_playback(value: MediaPlaybackState) -> QuickSettingsPlaybackState {
    match value {
        MediaPlaybackState::Playing => QuickSettingsPlaybackState::Playing,
        MediaPlaybackState::Paused => QuickSettingsPlaybackState::Paused,
        MediaPlaybackState::Stopped => QuickSettingsPlaybackState::Stopped,
    }
}

const fn renderer_action(value: MediaTransportAction) -> QuickSettingsMediaAction {
    match value {
        MediaTransportAction::Previous => QuickSettingsMediaAction::Previous,
        MediaTransportAction::TogglePlayback => QuickSettingsMediaAction::TogglePlayback,
        MediaTransportAction::Next => QuickSettingsMediaAction::Next,
    }
}

const fn platform_action(value: QuickSettingsMediaAction) -> MediaTransportAction {
    match value {
        QuickSettingsMediaAction::Previous => MediaTransportAction::Previous,
        QuickSettingsMediaAction::TogglePlayback => MediaTransportAction::TogglePlayback,
        QuickSettingsMediaAction::Next => MediaTransportAction::Next,
    }
}

fn projection_choice_ids() -> impl Iterator<Item = QuickSettingsChoiceId> {
    [
        QuickSettingsChoiceId::ProjectionInternal,
        QuickSettingsChoiceId::ProjectionDuplicate,
        QuickSettingsChoiceId::ProjectionExtend,
        QuickSettingsChoiceId::ProjectionExternal,
    ]
    .into_iter()
}

fn do_not_disturb_choice_ids() -> impl Iterator<Item = QuickSettingsChoiceId> {
    [
        QuickSettingsChoiceId::DoNotDisturbOff,
        QuickSettingsChoiceId::DoNotDisturbPriorityOnly,
        QuickSettingsChoiceId::DoNotDisturbAlarmsOnly,
    ]
    .into_iter()
}

fn is_grid_kind(kind: QuickControlKind) -> bool {
    matches!(
        kind,
        QuickControlKind::Wifi
            | QuickControlKind::Bluetooth
            | QuickControlKind::NearbySharing
            | QuickControlKind::Focus
            | QuickControlKind::Multitasking
            | QuickControlKind::Projection
    )
}
fn capability_active(capability: &QuickControlCapability) -> bool {
    matches!(
        capability.availability(),
        QuickControlAvailability::Available { active: true }
            | QuickControlAvailability::RouteOnly { active: Some(true) }
    )
}
fn capability_enabled(capability: &QuickControlCapability) -> bool {
    !matches!(
        capability.availability(),
        QuickControlAvailability::TemporarilyUnavailable | QuickControlAvailability::Unsupported
    )
}
fn tile_for(capability: &QuickControlCapability) -> QuickSettingsTile {
    QuickSettingsTile::new(
        capability.kind(),
        capability.label(),
        capability.detail(),
        glyph_for(capability.kind()),
        capability_active(capability),
        capability_enabled(capability),
    )
}
fn slider_for(
    capability: &QuickControlCapability,
    live_slider: Option<(QuickControlKind, u8)>,
) -> Option<QuickSettingsSlider> {
    capability.value().map(|value| {
        let value = live_slider
            .filter(|(kind, _)| *kind == capability.kind())
            .map_or(value, |(_, value)| value);
        QuickSettingsSlider::new(
            capability.kind(),
            capability.label(),
            value,
            capability_enabled(capability),
        )
    })
}
fn glyph_for(kind: QuickControlKind) -> &'static str {
    match kind {
        QuickControlKind::Wifi => "\u{E701}",
        QuickControlKind::Bluetooth => "\u{E702}",
        QuickControlKind::NearbySharing => "\u{E8C8}",
        QuickControlKind::Focus => "\u{E7ED}",
        QuickControlKind::Multitasking => "\u{E8A7}",
        QuickControlKind::Projection => "\u{E7F4}",
        QuickControlKind::Brightness => "\u{E706}",
        QuickControlKind::DarkMode => "\u{E793}",
        QuickControlKind::NightLight => "\u{E708}",
        QuickControlKind::Volume => "\u{E767}",
        QuickControlKind::Battery => "\u{E83F}",
        QuickControlKind::EnergySaver => "\u{E8C7}",
    }
}
fn focus_for_hit(hit: QuickSettingsHit) -> QuickSettingsFocus {
    match hit {
        QuickSettingsHit::Tile(kind) | QuickSettingsHit::Slider { kind, .. } => {
            QuickSettingsFocus::Control(kind)
        }
        QuickSettingsHit::MediaArtwork => QuickSettingsFocus::MediaArtwork,
        QuickSettingsHit::MediaAction(action) => QuickSettingsFocus::MediaAction(action),
        QuickSettingsHit::MediaSession(id) => QuickSettingsFocus::MediaSession(id),
        QuickSettingsHit::Back => QuickSettingsFocus::Back,
        QuickSettingsHit::Choice(id) => QuickSettingsFocus::Choice(id),
        QuickSettingsHit::SoundDetails => QuickSettingsFocus::SoundDetails,
        QuickSettingsHit::AudioSessionSlider { id, .. } => QuickSettingsFocus::AudioSession(id),
        QuickSettingsHit::AudioOutput(id) => QuickSettingsFocus::AudioOutput(id),
        QuickSettingsHit::AudioSpatial => QuickSettingsFocus::AudioSpatial,
        QuickSettingsHit::AudioSettings => QuickSettingsFocus::AudioSettings,
        QuickSettingsHit::EditControls => QuickSettingsFocus::EditControls,
    }
}
fn intent_for_focus(focus: QuickSettingsFocus) -> QuickSettingsIntent {
    match focus {
        QuickSettingsFocus::Control(kind) => QuickSettingsIntent::Activate(kind),
        QuickSettingsFocus::MediaArtwork => QuickSettingsIntent::OpenMediaSessions,
        QuickSettingsFocus::MediaAction(action) => QuickSettingsIntent::MediaAction(action),
        QuickSettingsFocus::MediaSession(id) => QuickSettingsIntent::SelectMediaSession(id),
        QuickSettingsFocus::Back => QuickSettingsIntent::Dismiss,
        QuickSettingsFocus::Choice(id) => intent_for_choice(id),
        QuickSettingsFocus::SoundDetails => QuickSettingsIntent::OpenSoundSettings,
        QuickSettingsFocus::AudioSession(id) => QuickSettingsIntent::SetAudioSessionVolume {
            id: AudioSessionId::new(id.value()),
            value: 0,
        },
        QuickSettingsFocus::AudioOutput(id) => {
            QuickSettingsIntent::SelectAudioOutput(AudioOutputId::new(id.value()))
        }
        QuickSettingsFocus::AudioSpatial => QuickSettingsIntent::OpenSoundSettings,
        QuickSettingsFocus::AudioSettings => QuickSettingsIntent::OpenSoundSettings,
        QuickSettingsFocus::EditControls => QuickSettingsIntent::EditControls,
    }
}
fn intent_for_hit(hit: QuickSettingsHit) -> QuickSettingsIntent {
    match hit {
        QuickSettingsHit::Tile(kind) => QuickSettingsIntent::Activate(kind),
        QuickSettingsHit::Slider { kind, value } => QuickSettingsIntent::SetValue { kind, value },
        QuickSettingsHit::MediaArtwork => QuickSettingsIntent::OpenMediaSessions,
        QuickSettingsHit::MediaAction(action) => QuickSettingsIntent::MediaAction(action),
        QuickSettingsHit::MediaSession(id) => QuickSettingsIntent::SelectMediaSession(id),
        QuickSettingsHit::Back => QuickSettingsIntent::Dismiss,
        QuickSettingsHit::Choice(id) => intent_for_choice(id),
        QuickSettingsHit::SoundDetails => QuickSettingsIntent::OpenSoundSettings,
        QuickSettingsHit::AudioSessionSlider { id, value } => {
            QuickSettingsIntent::SetAudioSessionVolume {
                id: AudioSessionId::new(id.value()),
                value,
            }
        }
        QuickSettingsHit::AudioOutput(id) => {
            QuickSettingsIntent::SelectAudioOutput(AudioOutputId::new(id.value()))
        }
        QuickSettingsHit::AudioSpatial => QuickSettingsIntent::OpenSoundSettings,
        QuickSettingsHit::AudioSettings => QuickSettingsIntent::OpenSoundSettings,
        QuickSettingsHit::EditControls => QuickSettingsIntent::EditControls,
    }
}
fn hit_identity(hit: QuickSettingsHit) -> QuickSettingsFocus {
    focus_for_hit(hit)
}

const fn intent_for_choice(id: QuickSettingsChoiceId) -> QuickSettingsIntent {
    match id {
        QuickSettingsChoiceId::DoNotDisturbOff => {
            QuickSettingsIntent::SetDoNotDisturbMode(DoNotDisturbMode::Off)
        }
        QuickSettingsChoiceId::DoNotDisturbPriorityOnly => {
            QuickSettingsIntent::SetDoNotDisturbMode(DoNotDisturbMode::PriorityOnly)
        }
        QuickSettingsChoiceId::DoNotDisturbAlarmsOnly => {
            QuickSettingsIntent::SetDoNotDisturbMode(DoNotDisturbMode::AlarmsOnly)
        }
        QuickSettingsChoiceId::ProjectionInternal => {
            QuickSettingsIntent::SetProjectionMode(crate::ProjectionMode::Internal)
        }
        QuickSettingsChoiceId::ProjectionDuplicate => {
            QuickSettingsIntent::SetProjectionMode(crate::ProjectionMode::Duplicate)
        }
        QuickSettingsChoiceId::ProjectionExtend => {
            QuickSettingsIntent::SetProjectionMode(crate::ProjectionMode::Extend)
        }
        QuickSettingsChoiceId::ProjectionExternal => {
            QuickSettingsIntent::SetProjectionMode(crate::ProjectionMode::External)
        }
    }
}
