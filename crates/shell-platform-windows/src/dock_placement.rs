#![deny(unsafe_code)]

use shell_renderer::PhysicalRect;

use crate::DockRuntimeConfig;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DockPhysicalPlacement {
    rect: PhysicalRect,
    hidden_strip: bool,
}

impl DockPhysicalPlacement {
    #[must_use]
    pub fn from_visibility(
        normal: PhysicalRect,
        config: DockRuntimeConfig,
        hidden: bool,
        scale: f32,
    ) -> Self {
        if !hidden || !config.autohide() {
            return Self {
                rect: normal,
                hidden_strip: false,
            };
        }
        let reveal_height = (config.reveal_zone_height() * scale).ceil() as i32;
        let height = reveal_height.clamp(1, normal.height.max(1));
        Self {
            rect: PhysicalRect::new(
                normal.x,
                normal.y + normal.height - height,
                normal.width,
                height,
            ),
            hidden_strip: true,
        }
    }

    #[must_use]
    pub const fn rect(self) -> PhysicalRect {
        self.rect
    }

    #[must_use]
    pub const fn is_hidden_strip(self) -> bool {
        self.hidden_strip
    }
}
