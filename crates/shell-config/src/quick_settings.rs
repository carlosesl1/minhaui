use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use shell_core::{QuickControlKind, QuickControlPlacement};

/// User-owned order and visibility, retained even while hardware is absent.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct QuickSettingsSettings {
    controls: Vec<QuickControlPlacement>,
}

impl Default for QuickSettingsSettings {
    fn default() -> Self {
        Self {
            controls: QuickControlKind::ALL
                .into_iter()
                .map(|kind| QuickControlPlacement::new(kind, true))
                .collect(),
        }
    }
}

impl QuickSettingsSettings {
    #[must_use]
    pub fn from_controls(controls: Vec<QuickControlPlacement>) -> Self {
        Self { controls }
    }

    #[must_use]
    pub fn controls(&self) -> &[QuickControlPlacement] {
        &self.controls
    }

    #[must_use]
    pub fn normalized(&self) -> Self {
        let mut seen = HashSet::new();
        let mut controls = self
            .controls
            .iter()
            .copied()
            .filter(|placement| seen.insert(placement.kind()))
            .collect::<Vec<_>>();
        controls.extend(
            QuickControlKind::ALL
                .into_iter()
                .filter(|kind| seen.insert(*kind))
                .map(|kind| QuickControlPlacement::new(kind, true)),
        );
        Self { controls }
    }

    #[must_use]
    pub fn with_visibility(&self, kind: QuickControlKind, visible: bool) -> Self {
        let mut normalized = self.normalized();
        if let Some(placement) = normalized
            .controls
            .iter_mut()
            .find(|placement| placement.kind() == kind)
        {
            *placement = QuickControlPlacement::new(kind, visible);
        }
        normalized
    }

    #[must_use]
    pub fn reordered(&self, kind: QuickControlKind, before: Option<QuickControlKind>) -> Self {
        let mut normalized = self.normalized();
        let Some(index) = normalized
            .controls
            .iter()
            .position(|placement| placement.kind() == kind)
        else {
            return normalized;
        };
        let placement = normalized.controls.remove(index);
        let insertion = before
            .and_then(|target| {
                normalized
                    .controls
                    .iter()
                    .position(|candidate| candidate.kind() == target)
            })
            .unwrap_or(normalized.controls.len());
        normalized.controls.insert(insertion, placement);
        normalized
    }

    #[must_use]
    pub fn validate(&self) -> bool {
        self.controls.len() <= QuickControlKind::ALL.len()
            && self.controls.len() == QuickControlKind::ALL.len()
            && self
                .controls
                .iter()
                .map(|placement| placement.kind())
                .collect::<HashSet<_>>()
                .len()
                == QuickControlKind::ALL.len()
    }
}
