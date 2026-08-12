#![deny(unsafe_code)]

use shell_core::WindowId;
use shell_renderer::native::{ShellScenes, ShowcaseRole, SurfaceMetrics};
use shell_renderer::{DipPoint, DipRect, WindowPreviewScene};
use windows::core::Result;

use crate::PreviewAction;
use crate::win32_owner::{RuntimeSurfaces, SurfaceWindows};
use crate::win32_surface_runtime::{
    SurfaceFrame, SurfaceSizeChange, SurfaceTarget, SurfaceUpdate, present_frame,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PreviewHit {
    Window(WindowId, PreviewAction),
    Page(usize),
}

impl RuntimeSurfaces {
    pub(super) fn preview_hit(&self, point: DipPoint) -> Option<PreviewHit> {
        let layout = self.preview_layout.as_ref()?;
        for card in layout.cards() {
            if contains(card.close(), point) {
                return Some(PreviewHit::Window(card.window(), PreviewAction::Close));
            }
            if contains(card.card(), point) {
                return Some(PreviewHit::Window(card.window(), PreviewAction::Focus));
            }
        }
        let page = self.preview_scene.as_ref()?.corrected_page();
        if layout
            .previous()
            .is_some_and(|bounds| contains(bounds, point))
            && page > 0
        {
            return Some(PreviewHit::Page(page - 1));
        }
        if layout.next().is_some_and(|bounds| contains(bounds, point))
            && page + 1 < self.preview_scene.as_ref()?.page_count()
        {
            return Some(PreviewHit::Page(page + 1));
        }
        None
    }

    pub(super) fn update_preview_hover(
        &mut self,
        point: DipPoint,
        windows: SurfaceWindows<'_>,
    ) -> Result<()> {
        let hovered = self.preview_layout.as_ref().and_then(|layout| {
            layout
                .cards()
                .iter()
                .find(|card| contains(card.card(), point))
                .map(|card| card.window())
        });
        let hovered_close = self.preview_layout.as_ref().and_then(|layout| {
            layout
                .cards()
                .iter()
                .find(|card| contains(card.close(), point))
                .map(|card| card.window())
        });
        let Some(scene) = self.preview_scene.clone() else {
            return Ok(());
        };
        if scene.hovered_card() == hovered && scene.hovered_close() == hovered_close {
            return Ok(());
        }
        let scene = scene
            .with_hovered_card(hovered)
            .with_hovered_close(hovered_close);
        self.redraw_window_preview(windows, &scene)?;
        self.preview_scene = Some(scene);
        Ok(())
    }

    pub(super) fn redraw_window_preview(
        &mut self,
        windows: SurfaceWindows<'_>,
        scene: &WindowPreviewScene,
    ) -> Result<()> {
        let preview = windows.preview;
        let metrics = SurfaceMetrics::new(
            preview.rect.width.max(1) as u32,
            preview.rect.height.max(1) as u32,
            preview.dpi(),
        );
        let scenes = ShellScenes {
            topbar: None,
            dock: None,
            popover: None,
            quick_settings: None,
            context_menu: None,
            settings: None,
            preview: Some(scene),
        };
        let update = present_frame(
            &mut self.surface_runtime,
            SurfaceFrame {
                target: SurfaceTarget {
                    hwnd: preview.hwnd,
                    role: ShowcaseRole::Preview,
                    metrics,
                },
                scenes,
            },
            SurfaceSizeChange::Resize,
        );
        finish_preview_update_with(update, self, scene, runtime_preview_scene, |runtime| {
            runtime.rebuild_native_surfaces(windows)
        })
    }
}

fn runtime_preview_scene(runtime: &mut RuntimeSurfaces) -> &mut Option<WindowPreviewScene> {
    &mut runtime.preview_scene
}

fn finish_preview_update_with<T, S: Clone, E>(
    outcome: std::result::Result<SurfaceUpdate, E>,
    owner: &mut T,
    requested: &S,
    scene_slot: for<'owner> fn(&'owner mut T) -> &'owner mut Option<S>,
    rebuild: impl FnOnce(&mut T) -> std::result::Result<(), E>,
) -> std::result::Result<(), E> {
    match outcome? {
        SurfaceUpdate::Presented | SurfaceUpdate::FrameSkipped => Ok(()),
        SurfaceUpdate::RebuildAllRequired => {
            let previous = scene_slot(owner).replace(requested.clone());
            if let Err(error) = rebuild(owner) {
                *scene_slot(owner) = previous;
                Err(error)
            } else {
                Ok(())
            }
        }
    }
}

fn contains(bounds: DipRect, point: DipPoint) -> bool {
    point.x >= bounds.x
        && point.x <= bounds.x + bounds.width
        && point.y >= bounds.y
        && point.y <= bounds.y + bounds.height
}

#[cfg(test)]
mod tests {
    use crate::win32_surface_runtime::{
        SurfaceRenderDecision, SurfaceUpdate, surface_render_decision,
    };
    use shell_renderer::Dpi;
    use shell_renderer::native::SurfaceMetrics;

    use super::finish_preview_update_with;

    #[derive(Debug, Eq, PartialEq)]
    struct PreviewState {
        scene: Option<u8>,
        rebuild_saw: Vec<Option<u8>>,
    }

    fn preview_scene(state: &mut PreviewState) -> &mut Option<u8> {
        &mut state.scene
    }

    #[test]
    fn requested_recovery_promotes_before_rebuild_restores_on_error_and_ignores_fatal() {
        let mut successful = PreviewState {
            scene: Some(1),
            rebuild_saw: Vec::new(),
        };
        finish_preview_update_with(
            Ok::<_, &'static str>(SurfaceUpdate::RebuildAllRequired),
            &mut successful,
            &2,
            preview_scene,
            |state| {
                state.rebuild_saw.push(state.scene);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(successful.scene, Some(2));
        assert_eq!(successful.rebuild_saw, [Some(2)]);

        let mut failed = PreviewState {
            scene: Some(3),
            rebuild_saw: Vec::new(),
        };
        let error = finish_preview_update_with(
            Ok::<_, &'static str>(SurfaceUpdate::RebuildAllRequired),
            &mut failed,
            &4,
            preview_scene,
            |state| {
                state.rebuild_saw.push(state.scene);
                Err("rebuild")
            },
        )
        .unwrap_err();
        assert_eq!(error, "rebuild");
        assert_eq!(failed.scene, Some(3));
        assert_eq!(failed.rebuild_saw, [Some(4)]);

        let mut fatal = PreviewState {
            scene: Some(5),
            rebuild_saw: Vec::new(),
        };
        let error = finish_preview_update_with(
            Err::<SurfaceUpdate, _>("fatal"),
            &mut fatal,
            &6,
            preview_scene,
            |state| {
                state.rebuild_saw.push(state.scene);
                Ok(())
            },
        )
        .unwrap_err();
        assert_eq!(error, "fatal");
        assert_eq!(fatal.scene, Some(5));
        assert!(fatal.rebuild_saw.is_empty());
    }

    #[test]
    fn preview_surface_policy_covers_missing_redraw_resize_and_recovery() {
        let desired = SurfaceMetrics::new(480, 320, Dpi::from_raw(144));
        let cases = [
            (None, SurfaceRenderDecision::RebuildAll),
            (Some(desired), SurfaceRenderDecision::Redraw),
            (
                Some(SurfaceMetrics::new(420, 320, Dpi::from_raw(144))),
                SurfaceRenderDecision::Resize,
            ),
        ];
        for (current, expected) in cases {
            assert_eq!(surface_render_decision(current, desired), expected);
        }
    }
}
