#![deny(unsafe_code)]

use shell_core::DockItemId;

pub const PREVIEW_DWELL_MS: u64 = 300;
pub const PREVIEW_BRIDGE_MS: u64 = 200;
pub const PREVIEW_MOTION_MS: u64 = 160;
#[expect(
    dead_code,
    reason = "preview pagination is retained for the next native preview increment"
)]
pub const PREVIEW_PAGE_SIZE: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreviewPhase {
    Closed,
    Dwelling {
        item: DockItemId,
        deadline_ms: u64,
    },
    Visible {
        item: DockItemId,
        page: usize,
        bridge_deadline_ms: Option<u64>,
    },
    Closing {
        item: DockItemId,
        deadline_ms: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreviewEffect {
    Show {
        item: DockItemId,
        page: usize,
        from_keyboard: bool,
    },
    Update {
        item: DockItemId,
        page: usize,
    },
    BeginClose,
    Hide,
    HoldDockReveal,
    ReleaseDockReveal,
}

pub struct PreviewController {
    phase: PreviewPhase,
    reduced_motion: bool,
}

impl PreviewController {
    #[must_use]
    pub const fn new(reduced_motion: bool) -> Self {
        Self {
            phase: PreviewPhase::Closed,
            reduced_motion,
        }
    }

    #[must_use]
    pub const fn phase(&self) -> PreviewPhase {
        self.phase
    }

    #[must_use]
    pub const fn visible_item(&self) -> Option<DockItemId> {
        match self.phase {
            PreviewPhase::Visible { item, .. } | PreviewPhase::Closing { item, .. } => Some(item),
            PreviewPhase::Closed | PreviewPhase::Dwelling { .. } => None,
        }
    }

    pub fn dock_target_changed(
        &mut self,
        target: Option<DockItemId>,
        now_ms: u64,
    ) -> Vec<PreviewEffect> {
        match (self.phase, target) {
            (PreviewPhase::Dwelling { item, .. }, Some(target)) if item == target => Vec::new(),
            (PreviewPhase::Closed, Some(item)) | (PreviewPhase::Dwelling { .. }, Some(item)) => {
                self.phase = PreviewPhase::Dwelling {
                    item,
                    deadline_ms: now_ms.saturating_add(PREVIEW_DWELL_MS),
                };
                Vec::new()
            }
            (PreviewPhase::Visible { item, page, .. }, Some(target)) if item == target => {
                self.phase = PreviewPhase::Visible {
                    item,
                    page,
                    bridge_deadline_ms: None,
                };
                Vec::new()
            }
            (PreviewPhase::Visible { .. } | PreviewPhase::Closing { .. }, Some(item)) => {
                self.phase = PreviewPhase::Visible {
                    item,
                    page: 0,
                    bridge_deadline_ms: None,
                };
                vec![PreviewEffect::Update { item, page: 0 }]
            }
            (PreviewPhase::Dwelling { .. }, None) => {
                self.phase = PreviewPhase::Closed;
                Vec::new()
            }
            (PreviewPhase::Visible { item, page, .. }, None) => {
                self.phase = PreviewPhase::Visible {
                    item,
                    page,
                    bridge_deadline_ms: Some(now_ms.saturating_add(PREVIEW_BRIDGE_MS)),
                };
                Vec::new()
            }
            (PreviewPhase::Closed | PreviewPhase::Closing { .. }, None) => Vec::new(),
        }
    }

    pub fn open_from_keyboard(&mut self, item: DockItemId, _now_ms: u64) -> Vec<PreviewEffect> {
        let hold = !matches!(self.phase, PreviewPhase::Visible { .. });
        self.phase = PreviewPhase::Visible {
            item,
            page: 0,
            bridge_deadline_ms: None,
        };
        let mut effects = Vec::with_capacity(2);
        if hold {
            effects.push(PreviewEffect::HoldDockReveal);
        }
        effects.push(PreviewEffect::Show {
            item,
            page: 0,
            from_keyboard: true,
        });
        effects
    }

    pub fn preview_entered(&mut self) {
        if let PreviewPhase::Visible { item, page, .. } = self.phase {
            self.phase = PreviewPhase::Visible {
                item,
                page,
                bridge_deadline_ms: None,
            };
        }
    }

    pub fn dismiss(&mut self, now_ms: u64) -> Vec<PreviewEffect> {
        match self.phase {
            PreviewPhase::Visible { item, .. } | PreviewPhase::Closing { item, .. } => {
                self.begin_close(item, now_ms)
            }
            PreviewPhase::Dwelling { .. } => {
                self.phase = PreviewPhase::Closed;
                Vec::new()
            }
            PreviewPhase::Closed => Vec::new(),
        }
    }

    pub fn set_page(&mut self, page: usize) -> Vec<PreviewEffect> {
        let PreviewPhase::Visible { item, .. } = self.phase else {
            return Vec::new();
        };
        self.phase = PreviewPhase::Visible {
            item,
            page,
            bridge_deadline_ms: None,
        };
        vec![PreviewEffect::Update { item, page }]
    }

    pub fn tick(&mut self, now_ms: u64) -> Vec<PreviewEffect> {
        match self.phase {
            PreviewPhase::Dwelling { item, deadline_ms } if now_ms >= deadline_ms => {
                self.phase = PreviewPhase::Visible {
                    item,
                    page: 0,
                    bridge_deadline_ms: None,
                };
                vec![
                    PreviewEffect::HoldDockReveal,
                    PreviewEffect::Show {
                        item,
                        page: 0,
                        from_keyboard: false,
                    },
                ]
            }
            PreviewPhase::Visible {
                item,
                bridge_deadline_ms: Some(deadline_ms),
                ..
            } if now_ms >= deadline_ms => self.begin_close(item, now_ms),
            PreviewPhase::Closing { deadline_ms, .. } if now_ms >= deadline_ms => {
                self.phase = PreviewPhase::Closed;
                vec![PreviewEffect::Hide, PreviewEffect::ReleaseDockReveal]
            }
            PreviewPhase::Closed
            | PreviewPhase::Dwelling { .. }
            | PreviewPhase::Visible { .. }
            | PreviewPhase::Closing { .. } => Vec::new(),
        }
    }

    fn begin_close(&mut self, item: DockItemId, now_ms: u64) -> Vec<PreviewEffect> {
        if self.reduced_motion {
            self.phase = PreviewPhase::Closed;
            vec![PreviewEffect::Hide, PreviewEffect::ReleaseDockReveal]
        } else {
            self.phase = PreviewPhase::Closing {
                item,
                deadline_ms: now_ms.saturating_add(PREVIEW_MOTION_MS),
            };
            vec![PreviewEffect::BeginClose]
        }
    }
}
