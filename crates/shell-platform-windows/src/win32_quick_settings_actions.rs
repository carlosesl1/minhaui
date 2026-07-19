use shell_core::QuickControlKind;

use crate::{
    ProjectionMode, QuickControlAvailability, QuickControlCapability, QuickSettingsIntent,
};

pub(super) enum QuickSettingsActionResult {
    Applied(QuickControlCapability),
    NoChange,
    Failed(windows::core::Error),
}

pub(super) fn apply_quick_settings_intent(
    intent: &QuickSettingsIntent,
    capability: Option<&QuickControlCapability>,
) -> QuickSettingsActionResult {
    let result = match *intent {
        QuickSettingsIntent::SetValue {
            kind: QuickControlKind::Volume,
            value,
        } => set_volume(capability, value),
        QuickSettingsIntent::SetValue { kind, .. } => Err(no_direct_action(kind)),
        QuickSettingsIntent::Activate(QuickControlKind::Volume) => toggle_mute(capability),
        QuickSettingsIntent::Activate(QuickControlKind::NightLight) => {
            return QuickSettingsActionResult::NoChange;
        }
        QuickSettingsIntent::SetProjectionMode(mode) => set_projection_mode(capability, mode),
        QuickSettingsIntent::OpenMediaSessions
        | QuickSettingsIntent::MediaAction(_)
        | QuickSettingsIntent::SelectMediaSession(_) => {
            return QuickSettingsActionResult::NoChange;
        }
        QuickSettingsIntent::Activate(QuickControlKind::Projection) => {
            Err(no_direct_action(QuickControlKind::Projection))
        }
        QuickSettingsIntent::Activate(kind) => toggle_native(kind, capability),
        QuickSettingsIntent::EditControls | QuickSettingsIntent::Dismiss => {
            return QuickSettingsActionResult::NoChange;
        }
    };
    match result {
        Ok(control) => QuickSettingsActionResult::Applied(control),
        Err(error) => QuickSettingsActionResult::Failed(error),
    }
}

fn set_volume(
    capability: Option<&QuickControlCapability>,
    value: u8,
) -> windows::core::Result<QuickControlCapability> {
    let capability = require_capability(QuickControlKind::Volume, capability)?;
    let state = crate::win32_audio_endpoint::set_volume(value)?;
    Ok(updated(
        capability,
        state.muted,
        Some(state.volume),
        capability.detail(),
    ))
}

fn toggle_mute(
    capability: Option<&QuickControlCapability>,
) -> windows::core::Result<QuickControlCapability> {
    let capability = require_capability(QuickControlKind::Volume, capability)?;
    let state = crate::win32_audio_endpoint::toggle_mute()?;
    Ok(updated(
        capability,
        state.muted,
        Some(state.volume),
        capability.detail(),
    ))
}

fn toggle_native(
    kind: QuickControlKind,
    capability: Option<&QuickControlCapability>,
) -> windows::core::Result<QuickControlCapability> {
    let capability = require_capability(kind, capability)?;
    let target = !active(capability);
    crate::win32_quick_settings_system::set_active(kind, target)?;
    Ok(updated(
        capability,
        target,
        capability.value(),
        direct_control_detail(kind, target),
    ))
}

const fn direct_control_detail(_kind: QuickControlKind, active: bool) -> &'static str {
    state_detail(active)
}

fn set_projection_mode(
    capability: Option<&QuickControlCapability>,
    mode: ProjectionMode,
) -> windows::core::Result<QuickControlCapability> {
    let capability = require_capability(QuickControlKind::Projection, capability)?;
    crate::win32_system_actions::set_projection_mode(mode)?;
    Ok(updated(
        capability,
        mode != ProjectionMode::Internal,
        Some(projection_mode_value(mode)),
        projection_mode_label(mode),
    ))
}

const fn projection_mode_value(mode: ProjectionMode) -> u8 {
    match mode {
        ProjectionMode::Internal => 0,
        ProjectionMode::Duplicate => 1,
        ProjectionMode::Extend => 2,
        ProjectionMode::External => 3,
    }
}

const fn projection_mode_label(mode: ProjectionMode) -> &'static str {
    match mode {
        ProjectionMode::Internal => "PC screen only",
        ProjectionMode::Duplicate => "Duplicate",
        ProjectionMode::Extend => "Extend",
        ProjectionMode::External => "Second screen only",
    }
}

fn require_capability(
    kind: QuickControlKind,
    capability: Option<&QuickControlCapability>,
) -> windows::core::Result<&QuickControlCapability> {
    capability.ok_or_else(|| {
        windows::core::Error::new(
            invalid_arg(),
            format!("{kind:?} is not available on this computer"),
        )
    })
}

fn updated(
    source: &QuickControlCapability,
    active: bool,
    value: Option<u8>,
    detail: &str,
) -> QuickControlCapability {
    QuickControlCapability::new(
        source.kind(),
        QuickControlAvailability::Available { active },
        source.label(),
        detail,
        value,
    )
}

fn active(capability: &QuickControlCapability) -> bool {
    matches!(
        capability.availability(),
        QuickControlAvailability::Available { active: true }
    )
}

const fn state_detail(active: bool) -> &'static str {
    if active { "On" } else { "Off" }
}

fn no_direct_action(kind: QuickControlKind) -> windows::core::Error {
    windows::core::Error::new(
        invalid_arg(),
        format!("{kind:?} has no supported direct value action"),
    )
}

const fn invalid_arg() -> windows::core::HRESULT {
    windows::core::HRESULT(0x8007_0057_u32 as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_toggle_result_never_represents_a_settings_route() {
        let original = QuickControlCapability::new(
            QuickControlKind::DarkMode,
            QuickControlAvailability::Available { active: false },
            "Dark mode",
            "Off",
            None,
        );
        let changed = updated(&original, true, None, state_detail(true));
        assert_eq!(
            changed.availability(),
            QuickControlAvailability::Available { active: true }
        );
        assert_eq!(changed.detail(), "On");
    }
}
