use shell_core::QuickControlKind;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub struct QuickSettingsTile {
    kind: QuickControlKind,
    label: String,
    detail: String,
    glyph: String,
    active: bool,
    enabled: bool,
}

impl QuickSettingsTile {
    #[must_use]
    pub fn new(
        kind: QuickControlKind,
        label: &str,
        detail: &str,
        glyph: &str,
        active: bool,
        enabled: bool,
    ) -> Self {
        Self {
            kind,
            label: label.to_owned(),
            detail: detail.to_owned(),
            glyph: glyph.to_owned(),
            active,
            enabled,
        }
    }

    #[must_use]
    pub const fn kind(&self) -> QuickControlKind {
        self.kind
    }
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
    #[must_use]
    pub fn glyph(&self) -> &str {
        &self.glyph
    }
    #[must_use]
    pub const fn active(&self) -> bool {
        self.active
    }
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuickSettingsSlider {
    kind: QuickControlKind,
    label: String,
    value: u8,
    enabled: bool,
}

impl QuickSettingsSlider {
    #[must_use]
    pub fn new(kind: QuickControlKind, label: &str, value: u8, enabled: bool) -> Self {
        Self {
            kind,
            label: label.to_owned(),
            value: value.min(100),
            enabled,
        }
    }
    #[must_use]
    pub const fn kind(&self) -> QuickControlKind {
        self.kind
    }
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }
    #[must_use]
    pub const fn value(&self) -> u8 {
        self.value
    }
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuickSettingsDisplay {
    brightness: Option<QuickSettingsSlider>,
    actions: Vec<QuickSettingsTile>,
}

impl QuickSettingsDisplay {
    #[must_use]
    pub fn new(brightness: Option<QuickSettingsSlider>, actions: Vec<QuickSettingsTile>) -> Self {
        Self {
            brightness,
            actions,
        }
    }
    #[must_use]
    pub const fn brightness(&self) -> Option<&QuickSettingsSlider> {
        self.brightness.as_ref()
    }
    #[must_use]
    pub fn actions(&self) -> &[QuickSettingsTile] {
        &self.actions
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuickSettingsSound {
    volume: QuickSettingsSlider,
    output: String,
    muted: bool,
}

impl QuickSettingsSound {
    #[must_use]
    pub fn new(volume: QuickSettingsSlider, output: &str, muted: bool) -> Self {
        Self {
            volume,
            output: output.to_owned(),
            muted,
        }
    }
    #[must_use]
    pub const fn volume(&self) -> &QuickSettingsSlider {
        &self.volume
    }
    #[must_use]
    pub fn output(&self) -> &str {
        &self.output
    }
    #[must_use]
    pub const fn muted(&self) -> bool {
        self.muted
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct QuickSettingsAudioSessionId(u64);

impl QuickSettingsAudioSessionId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct QuickSettingsAudioOutputId(u64);

impl QuickSettingsAudioOutputId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuickSettingsAudioSession {
    id: QuickSettingsAudioSessionId,
    label: String,
    detail: String,
    volume: u8,
    muted: bool,
    icon_source: Option<String>,
}

impl QuickSettingsAudioSession {
    #[must_use]
    pub fn new(
        id: QuickSettingsAudioSessionId,
        label: &str,
        detail: &str,
        volume: u8,
        muted: bool,
    ) -> Self {
        Self {
            id,
            label: label.to_owned(),
            detail: detail.to_owned(),
            volume: volume.min(100),
            muted,
            icon_source: None,
        }
    }
    #[must_use]
    pub fn with_icon_source(mut self, value: Option<&str>) -> Self {
        self.icon_source = value.map(str::to_owned);
        self
    }
    #[must_use]
    pub const fn id(&self) -> QuickSettingsAudioSessionId {
        self.id
    }
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
    #[must_use]
    pub const fn volume(&self) -> u8 {
        self.volume
    }
    #[must_use]
    pub const fn muted(&self) -> bool {
        self.muted
    }
    #[must_use]
    pub fn icon_source(&self) -> Option<&str> {
        self.icon_source.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuickSettingsAudioOutput {
    id: QuickSettingsAudioOutputId,
    label: String,
    detail: String,
    selected: bool,
}

impl QuickSettingsAudioOutput {
    #[must_use]
    pub fn new(id: QuickSettingsAudioOutputId, label: &str, detail: &str, selected: bool) -> Self {
        Self {
            id,
            label: label.to_owned(),
            detail: detail.to_owned(),
            selected,
        }
    }
    #[must_use]
    pub const fn id(&self) -> QuickSettingsAudioOutputId {
        self.id
    }
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
    #[must_use]
    pub const fn selected(&self) -> bool {
        self.selected
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuickSettingsAudioPanel {
    master: QuickSettingsSlider,
    sessions: Vec<QuickSettingsAudioSession>,
    outputs: Vec<QuickSettingsAudioOutput>,
    spatial_audio: bool,
    standalone: bool,
}

impl QuickSettingsAudioPanel {
    #[must_use]
    pub fn new(
        master: QuickSettingsSlider,
        sessions: Vec<QuickSettingsAudioSession>,
        outputs: Vec<QuickSettingsAudioOutput>,
        spatial_audio: bool,
    ) -> Self {
        Self {
            master,
            sessions,
            outputs,
            spatial_audio,
            standalone: false,
        }
    }
    #[must_use]
    pub const fn with_standalone(mut self, value: bool) -> Self {
        self.standalone = value;
        self
    }
    #[must_use]
    pub const fn master(&self) -> &QuickSettingsSlider {
        &self.master
    }
    #[must_use]
    pub fn sessions(&self) -> &[QuickSettingsAudioSession] {
        &self.sessions
    }
    #[must_use]
    pub fn outputs(&self) -> &[QuickSettingsAudioOutput] {
        &self.outputs
    }
    #[must_use]
    pub const fn spatial_audio(&self) -> bool {
        self.spatial_audio
    }
    #[must_use]
    pub const fn standalone(&self) -> bool {
        self.standalone
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuickSettingsEnergy {
    percent: u8,
    charging: bool,
    saver: Option<QuickSettingsTile>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct QuickSettingsMediaSessionId(u64);

impl QuickSettingsMediaSessionId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuickSettingsPlaybackState {
    Playing,
    Paused,
    Stopped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuickSettingsMediaAction {
    Previous,
    TogglePlayback,
    Next,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuickSettingsMediaArtwork {
    generation: u64,
    encoded: Arc<[u8]>,
}

impl QuickSettingsMediaArtwork {
    #[must_use]
    pub fn new(generation: u64, encoded: Arc<[u8]>) -> Self {
        Self {
            generation,
            encoded,
        }
    }

    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub fn encoded(&self) -> &[u8] {
        &self.encoded
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuickSettingsMediaPlayer {
    session_id: QuickSettingsMediaSessionId,
    source: String,
    title: String,
    artist: String,
    artwork: Option<QuickSettingsMediaArtwork>,
    playback: QuickSettingsPlaybackState,
    can_previous: bool,
    can_toggle: bool,
    can_next: bool,
    pending: Option<QuickSettingsMediaAction>,
}

impl QuickSettingsMediaPlayer {
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "the media transport contract is intentionally explicit"
    )]
    pub fn new(
        session_id: QuickSettingsMediaSessionId,
        source: &str,
        title: &str,
        artist: &str,
        artwork: Option<QuickSettingsMediaArtwork>,
        playback: QuickSettingsPlaybackState,
        can_previous: bool,
        can_toggle: bool,
        can_next: bool,
        pending: Option<QuickSettingsMediaAction>,
    ) -> Self {
        Self {
            session_id,
            source: source.to_owned(),
            title: title.to_owned(),
            artist: artist.to_owned(),
            artwork,
            playback,
            can_previous,
            can_toggle,
            can_next,
            pending,
        }
    }

    #[must_use]
    pub const fn session_id(&self) -> QuickSettingsMediaSessionId {
        self.session_id
    }
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }
    #[must_use]
    pub fn artist(&self) -> &str {
        &self.artist
    }
    #[must_use]
    pub const fn artwork(&self) -> Option<&QuickSettingsMediaArtwork> {
        self.artwork.as_ref()
    }
    #[must_use]
    pub const fn playback(&self) -> QuickSettingsPlaybackState {
        self.playback
    }
    #[must_use]
    pub const fn can_previous(&self) -> bool {
        self.can_previous
    }
    #[must_use]
    pub const fn can_toggle(&self) -> bool {
        self.can_toggle
    }
    #[must_use]
    pub const fn can_next(&self) -> bool {
        self.can_next
    }
    #[must_use]
    pub const fn pending(&self) -> Option<QuickSettingsMediaAction> {
        self.pending
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuickSettingsMediaChoice {
    session_id: QuickSettingsMediaSessionId,
    source: String,
    title: String,
    artist: String,
    artwork: Option<QuickSettingsMediaArtwork>,
    playback: QuickSettingsPlaybackState,
    selected: bool,
}

impl QuickSettingsMediaChoice {
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "the media choice mirrors one session snapshot"
    )]
    pub fn new(
        session_id: QuickSettingsMediaSessionId,
        source: &str,
        title: &str,
        artist: &str,
        artwork: Option<QuickSettingsMediaArtwork>,
        playback: QuickSettingsPlaybackState,
        selected: bool,
    ) -> Self {
        Self {
            session_id,
            source: source.to_owned(),
            title: title.to_owned(),
            artist: artist.to_owned(),
            artwork,
            playback,
            selected,
        }
    }

    #[must_use]
    pub const fn session_id(&self) -> QuickSettingsMediaSessionId {
        self.session_id
    }
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }
    #[must_use]
    pub fn artist(&self) -> &str {
        &self.artist
    }
    #[must_use]
    pub const fn artwork(&self) -> Option<&QuickSettingsMediaArtwork> {
        self.artwork.as_ref()
    }
    #[must_use]
    pub const fn playback(&self) -> QuickSettingsPlaybackState {
        self.playback
    }
    #[must_use]
    pub const fn selected(&self) -> bool {
        self.selected
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuickSettingsChoiceId {
    DoNotDisturbOff,
    DoNotDisturbPriorityOnly,
    DoNotDisturbAlarmsOnly,
    ProjectionInternal,
    ProjectionDuplicate,
    ProjectionExtend,
    ProjectionExternal,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuickSettingsChoice {
    id: QuickSettingsChoiceId,
    label: String,
    detail: String,
    glyph: String,
    selected: bool,
    enabled: bool,
}

impl QuickSettingsChoice {
    #[must_use]
    pub fn new(
        id: QuickSettingsChoiceId,
        label: &str,
        detail: &str,
        glyph: &str,
        selected: bool,
        enabled: bool,
    ) -> Self {
        Self {
            id,
            label: label.to_owned(),
            detail: detail.to_owned(),
            glyph: glyph.to_owned(),
            selected,
            enabled,
        }
    }
    #[must_use]
    pub const fn id(&self) -> QuickSettingsChoiceId {
        self.id
    }
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
    #[must_use]
    pub fn glyph(&self) -> &str {
        &self.glyph
    }
    #[must_use]
    pub const fn selected(&self) -> bool {
        self.selected
    }
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuickSettingsSubmenu {
    title: String,
    detail: String,
    choices: Vec<QuickSettingsChoice>,
}

impl QuickSettingsSubmenu {
    #[must_use]
    pub fn new(title: &str, detail: &str, choices: Vec<QuickSettingsChoice>) -> Self {
        Self {
            title: title.to_owned(),
            detail: detail.to_owned(),
            choices,
        }
    }
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
    #[must_use]
    pub fn choices(&self) -> &[QuickSettingsChoice] {
        &self.choices
    }
}

impl QuickSettingsEnergy {
    #[must_use]
    pub fn new(percent: u8, charging: bool, saver: Option<QuickSettingsTile>) -> Self {
        Self {
            percent: percent.min(100),
            charging,
            saver,
        }
    }
    #[must_use]
    pub const fn percent(&self) -> u8 {
        self.percent
    }
    #[must_use]
    pub const fn charging(&self) -> bool {
        self.charging
    }
    #[must_use]
    pub const fn saver(&self) -> Option<&QuickSettingsTile> {
        self.saver.as_ref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuickSettingsFocus {
    Control(QuickControlKind),
    MediaArtwork,
    MediaAction(QuickSettingsMediaAction),
    MediaSession(QuickSettingsMediaSessionId),
    Back,
    Choice(QuickSettingsChoiceId),
    SoundDetails,
    AudioSession(QuickSettingsAudioSessionId),
    AudioOutput(QuickSettingsAudioOutputId),
    AudioSpatial,
    AudioSettings,
    EditControls,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuickSettingsHit {
    Tile(QuickControlKind),
    Slider {
        kind: QuickControlKind,
        value: u8,
    },
    MediaArtwork,
    MediaAction(QuickSettingsMediaAction),
    MediaSession(QuickSettingsMediaSessionId),
    Back,
    Choice(QuickSettingsChoiceId),
    SoundDetails,
    AudioSessionSlider {
        id: QuickSettingsAudioSessionId,
        value: u8,
    },
    AudioOutput(QuickSettingsAudioOutputId),
    AudioSpatial,
    AudioSettings,
    EditControls,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuickSettingsScene {
    tiles: Vec<QuickSettingsTile>,
    display: Option<QuickSettingsDisplay>,
    sound: Option<QuickSettingsSound>,
    energy: Option<QuickSettingsEnergy>,
    media: Option<QuickSettingsMediaPlayer>,
    media_choices: Option<Vec<QuickSettingsMediaChoice>>,
    submenu: Option<QuickSettingsSubmenu>,
    audio_panel: Option<QuickSettingsAudioPanel>,
    focused: Option<QuickSettingsFocus>,
    hovered: Option<QuickSettingsHit>,
    pressed: Option<QuickSettingsHit>,
    scroll_offset: f32,
    anchor_x: Option<f32>,
    status_text: String,
}

impl QuickSettingsScene {
    #[must_use]
    pub fn new(tiles: Vec<QuickSettingsTile>) -> Self {
        Self {
            tiles,
            display: None,
            sound: None,
            energy: None,
            media: None,
            media_choices: None,
            submenu: None,
            audio_panel: None,
            focused: None,
            hovered: None,
            pressed: None,
            scroll_offset: 0.0,
            anchor_x: None,
            status_text: String::new(),
        }
    }
    #[must_use]
    pub fn with_display(mut self, value: Option<QuickSettingsDisplay>) -> Self {
        self.display = value;
        self
    }
    #[must_use]
    pub fn with_sound(mut self, value: Option<QuickSettingsSound>) -> Self {
        self.sound = value;
        self
    }
    #[must_use]
    pub fn with_energy(mut self, value: Option<QuickSettingsEnergy>) -> Self {
        self.energy = value;
        self
    }
    #[must_use]
    pub fn with_media(mut self, value: Option<QuickSettingsMediaPlayer>) -> Self {
        self.media = value;
        self
    }
    #[must_use]
    pub fn with_media_choices(mut self, value: Option<Vec<QuickSettingsMediaChoice>>) -> Self {
        self.media_choices = value;
        self
    }
    #[must_use]
    pub fn with_submenu(mut self, value: Option<QuickSettingsSubmenu>) -> Self {
        self.submenu = value;
        self
    }
    #[must_use]
    pub fn with_audio_panel(mut self, value: Option<QuickSettingsAudioPanel>) -> Self {
        self.audio_panel = value;
        self
    }
    #[must_use]
    pub fn with_focus(mut self, value: Option<QuickSettingsFocus>) -> Self {
        self.focused = value;
        self
    }
    #[must_use]
    pub fn with_hover(mut self, value: Option<QuickSettingsHit>) -> Self {
        self.hovered = value;
        self
    }
    #[must_use]
    pub fn with_pressed(mut self, value: Option<QuickSettingsHit>) -> Self {
        self.pressed = value;
        self
    }
    #[must_use]
    pub fn with_scroll_offset(mut self, value: f32) -> Self {
        self.scroll_offset = value.max(0.0);
        self
    }
    #[must_use]
    pub fn with_anchor_x(mut self, value: f32) -> Self {
        self.anchor_x = value.is_finite().then_some(value);
        self
    }
    #[must_use]
    pub fn with_status_text(mut self, value: &str) -> Self {
        self.status_text = value.to_owned();
        self
    }
    #[must_use]
    pub fn tiles(&self) -> &[QuickSettingsTile] {
        &self.tiles
    }
    #[must_use]
    pub const fn display(&self) -> Option<&QuickSettingsDisplay> {
        self.display.as_ref()
    }
    #[must_use]
    pub const fn sound(&self) -> Option<&QuickSettingsSound> {
        self.sound.as_ref()
    }
    #[must_use]
    pub const fn energy(&self) -> Option<&QuickSettingsEnergy> {
        self.energy.as_ref()
    }
    #[must_use]
    pub const fn media(&self) -> Option<&QuickSettingsMediaPlayer> {
        self.media.as_ref()
    }
    #[must_use]
    pub fn media_choices(&self) -> Option<&[QuickSettingsMediaChoice]> {
        self.media_choices.as_deref()
    }
    #[must_use]
    pub const fn submenu(&self) -> Option<&QuickSettingsSubmenu> {
        self.submenu.as_ref()
    }
    #[must_use]
    pub const fn audio_panel(&self) -> Option<&QuickSettingsAudioPanel> {
        self.audio_panel.as_ref()
    }
    #[must_use]
    pub const fn focused(&self) -> Option<QuickSettingsFocus> {
        self.focused
    }
    #[must_use]
    pub const fn hovered(&self) -> Option<QuickSettingsHit> {
        self.hovered
    }
    #[must_use]
    pub const fn pressed(&self) -> Option<QuickSettingsHit> {
        self.pressed
    }
    #[must_use]
    pub fn slider_feedback_value(&self, kind: QuickControlKind) -> Option<u8> {
        match self.pressed {
            Some(QuickSettingsHit::Slider {
                kind: pressed_kind,
                value: pressed_value,
            }) if pressed_kind == kind => match self.hovered {
                Some(QuickSettingsHit::Slider {
                    kind: hovered_kind,
                    value,
                }) if hovered_kind == kind => Some(value),
                _ => Some(pressed_value),
            },
            _ => None,
        }
    }
    #[must_use]
    pub fn audio_slider_feedback_value(&self, id: QuickSettingsAudioSessionId) -> Option<u8> {
        match self.pressed {
            Some(QuickSettingsHit::AudioSessionSlider {
                id: pressed_id,
                value: pressed_value,
            }) if pressed_id == id => match self.hovered {
                Some(QuickSettingsHit::AudioSessionSlider {
                    id: hovered_id,
                    value,
                }) if hovered_id == id => Some(value),
                _ => Some(pressed_value),
            },
            _ => None,
        }
    }
    #[must_use]
    pub const fn scroll_offset(&self) -> f32 {
        self.scroll_offset
    }
    #[must_use]
    pub const fn anchor_x(&self) -> Option<f32> {
        self.anchor_x
    }
    #[must_use]
    pub fn status_text(&self) -> &str {
        &self.status_text
    }
}
