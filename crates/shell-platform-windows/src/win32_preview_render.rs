#![deny(unsafe_code)]

use shell_core::DockItemId;
use shell_renderer::native::ShowcaseRole;
use shell_renderer::{
    PreviewCardVisual, PreviewPlacementInput, PreviewUnavailableReason,
    WINDOW_PREVIEW_THUMBNAIL_RADIUS, WindowPreviewCapture, WindowPreviewScene, WindowPreviewVisual,
    layout_dock_scene, layout_window_preview, physical_from_dip, place_window_preview,
    preview_panel_size_for_scene,
};
use windows::core::Result;

use crate::preview_motion::{PREVIEW_ENTRANCE_DURATION_MS, preview_reveal_state};
use crate::win32_dock_render::dip_surface;
use crate::win32_owner::{RuntimeSurfaces, SurfaceWindows, now_ms};
use crate::win32_preview::{
    DwmPreviewThumbnail, reveal_preview_composition, stage_preview_composition,
};
use crate::win32_preview_geometry::{
    physical_anchor, physical_card_rect, renderer_capture, thumbnail_opacity,
};
use crate::win32_preview_source::preview_source_size;
use crate::win32_surface_runtime::SurfaceUpdate;
use crate::win32_window::OwnedWindow;
use crate::{PreviewEntranceMotion, PreviewMotionSpec};

const PREVIEW_ENTRANCE_OFFSET_DIP: f32 = 4.0;
const PREVIEW_FIRST_COMPOSED_FRAME_MS: u64 = 16;

pub(super) struct PreviewPresentation {
    item: DockItemId,
    page: usize,
    activate: bool,
    animate: bool,
}

pub(super) struct PreviewRenderWindows<'a> {
    pub(super) topbar: &'a OwnedWindow,
    pub(super) dock: &'a OwnedWindow,
    pub(super) popover: &'a OwnedWindow,
    pub(super) preview: &'a mut OwnedWindow,
    pub(super) settings: &'a OwnedWindow,
}

impl PreviewPresentation {
    pub(super) const fn initial(item: DockItemId, page: usize, activate: bool) -> Self {
        Self {
            item,
            page,
            activate,
            animate: true,
        }
    }

    pub(super) const fn update(item: DockItemId, page: usize) -> Self {
        Self {
            item,
            page,
            activate: false,
            animate: false,
        }
    }
}

impl RuntimeSurfaces {
    pub(super) fn show_window_preview(
        &mut self,
        presentation: PreviewPresentation,
        windows: PreviewRenderWindows<'_>,
    ) -> Result<()> {
        let PreviewRenderWindows {
            topbar,
            dock,
            popover,
            preview,
            settings,
        } = windows;
        let PreviewPresentation {
            item,
            page,
            activate,
            animate,
        } = presentation;
        let states = self.dock_controller.preview_windows_for_item(item);
        if states.is_empty() {
            self.hide_window_preview(preview);
            return Ok(());
        }
        let dock_scene = self.dock_controller.scene();
        let Some(item_visual) = dock_scene
            .items()
            .iter()
            .find(|visual| visual.id() == item.value())
        else {
            self.hide_window_preview(preview);
            return Ok(());
        };
        let cards = states
            .iter()
            .map(|state| {
                let mut card = PreviewCardVisual::new(
                    state.window(),
                    state.title(),
                    renderer_capture(state.capture()),
                )
                .with_focused(state.foreground())
                .with_minimized(state.minimized());
                if let Ok(source_size) = preview_source_size(preview.hwnd, state.window()) {
                    card = card.with_source_size(source_size);
                }
                card
            })
            .collect::<Vec<_>>();
        let mut scene = WindowPreviewScene::new(item, item_visual.label(), cards)
            .with_icon(item_visual.icon().clone())
            .with_page(page)
            .with_reduced_motion(self.reduced_motion);
        let layout = layout_dock_scene(&dock_scene, dip_surface(dock));
        let Some(anchor_item) = layout
            .items()
            .iter()
            .find(|entry| entry.id() == item.value())
        else {
            self.hide_window_preview(preview);
            return Ok(());
        };
        let anchor = physical_anchor(dock, anchor_item.bounds());
        let size = preview_panel_size_for_scene(&scene);
        let work = crate::win32_windowing::window_work_area(dock.hwnd)?;
        let rect = place_window_preview(
            PreviewPlacementInput {
                anchor,
                dock: dock.rect,
                work,
                dpi: preview.dpi(),
            },
            size,
        );
        let animation_now = now_ms();
        self.preview_entrance = if animate && !self.reduced_motion {
            Some(PreviewEntranceMotion::new(
                rect,
                PreviewMotionSpec {
                    started_ms: animation_now.saturating_sub(PREVIEW_FIRST_COMPOSED_FRAME_MS),
                    duration_ms: PREVIEW_ENTRANCE_DURATION_MS,
                    offset_px: physical_from_dip(PREVIEW_ENTRANCE_OFFSET_DIP, preview.dpi()),
                },
            ))
        } else {
            None
        };
        let initial_rect = self
            .preview_entrance
            .map_or(rect, |motion| motion.rect_at(animation_now));
        preview.place_preview(initial_rect)?;
        preview.refresh_rect()?;
        self.redraw_window_preview(
            SurfaceWindows {
                topbar,
                dock,
                popover,
                preview: &*preview,
                settings,
            },
            &scene,
        )?;
        let initial_opacity = self
            .preview_entrance
            .map_or(1.0, |motion| motion.opacity_at(animation_now));
        let (cloaked, warmup_opacity, initial_opacity) = preview_reveal_state(initial_opacity);
        let update = self
            .surface_runtime
            .set_opacity(ShowcaseRole::Preview, warmup_opacity);
        self.complete_surface_update(
            update,
            SurfaceWindows {
                topbar,
                dock,
                popover,
                preview: &*preview,
                settings,
            },
        )?;
        self.preview_thumbnails.clear();
        let preview_layout = layout_window_preview(&scene, dip_surface(preview));
        let mut unavailable_windows = Vec::new();
        for (card, visual) in preview_layout.cards().iter().zip(scene.visible_cards()) {
            if !matches!(visual.capture(), WindowPreviewCapture::DwmThumbnail) {
                continue;
            }
            let destination = physical_card_rect(card.thumbnail(), preview.dpi());
            let thumbnail = DwmPreviewThumbnail::show(
                preview.hwnd,
                preview.rect,
                WindowPreviewVisual::available(visual.window(), item),
                destination,
                physical_from_dip(WINDOW_PREVIEW_THUMBNAIL_RADIUS, preview.dpi()),
                thumbnail_opacity(warmup_opacity),
            );
            match thumbnail {
                Ok(Some(thumbnail)) => self.preview_thumbnails.push(thumbnail),
                Ok(None) => {}
                Err(_) => unavailable_windows.push(visual.window()),
            }
        }
        if !unavailable_windows.is_empty() {
            for window in unavailable_windows {
                scene = scene.with_card_capture(
                    window,
                    WindowPreviewCapture::Restricted(PreviewUnavailableReason::SourceUnavailable),
                );
            }
            self.redraw_window_preview(
                SurfaceWindows {
                    topbar,
                    dock,
                    popover,
                    preview: &*preview,
                    settings,
                },
                &scene,
            )?;
        }
        self.preview_scene = Some(scene);
        self.preview_layout = Some(preview_layout);

        stage_preview_composition(preview.hwnd, &self.preview_thumbnails, cloaked)?;
        if activate {
            preview.show_activating();
        } else {
            preview.show();
        }

        let initial_thumbnail_opacity = thumbnail_opacity(initial_opacity);
        for thumbnail in &mut self.preview_thumbnails {
            thumbnail.set_opacity(initial_thumbnail_opacity)?;
        }
        let update = self
            .surface_runtime
            .set_opacity(ShowcaseRole::Preview, initial_opacity);
        self.complete_surface_update(
            update,
            SurfaceWindows {
                topbar,
                dock,
                popover,
                preview: &*preview,
                settings,
            },
        )?;
        reveal_preview_composition(preview.hwnd, &self.preview_thumbnails, cloaked)?;
        Ok(())
    }

    pub(super) fn hide_window_preview(&mut self, preview: &OwnedWindow) {
        self.preview_entrance = None;
        self.preview_thumbnails.clear();
        self.preview_layout = None;
        self.preview_scene = None;
        preview.hide();
    }

    pub(super) fn refit_window_previews(&mut self, preview: &OwnedWindow) {
        self.preview_thumbnails
            .retain_mut(|thumbnail| thumbnail.refit(preview.rect).is_ok());
    }

    pub(super) fn advance_preview_entrance(
        &mut self,
        now_ms: u64,
        preview: &mut OwnedWindow,
    ) -> Result<SurfaceUpdate> {
        let Some(motion) = self.preview_entrance else {
            return Ok(SurfaceUpdate::Presented);
        };
        let owner_rect = motion.rect_at(now_ms);
        preview.place_preview(owner_rect)?;
        let opacity = motion.opacity_at(now_ms);
        let update = self
            .surface_runtime
            .set_opacity(ShowcaseRole::Preview, opacity)?;
        let thumbnail_opacity = thumbnail_opacity(opacity);
        self.preview_thumbnails.retain_mut(|thumbnail| {
            thumbnail.reposition(owner_rect).is_ok()
                && thumbnail.set_opacity(thumbnail_opacity).is_ok()
        });
        if motion.finished(now_ms) {
            self.preview_entrance = None;
        }
        Ok(update)
    }
}
