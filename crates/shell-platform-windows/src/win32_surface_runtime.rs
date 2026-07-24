use shell_renderer::native::{
    CompositionRenderer, DeviceKind, PresentOutcome, ShellScenes, ShowcaseRole, SurfaceMetrics,
    SurfaceVisibilityAnimation, WindowSurface, is_recoverable_hresult,
};
use windows::Win32::Foundation::{E_INVALIDARG, E_UNEXPECTED, HWND};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SurfaceUpdate {
    Presented,
    RebuildAllRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SurfaceRenderDecision {
    RebuildAll,
    Redraw,
    Resize,
}

pub(super) fn surface_render_decision(
    current: Option<SurfaceMetrics>,
    desired: SurfaceMetrics,
) -> SurfaceRenderDecision {
    match current {
        None => SurfaceRenderDecision::RebuildAll,
        Some(metrics) if metrics != desired => SurfaceRenderDecision::Resize,
        Some(_) => SurfaceRenderDecision::Redraw,
    }
}

fn normalize_present_result(
    result: windows::core::Result<PresentOutcome>,
) -> windows::core::Result<SurfaceUpdate> {
    match result {
        Ok(PresentOutcome::Presented | PresentOutcome::FrameSkipped) => {
            Ok(SurfaceUpdate::Presented)
        }
        Ok(PresentOutcome::DeviceLost(_)) => Ok(SurfaceUpdate::RebuildAllRequired),
        Ok(PresentOutcome::Failed(code)) if is_recoverable_hresult(code) => {
            Ok(SurfaceUpdate::RebuildAllRequired)
        }
        Ok(PresentOutcome::Failed(code)) => Err(windows::core::Error::from_hresult(code)),
        Err(error) if is_recoverable_hresult(error.code()) => Ok(SurfaceUpdate::RebuildAllRequired),
        Err(error) => Err(error),
    }
}

fn normalize_unit_result(
    result: windows::core::Result<()>,
) -> windows::core::Result<SurfaceUpdate> {
    match result {
        Ok(()) => Ok(SurfaceUpdate::Presented),
        Err(error) if is_recoverable_hresult(error.code()) => Ok(SurfaceUpdate::RebuildAllRequired),
        Err(error) => Err(error),
    }
}

#[derive(Clone, Copy)]
pub(super) struct NativeSurfaceOptions {
    pub(super) force_warp: bool,
    pub(super) solid_material: bool,
    pub(super) liquid_glass: bool,
    pub(super) reduced_motion: bool,
}

#[derive(Clone, Copy)]
pub(super) struct SurfaceTarget {
    pub(super) hwnd: HWND,
    pub(super) role: ShowcaseRole,
    pub(super) metrics: SurfaceMetrics,
}

#[derive(Clone, Copy)]
pub(super) struct SurfaceFrame<'scene> {
    pub(super) target: SurfaceTarget,
    pub(super) scenes: ShellScenes<'scene>,
}

#[derive(Clone, Copy)]
pub(super) struct SurfaceBuildPlan<'scene> {
    pub(super) topbar: SurfaceFrame<'scene>,
    pub(super) dock: SurfaceFrame<'scene>,
    pub(super) popover: SurfaceFrame<'scene>,
    pub(super) app_menu: SurfaceFrame<'scene>,
    pub(super) preview: SurfaceFrame<'scene>,
    pub(super) settings: SurfaceFrame<'scene>,
    pub(super) dock_visibility_offset_y: f32,
    pub(super) dock_visibility_opacity: f32,
    pub(super) dock_visibility_animation: Option<SurfaceVisibilityAnimation>,
    pub(super) preview_opacity: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SurfaceSizeChange {
    Resize,
}

pub(super) trait SurfaceAdapter {
    type Renderer;
    type Surface;

    fn create_renderer(options: NativeSurfaceOptions) -> windows::core::Result<Self::Renderer>;
    fn device_kind(renderer: &Self::Renderer) -> DeviceKind;
    fn create_surface(
        renderer: &Self::Renderer,
        hwnd: HWND,
        role: ShowcaseRole,
        metrics: SurfaceMetrics,
        scenes: ShellScenes<'_>,
    ) -> windows::core::Result<Self::Surface>;
    fn redraw_surface(
        renderer: &Self::Renderer,
        surface: &Self::Surface,
        role: ShowcaseRole,
        scenes: ShellScenes<'_>,
    ) -> windows::core::Result<PresentOutcome>;
    fn resize_surface(
        renderer: &Self::Renderer,
        surface: &mut Self::Surface,
        metrics: SurfaceMetrics,
        role: ShowcaseRole,
        scenes: ShellScenes<'_>,
    ) -> windows::core::Result<PresentOutcome>;
    fn metrics(surface: &Self::Surface) -> SurfaceMetrics;
    fn set_opacity(surface: &Self::Surface, opacity: f32) -> windows::core::Result<()>;
    fn animate_entrance(surface: &Self::Surface, reduced_motion: bool)
    -> windows::core::Result<()>;
    fn animate_visibility(
        surface: &Self::Surface,
        spec: SurfaceVisibilityAnimation,
    ) -> windows::core::Result<()>;
    fn set_visibility_state(
        surface: &Self::Surface,
        offset_y: f32,
        opacity: f32,
    ) -> windows::core::Result<()>;
}

pub(super) struct DirectCompositionAdapter;

impl SurfaceAdapter for DirectCompositionAdapter {
    type Renderer = CompositionRenderer;
    type Surface = WindowSurface;

    fn create_renderer(options: NativeSurfaceOptions) -> windows::core::Result<Self::Renderer> {
        CompositionRenderer::new(
            options.force_warp,
            options.solid_material,
            options.liquid_glass,
            options.reduced_motion,
        )
    }

    fn device_kind(renderer: &Self::Renderer) -> DeviceKind {
        renderer.device_kind()
    }

    fn create_surface(
        renderer: &Self::Renderer,
        hwnd: HWND,
        role: ShowcaseRole,
        metrics: SurfaceMetrics,
        scenes: ShellScenes<'_>,
    ) -> windows::core::Result<Self::Surface> {
        renderer.create_surface(hwnd, role, metrics, scenes)
    }

    fn redraw_surface(
        renderer: &Self::Renderer,
        surface: &Self::Surface,
        role: ShowcaseRole,
        scenes: ShellScenes<'_>,
    ) -> windows::core::Result<PresentOutcome> {
        renderer.redraw_surface(surface, role, scenes)
    }

    fn resize_surface(
        renderer: &Self::Renderer,
        surface: &mut Self::Surface,
        metrics: SurfaceMetrics,
        role: ShowcaseRole,
        scenes: ShellScenes<'_>,
    ) -> windows::core::Result<PresentOutcome> {
        renderer.resize_surface(surface, metrics, role, scenes)
    }

    fn metrics(surface: &Self::Surface) -> SurfaceMetrics {
        surface.metrics()
    }

    fn set_opacity(surface: &Self::Surface, opacity: f32) -> windows::core::Result<()> {
        surface.set_opacity(opacity)
    }

    fn animate_entrance(
        surface: &Self::Surface,
        reduced_motion: bool,
    ) -> windows::core::Result<()> {
        surface.animate_entrance(reduced_motion)
    }

    fn animate_visibility(
        surface: &Self::Surface,
        spec: SurfaceVisibilityAnimation,
    ) -> windows::core::Result<()> {
        surface.animate_visibility(spec)
    }

    fn set_visibility_state(
        surface: &Self::Surface,
        offset_y: f32,
        opacity: f32,
    ) -> windows::core::Result<()> {
        surface.set_visibility_state(offset_y, opacity)
    }
}

struct SurfaceSet<S> {
    topbar: S,
    dock: S,
    popover: S,
    app_menu: S,
    preview: S,
    settings: S,
}

impl<S> SurfaceSet<S> {
    fn surface(&self, role: ShowcaseRole) -> &S {
        match role {
            ShowcaseRole::Topbar => &self.topbar,
            ShowcaseRole::Dock => &self.dock,
            ShowcaseRole::Popover => &self.popover,
            ShowcaseRole::AppMenu => &self.app_menu,
            ShowcaseRole::Preview => &self.preview,
            ShowcaseRole::Settings => &self.settings,
        }
    }

    fn surface_mut(&mut self, role: ShowcaseRole) -> &mut S {
        match role {
            ShowcaseRole::Topbar => &mut self.topbar,
            ShowcaseRole::Dock => &mut self.dock,
            ShowcaseRole::Popover => &mut self.popover,
            ShowcaseRole::AppMenu => &mut self.app_menu,
            ShowcaseRole::Preview => &mut self.preview,
            ShowcaseRole::Settings => &mut self.settings,
        }
    }
}

struct SurfaceResources<R, S> {
    surfaces: Option<SurfaceSet<S>>,
    renderer: Option<R>,
}

impl<R, S> SurfaceResources<R, S> {
    fn new(renderer: R, surfaces: SurfaceSet<S>) -> Self {
        Self {
            surfaces: Some(surfaces),
            renderer: Some(renderer),
        }
    }
}

impl<R, S> Drop for SurfaceResources<R, S> {
    fn drop(&mut self) {
        drop(self.surfaces.take());
        drop(self.renderer.take());
    }
}

pub(super) struct NativeSurfaceRuntime<A: SurfaceAdapter> {
    options: NativeSurfaceOptions,
    resources: Option<SurfaceResources<A::Renderer, A::Surface>>,
}

impl<A: SurfaceAdapter> NativeSurfaceRuntime<A> {
    pub(super) fn new(options: NativeSurfaceOptions) -> Self {
        Self {
            options,
            resources: None,
        }
    }

    pub(super) fn build(&mut self, plan: SurfaceBuildPlan<'_>) -> windows::core::Result<()> {
        Self::validate_plan(&plan)?;
        self.resources = None;
        for attempt in 0..2 {
            match Self::build_once(self.options, &plan) {
                Ok(resources) => {
                    self.resources = Some(resources);
                    return Ok(());
                }
                Err(error) if attempt == 0 && is_recoverable_hresult(error.code()) => {}
                Err(error) => return Err(error),
            }
        }
        unreachable!("the bounded retry loop always returns on its final attempt")
    }

    #[cfg(test)]
    pub(super) fn is_ready(&self) -> bool {
        self.resources.is_some()
    }

    pub(super) fn device_kind(&self) -> Option<DeviceKind> {
        self.resources
            .as_ref()
            .and_then(|resources| resources.renderer.as_ref())
            .map(A::device_kind)
    }

    pub(super) fn metrics(&self, role: ShowcaseRole) -> Option<SurfaceMetrics> {
        self.resources
            .as_ref()
            .and_then(|resources| resources.surfaces.as_ref())
            .map(|surfaces| A::metrics(surfaces.surface(role)))
    }

    pub(super) fn size(&self, role: ShowcaseRole) -> Option<(u32, u32)> {
        self.metrics(role).map(|metrics| metrics.size())
    }

    pub(super) fn redraw(
        &self,
        role: ShowcaseRole,
        scenes: ShellScenes<'_>,
    ) -> windows::core::Result<SurfaceUpdate> {
        let resources = self.resources.as_ref().ok_or_else(runtime_not_ready)?;
        let renderer = resources.renderer.as_ref().ok_or_else(runtime_not_ready)?;
        let surfaces = resources.surfaces.as_ref().ok_or_else(runtime_not_ready)?;
        normalize_present_result(A::redraw_surface(
            renderer,
            surfaces.surface(role),
            role,
            scenes,
        ))
    }

    pub(super) fn resize(
        &mut self,
        role: ShowcaseRole,
        metrics: SurfaceMetrics,
        scenes: ShellScenes<'_>,
    ) -> windows::core::Result<SurfaceUpdate> {
        let resources = self.resources.as_mut().ok_or_else(runtime_not_ready)?;
        let SurfaceResources { surfaces, renderer } = resources;
        let renderer = renderer.as_ref().ok_or_else(runtime_not_ready)?;
        let surfaces = surfaces.as_mut().ok_or_else(runtime_not_ready)?;
        normalize_present_result(A::resize_surface(
            renderer,
            surfaces.surface_mut(role),
            metrics,
            role,
            scenes,
        ))
    }

    pub(super) fn set_opacity(
        &self,
        role: ShowcaseRole,
        opacity: f32,
    ) -> windows::core::Result<SurfaceUpdate> {
        let resources = self.resources.as_ref().ok_or_else(runtime_not_ready)?;
        let surfaces = resources.surfaces.as_ref().ok_or_else(runtime_not_ready)?;
        normalize_unit_result(A::set_opacity(surfaces.surface(role), opacity))
    }

    pub(super) fn animate_entrance(
        &self,
        role: ShowcaseRole,
        reduced_motion: bool,
    ) -> windows::core::Result<SurfaceUpdate> {
        let resources = self.resources.as_ref().ok_or_else(runtime_not_ready)?;
        let surfaces = resources.surfaces.as_ref().ok_or_else(runtime_not_ready)?;
        normalize_unit_result(A::animate_entrance(surfaces.surface(role), reduced_motion))
    }

    pub(super) fn animate_visibility(
        &self,
        role: ShowcaseRole,
        spec: SurfaceVisibilityAnimation,
    ) -> windows::core::Result<SurfaceUpdate> {
        let resources = self.resources.as_ref().ok_or_else(runtime_not_ready)?;
        let surfaces = resources.surfaces.as_ref().ok_or_else(runtime_not_ready)?;
        normalize_unit_result(A::animate_visibility(surfaces.surface(role), spec))
    }

    pub(super) fn set_visibility_state(
        &self,
        role: ShowcaseRole,
        offset_y: f32,
        opacity: f32,
    ) -> windows::core::Result<SurfaceUpdate> {
        let resources = self.resources.as_ref().ok_or_else(runtime_not_ready)?;
        let surfaces = resources.surfaces.as_ref().ok_or_else(runtime_not_ready)?;
        normalize_unit_result(A::set_visibility_state(
            surfaces.surface(role),
            offset_y,
            opacity,
        ))
    }

    pub(super) fn rebuild(&mut self, plan: SurfaceBuildPlan<'_>) -> windows::core::Result<()> {
        self.build(plan)
    }

    fn build_once(
        options: NativeSurfaceOptions,
        plan: &SurfaceBuildPlan<'_>,
    ) -> windows::core::Result<SurfaceResources<A::Renderer, A::Surface>> {
        let renderer = A::create_renderer(options)?;
        let topbar = Self::create_surface(&renderer, plan.topbar)?;
        let dock = Self::create_surface(&renderer, plan.dock)?;
        let popover = Self::create_surface(&renderer, plan.popover)?;
        let app_menu = Self::create_surface(&renderer, plan.app_menu)?;
        let preview = Self::create_surface(&renderer, plan.preview)?;
        let settings = Self::create_surface(&renderer, plan.settings)?;
        A::set_visibility_state(
            &dock,
            plan.dock_visibility_offset_y,
            plan.dock_visibility_opacity,
        )?;
        if let Some(animation) = plan.dock_visibility_animation {
            A::animate_visibility(&dock, animation)?;
        }
        A::set_opacity(&preview, plan.preview_opacity)?;
        let surfaces = SurfaceSet {
            topbar,
            dock,
            popover,
            app_menu,
            preview,
            settings,
        };
        Ok(SurfaceResources::new(renderer, surfaces))
    }

    fn validate_plan(plan: &SurfaceBuildPlan<'_>) -> windows::core::Result<()> {
        for (frame, expected) in [
            (plan.topbar, ShowcaseRole::Topbar),
            (plan.dock, ShowcaseRole::Dock),
            (plan.popover, ShowcaseRole::Popover),
            (plan.app_menu, ShowcaseRole::AppMenu),
            (plan.preview, ShowcaseRole::Preview),
            (plan.settings, ShowcaseRole::Settings),
        ] {
            if frame.target.role != expected {
                return Err(windows::core::Error::new(
                    E_INVALIDARG,
                    format!(
                        "surface build plan slot {expected:?} contains role {:?}",
                        frame.target.role
                    ),
                ));
            }
        }
        Ok(())
    }

    fn create_surface(
        renderer: &A::Renderer,
        frame: SurfaceFrame<'_>,
    ) -> windows::core::Result<A::Surface> {
        A::create_surface(
            renderer,
            frame.target.hwnd,
            frame.target.role,
            frame.target.metrics,
            frame.scenes,
        )
    }
}

pub(super) fn runtime_device_kind<A: SurfaceAdapter>(
    runtime: &NativeSurfaceRuntime<A>,
) -> DeviceKind {
    runtime.device_kind().unwrap_or(DeviceKind::Warp)
}

pub(super) fn present_frame<A: SurfaceAdapter>(
    runtime: &mut NativeSurfaceRuntime<A>,
    frame: SurfaceFrame<'_>,
    size_change: SurfaceSizeChange,
) -> windows::core::Result<SurfaceUpdate> {
    let role = frame.target.role;
    match surface_render_decision(runtime.metrics(role), frame.target.metrics) {
        SurfaceRenderDecision::RebuildAll => Ok(SurfaceUpdate::RebuildAllRequired),
        SurfaceRenderDecision::Redraw => runtime.redraw(role, frame.scenes),
        SurfaceRenderDecision::Resize => match size_change {
            SurfaceSizeChange::Resize => runtime.resize(role, frame.target.metrics, frame.scenes),
        },
    }
}

pub(super) fn present_app_menu_frame<A: SurfaceAdapter>(
    runtime: &mut NativeSurfaceRuntime<A>,
    frame: SurfaceFrame<'_>,
) -> windows::core::Result<SurfaceUpdate> {
    debug_assert_eq!(frame.target.role, ShowcaseRole::AppMenu);
    present_frame(runtime, frame, SurfaceSizeChange::Resize)
}

fn runtime_not_ready() -> windows::core::Error {
    windows::core::Error::new(E_UNEXPECTED, "native surface runtime is not ready")
}

pub(super) type Win32NativeSurfaceRuntime = NativeSurfaceRuntime<DirectCompositionAdapter>;

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use shell_core::DockItemId;
    use shell_renderer::Dpi;
    use shell_renderer::WindowPreviewScene;
    use shell_renderer::native::{
        DeviceKind, DeviceLossKind, PresentOutcome, ShellScenes, ShowcaseRole, SurfaceMetrics,
        SurfaceVisibilityAnimation, device_loss_hresult,
    };
    use shell_renderer::{ContextMenuEntry, ContextMenuScene};
    use windows::Win32::Foundation::HWND;
    use windows::core::HRESULT;

    use super::{
        NativeSurfaceOptions, NativeSurfaceRuntime, SurfaceAdapter, SurfaceBuildPlan, SurfaceFrame,
        SurfaceRenderDecision, SurfaceSizeChange, SurfaceTarget, SurfaceUpdate,
        normalize_present_result, present_app_menu_frame, present_frame, runtime_device_kind,
        surface_render_decision,
    };

    #[test]
    fn surface_render_decision_resizes_when_either_physical_dimension_changes() {
        let desired = SurfaceMetrics::new(387, 55, Dpi::from_raw(144));
        assert_eq!(
            surface_render_decision(
                Some(SurfaceMetrics::new(328, 55, Dpi::from_raw(144))),
                desired,
            ),
            SurfaceRenderDecision::Resize
        );
        assert_eq!(
            surface_render_decision(
                Some(SurfaceMetrics::new(387, 54, Dpi::from_raw(144))),
                desired,
            ),
            SurfaceRenderDecision::Resize
        );
        assert_eq!(
            surface_render_decision(Some(desired), desired),
            SurfaceRenderDecision::Redraw
        );
    }

    fn recoverable_failure() -> HRESULT {
        device_loss_hresult(DeviceLossKind::Removed)
    }

    const fn unrecoverable_failure() -> HRESULT {
        HRESULT(0x8000_4005_u32 as i32)
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Event {
        Attempt(usize),
        Created(usize, ShowcaseRole),
        MetricsRead(usize, ShowcaseRole),
        Opacity(usize, ShowcaseRole, f32),
        Redrawn(usize, ShowcaseRole, ShowcaseRole),
        Resized(usize, ShowcaseRole, ShowcaseRole),
        EntranceAnimated(usize, ShowcaseRole, bool),
        VisibilityAnimated(usize, ShowcaseRole, SurfaceVisibilityAnimation),
        VisibilityState(usize, ShowcaseRole, f32, f32),
        SurfaceDropped(usize, ShowcaseRole),
        RendererDropped(usize),
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum UnitOperation {
        Opacity,
        Entrance,
        Visibility,
        VisibilityState,
    }

    #[derive(Default)]
    struct RecordingState {
        events: Vec<Event>,
        failures: VecDeque<(ShowcaseRole, HRESULT)>,
        redraw_results: VecDeque<windows::core::Result<PresentOutcome>>,
        resize_results: VecDeque<windows::core::Result<PresentOutcome>>,
        attempts: usize,
        scene_records: Vec<(ShowcaseRole, ScenePresence)>,
        create_records: Vec<CreateRecord>,
        device_kind: Option<DeviceKind>,
        unit_failures: VecDeque<(UnitOperation, HRESULT)>,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct CreateRecord {
        hwnd: usize,
        role: ShowcaseRole,
        metrics: (u32, u32),
        preview_item: Option<u64>,
    }

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    struct ScenePresence {
        topbar: bool,
        dock: bool,
        popover: bool,
        context_menu: bool,
        settings: bool,
        preview: bool,
    }

    impl ScenePresence {
        const fn from_scenes(scenes: ShellScenes<'_>) -> Self {
            Self {
                topbar: scenes.topbar.is_some(),
                dock: scenes.dock.is_some(),
                popover: scenes.popover.is_some(),
                context_menu: scenes.context_menu.is_some(),
                settings: scenes.settings.is_some(),
                preview: scenes.preview.is_some(),
            }
        }
    }

    static TEST_LOCK: Mutex<()> = Mutex::new(());
    static RECORDING: Mutex<RecordingState> = Mutex::new(RecordingState {
        events: Vec::new(),
        failures: VecDeque::new(),
        redraw_results: VecDeque::new(),
        resize_results: VecDeque::new(),
        attempts: 0,
        scene_records: Vec::new(),
        create_records: Vec::new(),
        device_kind: None,
        unit_failures: VecDeque::new(),
    });

    fn recording() -> std::sync::MutexGuard<'static, RecordingState> {
        RECORDING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn reset(failures: impl IntoIterator<Item = (ShowcaseRole, HRESULT)>) {
        *recording() = RecordingState {
            failures: failures.into_iter().collect(),
            ..RecordingState::default()
        };
    }

    fn events() -> Vec<Event> {
        recording().events.clone()
    }

    struct RecordingRenderer {
        attempt: usize,
    }

    impl Drop for RecordingRenderer {
        fn drop(&mut self) {
            recording()
                .events
                .push(Event::RendererDropped(self.attempt));
        }
    }

    struct RecordingSurface {
        attempt: usize,
        role: ShowcaseRole,
        metrics: SurfaceMetrics,
    }

    impl Drop for RecordingSurface {
        fn drop(&mut self) {
            recording()
                .events
                .push(Event::SurfaceDropped(self.attempt, self.role));
        }
    }

    struct RecordingAdapter;

    impl SurfaceAdapter for RecordingAdapter {
        type Renderer = RecordingRenderer;
        type Surface = RecordingSurface;

        fn create_renderer(options: NativeSurfaceOptions) -> windows::core::Result<Self::Renderer> {
            let _ = (
                options.force_warp,
                options.solid_material,
                options.liquid_glass,
                options.reduced_motion,
            );
            let mut state = recording();
            state.attempts += 1;
            let attempt = state.attempts;
            state.events.push(Event::Attempt(attempt));
            Ok(RecordingRenderer { attempt })
        }

        fn device_kind(_renderer: &Self::Renderer) -> DeviceKind {
            recording().device_kind.unwrap_or(DeviceKind::Hardware)
        }

        fn create_surface(
            renderer: &Self::Renderer,
            hwnd: HWND,
            role: ShowcaseRole,
            metrics: SurfaceMetrics,
            scenes: ShellScenes<'_>,
        ) -> windows::core::Result<Self::Surface> {
            let mut state = recording();
            if state
                .failures
                .front()
                .is_some_and(|(failed_role, _)| *failed_role == role)
            {
                let (_, code) = state.failures.pop_front().expect("front was present");
                return Err(windows::core::Error::from_hresult(code));
            }
            state
                .scene_records
                .push((role, ScenePresence::from_scenes(scenes)));
            state.create_records.push(CreateRecord {
                hwnd: hwnd.0 as usize,
                role,
                metrics: metrics.size(),
                preview_item: scenes.preview.map(|scene| scene.item().value()),
            });
            state.events.push(Event::Created(renderer.attempt, role));
            Ok(RecordingSurface {
                attempt: renderer.attempt,
                role,
                metrics,
            })
        }

        fn redraw_surface(
            _renderer: &Self::Renderer,
            surface: &Self::Surface,
            role: ShowcaseRole,
            scenes: ShellScenes<'_>,
        ) -> windows::core::Result<PresentOutcome> {
            let mut state = recording();
            state
                .scene_records
                .push((role, ScenePresence::from_scenes(scenes)));
            state
                .events
                .push(Event::Redrawn(surface.attempt, surface.role, role));
            state
                .redraw_results
                .pop_front()
                .unwrap_or(Ok(PresentOutcome::Presented))
        }

        fn resize_surface(
            _renderer: &Self::Renderer,
            surface: &mut Self::Surface,
            metrics: SurfaceMetrics,
            role: ShowcaseRole,
            scenes: ShellScenes<'_>,
        ) -> windows::core::Result<PresentOutcome> {
            let mut state = recording();
            state
                .scene_records
                .push((role, ScenePresence::from_scenes(scenes)));
            state
                .events
                .push(Event::Resized(surface.attempt, surface.role, role));
            surface.metrics = metrics;
            state
                .resize_results
                .pop_front()
                .unwrap_or(Ok(PresentOutcome::Presented))
        }

        fn metrics(surface: &Self::Surface) -> SurfaceMetrics {
            recording()
                .events
                .push(Event::MetricsRead(surface.attempt, surface.role));
            surface.metrics
        }

        fn set_opacity(surface: &Self::Surface, opacity: f32) -> windows::core::Result<()> {
            let mut state = recording();
            state
                .events
                .push(Event::Opacity(surface.attempt, surface.role, opacity));
            if state
                .unit_failures
                .front()
                .is_some_and(|(operation, _)| *operation == UnitOperation::Opacity)
            {
                let (_, code) = state.unit_failures.pop_front().expect("front was present");
                return Err(windows::core::Error::from_hresult(code));
            }
            Ok(())
        }

        fn animate_entrance(
            surface: &Self::Surface,
            reduced_motion: bool,
        ) -> windows::core::Result<()> {
            let mut state = recording();
            state.events.push(Event::EntranceAnimated(
                surface.attempt,
                surface.role,
                reduced_motion,
            ));
            if state
                .unit_failures
                .front()
                .is_some_and(|(operation, _)| *operation == UnitOperation::Entrance)
            {
                let (_, code) = state.unit_failures.pop_front().expect("front was present");
                return Err(windows::core::Error::from_hresult(code));
            }
            Ok(())
        }

        fn animate_visibility(
            surface: &Self::Surface,
            spec: SurfaceVisibilityAnimation,
        ) -> windows::core::Result<()> {
            let mut state = recording();
            state.events.push(Event::VisibilityAnimated(
                surface.attempt,
                surface.role,
                spec,
            ));
            if state
                .unit_failures
                .front()
                .is_some_and(|(operation, _)| *operation == UnitOperation::Visibility)
            {
                let (_, code) = state.unit_failures.pop_front().expect("front was present");
                return Err(windows::core::Error::from_hresult(code));
            }
            Ok(())
        }

        fn set_visibility_state(
            surface: &Self::Surface,
            offset_y: f32,
            opacity: f32,
        ) -> windows::core::Result<()> {
            let mut state = recording();
            state.events.push(Event::VisibilityState(
                surface.attempt,
                surface.role,
                offset_y,
                opacity,
            ));
            if state
                .unit_failures
                .front()
                .is_some_and(|(operation, _)| *operation == UnitOperation::VisibilityState)
            {
                let (_, code) = state.unit_failures.pop_front().expect("front was present");
                return Err(windows::core::Error::from_hresult(code));
            }
            Ok(())
        }
    }

    fn empty_scenes() -> ShellScenes<'static> {
        ShellScenes {
            topbar: None,
            dock: None,
            popover: None,
            quick_settings: None,
            context_menu: None,
            settings: None,
            preview: None,
        }
    }

    fn frame(role: ShowcaseRole) -> SurfaceFrame<'static> {
        let (width, height) = role_size(role);
        SurfaceFrame {
            target: SurfaceTarget {
                hwnd: HWND::default(),
                role,
                metrics: SurfaceMetrics::new(width, height, Dpi::from_raw(96)),
            },
            scenes: empty_scenes(),
        }
    }

    fn plan() -> SurfaceBuildPlan<'static> {
        SurfaceBuildPlan {
            topbar: frame(ShowcaseRole::Topbar),
            dock: frame(ShowcaseRole::Dock),
            popover: frame(ShowcaseRole::Popover),
            app_menu: frame(ShowcaseRole::AppMenu),
            preview: frame(ShowcaseRole::Preview),
            settings: frame(ShowcaseRole::Settings),
            dock_visibility_offset_y: 0.0,
            dock_visibility_opacity: 0.42,
            dock_visibility_animation: None,
            preview_opacity: 1.0,
        }
    }

    fn runtime() -> NativeSurfaceRuntime<RecordingAdapter> {
        NativeSurfaceRuntime::new(NativeSurfaceOptions {
            force_warp: false,
            solid_material: false,
            liquid_glass: false,
            reduced_motion: false,
        })
    }

    const fn role_size(role: ShowcaseRole) -> (u32, u32) {
        match role {
            ShowcaseRole::Topbar => (1, 11),
            ShowcaseRole::Dock => (2, 12),
            ShowcaseRole::Popover => (3, 13),
            ShowcaseRole::AppMenu => (4, 14),
            ShowcaseRole::Preview => (5, 15),
            ShowcaseRole::Settings => (6, 16),
        }
    }

    const ROLES: [ShowcaseRole; 6] = [
        ShowcaseRole::Topbar,
        ShowcaseRole::Dock,
        ShowcaseRole::Popover,
        ShowcaseRole::AppMenu,
        ShowcaseRole::Preview,
        ShowcaseRole::Settings,
    ];

    const fn supported_size_change(change: SurfaceSizeChange) -> &'static str {
        match change {
            SurfaceSizeChange::Resize => "resize",
        }
    }

    #[test]
    fn surface_size_change_supports_only_resize() {
        assert_eq!(supported_size_change(SurfaceSizeChange::Resize), "resize");
    }

    #[test]
    fn presentation_results_have_one_recovery_policy() {
        assert_eq!(
            normalize_present_result(Ok(PresentOutcome::Presented)).unwrap(),
            SurfaceUpdate::Presented
        );
        assert_eq!(
            normalize_present_result(Ok(PresentOutcome::DeviceLost(DeviceLossKind::Reset)))
                .unwrap(),
            SurfaceUpdate::RebuildAllRequired
        );
        assert_eq!(
            normalize_present_result(Ok(PresentOutcome::Failed(device_loss_hresult(
                DeviceLossKind::Removed,
            ))))
            .unwrap(),
            SurfaceUpdate::RebuildAllRequired
        );

        let unrecoverable = HRESULT(0x8000_4005_u32 as i32);
        let error =
            normalize_present_result(Ok(PresentOutcome::Failed(unrecoverable))).unwrap_err();
        assert_eq!(error.code(), unrecoverable);
        assert_eq!(
            normalize_present_result(Err(windows::core::Error::from_hresult(
                recoverable_failure(),
            )))
            .unwrap(),
            SurfaceUpdate::RebuildAllRequired
        );
        let error =
            normalize_present_result(Err(windows::core::Error::from_hresult(unrecoverable)))
                .unwrap_err();
        assert_eq!(error.code(), unrecoverable);
    }

    #[test]
    fn runtime_routes_every_surface_operation_to_only_the_requested_role() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let visibility = SurfaceVisibilityAnimation {
            start_offset_y: 8.0,
            target_offset_y: 0.0,
            start_opacity: 0.25,
            target_opacity: 1.0,
            duration_seconds: 0.18,
        };

        for role in ROLES {
            reset([]);
            let mut runtime = runtime();
            runtime.build(plan()).unwrap();
            recording().events.clear();

            assert_eq!(runtime.size(role), Some(role_size(role)));
            assert_eq!(
                runtime.redraw(role, empty_scenes()).unwrap(),
                SurfaceUpdate::Presented
            );
            assert_eq!(
                runtime
                    .resize(
                        role,
                        SurfaceMetrics::new(21, 22, Dpi::from_raw(144)),
                        empty_scenes(),
                    )
                    .unwrap(),
                SurfaceUpdate::Presented
            );
            assert_eq!(runtime.size(role), Some((21, 22)));
            runtime.set_opacity(role, 0.7).unwrap();
            runtime.animate_entrance(role, true).unwrap();
            runtime.animate_visibility(role, visibility).unwrap();
            runtime.set_visibility_state(role, 6.0, 0.35).unwrap();

            assert_eq!(
                events(),
                [
                    Event::MetricsRead(1, role),
                    Event::Redrawn(1, role, role),
                    Event::Resized(1, role, role),
                    Event::MetricsRead(1, role),
                    Event::Opacity(1, role, 0.7),
                    Event::EntranceAnimated(1, role, true),
                    Event::VisibilityAnimated(1, role, visibility),
                    Event::VisibilityState(1, role, 6.0, 0.35),
                ],
                "operation routing mismatch for {role:?}"
            );
        }
    }

    #[test]
    fn dock_present_frame_executes_missing_redraw_and_resize_on_dock_only() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset([]);
        let mut unavailable = runtime();
        assert_eq!(
            present_frame(
                &mut unavailable,
                frame(ShowcaseRole::Dock),
                SurfaceSizeChange::Resize,
            )
            .unwrap(),
            SurfaceUpdate::RebuildAllRequired
        );
        assert!(events().is_empty());

        reset([]);
        let mut runtime = runtime();
        runtime.build(plan()).unwrap();
        recording().events.clear();
        present_frame(
            &mut runtime,
            frame(ShowcaseRole::Dock),
            SurfaceSizeChange::Resize,
        )
        .unwrap();
        let mut changed = frame(ShowcaseRole::Dock);
        changed.target.metrics = SurfaceMetrics::new(88, 89, Dpi::from_raw(144));
        present_frame(&mut runtime, changed, SurfaceSizeChange::Resize).unwrap();

        assert_eq!(
            events(),
            [
                Event::MetricsRead(1, ShowcaseRole::Dock),
                Event::Redrawn(1, ShowcaseRole::Dock, ShowcaseRole::Dock),
                Event::MetricsRead(1, ShowcaseRole::Dock),
                Event::Resized(1, ShowcaseRole::Dock, ShowcaseRole::Dock),
            ]
        );
    }

    #[test]
    fn app_menu_open_resizes_surface_before_present() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset([]);
        let mut runtime = runtime();
        let mut initial_plan = plan();
        initial_plan.app_menu.target.metrics = SurfaceMetrics::new(300, 1, Dpi::from_raw(144));
        runtime.build(initial_plan).unwrap();
        recording().events.clear();
        let mut menu = frame(ShowcaseRole::AppMenu);
        menu.target.metrics = SurfaceMetrics::new(300, 224, Dpi::from_raw(144));

        assert_eq!(
            present_app_menu_frame(&mut runtime, menu).unwrap(),
            SurfaceUpdate::Presented
        );
        assert_eq!(
            runtime.metrics(ShowcaseRole::AppMenu),
            Some(menu.target.metrics)
        );
        assert_eq!(
            events(),
            [
                Event::MetricsRead(1, ShowcaseRole::AppMenu),
                Event::Resized(1, ShowcaseRole::AppMenu, ShowcaseRole::AppMenu),
                Event::MetricsRead(1, ShowcaseRole::AppMenu),
            ]
        );
    }

    #[test]
    fn settings_present_frame_executes_redraw_and_resize_on_settings_only() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset([]);
        let mut unavailable = runtime();
        assert_eq!(
            present_frame(
                &mut unavailable,
                frame(ShowcaseRole::Settings),
                SurfaceSizeChange::Resize,
            )
            .unwrap(),
            SurfaceUpdate::RebuildAllRequired
        );
        assert!(events().is_empty());

        reset([]);
        let mut runtime = runtime();
        runtime.build(plan()).unwrap();
        recording().events.clear();

        present_frame(
            &mut runtime,
            frame(ShowcaseRole::Settings),
            SurfaceSizeChange::Resize,
        )
        .unwrap();
        let mut changed = frame(ShowcaseRole::Settings);
        changed.target.metrics = SurfaceMetrics::new(98, 99, Dpi::from_raw(144));
        present_frame(&mut runtime, changed, SurfaceSizeChange::Resize).unwrap();

        assert_eq!(
            events(),
            [
                Event::MetricsRead(1, ShowcaseRole::Settings),
                Event::Redrawn(1, ShowcaseRole::Settings, ShowcaseRole::Settings),
                Event::MetricsRead(1, ShowcaseRole::Settings),
                Event::Resized(1, ShowcaseRole::Settings, ShowcaseRole::Settings),
            ]
        );
    }

    #[test]
    fn present_frame_resizes_when_only_dpi_changes() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset([]);
        let mut runtime = runtime();
        runtime.build(plan()).unwrap();
        recording().events.clear();
        let mut changed = frame(ShowcaseRole::Settings);
        changed.target.metrics = SurfaceMetrics::new(5, 15, Dpi::from_raw(144));

        assert_eq!(
            present_frame(&mut runtime, changed, SurfaceSizeChange::Resize).unwrap(),
            SurfaceUpdate::Presented
        );
        assert_eq!(
            runtime.metrics(ShowcaseRole::Settings),
            Some(changed.target.metrics)
        );
        assert_eq!(
            events(),
            [
                Event::MetricsRead(1, ShowcaseRole::Settings),
                Event::Resized(1, ShowcaseRole::Settings, ShowcaseRole::Settings),
                Event::MetricsRead(1, ShowcaseRole::Settings),
            ]
        );
    }

    #[test]
    fn popover_present_frame_redraws_or_resizes_shared_context_surface() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset([]);
        let mut runtime = runtime();
        runtime.build(plan()).unwrap();
        recording().events.clear();
        recording().scene_records.clear();
        let context_menu =
            ContextMenuScene::new(vec![ContextMenuEntry::action("Open", true)], Some(0));
        let scenes = ShellScenes {
            context_menu: Some(&context_menu),
            ..empty_scenes()
        };
        let mut popover = frame(ShowcaseRole::Popover);
        popover.scenes = scenes;

        present_frame(&mut runtime, popover, SurfaceSizeChange::Resize).unwrap();
        popover.target.metrics = SurfaceMetrics::new(108, 109, Dpi::from_raw(144));
        present_frame(&mut runtime, popover, SurfaceSizeChange::Resize).unwrap();

        assert!(events().contains(&Event::Redrawn(
            1,
            ShowcaseRole::Popover,
            ShowcaseRole::Popover,
        )));
        assert!(events().contains(&Event::Resized(
            1,
            ShowcaseRole::Popover,
            ShowcaseRole::Popover,
        )));
        assert!(!events().iter().any(|event| matches!(
            event,
            Event::Created(_, ShowcaseRole::Popover)
                | Event::SurfaceDropped(_, ShowcaseRole::Popover)
        )));
        assert_eq!(
            recording().scene_records,
            [
                (
                    ShowcaseRole::Popover,
                    ScenePresence {
                        context_menu: true,
                        ..ScenePresence::default()
                    }
                ),
                (
                    ShowcaseRole::Popover,
                    ScenePresence {
                        context_menu: true,
                        ..ScenePresence::default()
                    }
                ),
            ]
        );
    }

    #[test]
    fn preview_present_frame_preserves_role_and_recoverable_update_signal() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset([]);
        let mut unavailable = runtime();
        assert_eq!(
            present_frame(
                &mut unavailable,
                frame(ShowcaseRole::Preview),
                SurfaceSizeChange::Resize,
            )
            .unwrap(),
            SurfaceUpdate::RebuildAllRequired
        );
        assert!(events().is_empty());

        reset([]);
        let mut runtime = runtime();
        runtime.build(plan()).unwrap();
        recording().events.clear();
        recording()
            .redraw_results
            .push_back(Ok(PresentOutcome::Failed(recoverable_failure())));

        assert_eq!(
            present_frame(
                &mut runtime,
                frame(ShowcaseRole::Preview),
                SurfaceSizeChange::Resize,
            )
            .unwrap(),
            SurfaceUpdate::RebuildAllRequired
        );
        assert_eq!(
            events(),
            [
                Event::MetricsRead(1, ShowcaseRole::Preview),
                Event::Redrawn(1, ShowcaseRole::Preview, ShowcaseRole::Preview),
            ]
        );

        recording().events.clear();
        let mut changed = frame(ShowcaseRole::Preview);
        changed.target.metrics = SurfaceMetrics::new(108, 109, Dpi::from_raw(144));
        assert_eq!(
            present_frame(&mut runtime, changed, SurfaceSizeChange::Resize).unwrap(),
            SurfaceUpdate::Presented
        );
        assert_eq!(
            events(),
            [
                Event::MetricsRead(1, ShowcaseRole::Preview),
                Event::Resized(1, ShowcaseRole::Preview, ShowcaseRole::Preview),
            ]
        );
    }

    #[test]
    fn redraw_and_resize_normalize_every_presentation_outcome() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset([]);
        let mut runtime = runtime();
        runtime.build(plan()).unwrap();
        let cases = [
            (PresentOutcome::Presented, Ok(SurfaceUpdate::Presented)),
            (
                PresentOutcome::DeviceLost(DeviceLossKind::Reset),
                Ok(SurfaceUpdate::RebuildAllRequired),
            ),
            (
                PresentOutcome::Failed(recoverable_failure()),
                Ok(SurfaceUpdate::RebuildAllRequired),
            ),
            (PresentOutcome::Failed(unrecoverable_failure()), Err(())),
        ];

        for (outcome, expected) in cases {
            recording().redraw_results.push_back(Ok(outcome));
            let result = runtime.redraw(ShowcaseRole::Dock, empty_scenes());
            match expected {
                Ok(update) => assert_eq!(result.unwrap(), update),
                Err(()) => assert_eq!(result.unwrap_err().code(), unrecoverable_failure()),
            }

            recording().resize_results.push_back(Ok(outcome));
            let result = runtime.resize(
                ShowcaseRole::Dock,
                SurfaceMetrics::new(21, 22, Dpi::from_raw(144)),
                empty_scenes(),
            );
            match expected {
                Ok(update) => assert_eq!(result.unwrap(), update),
                Err(()) => assert_eq!(result.unwrap_err().code(), unrecoverable_failure()),
            }
        }

        recording()
            .redraw_results
            .push_back(Err(windows::core::Error::from_hresult(
                recoverable_failure(),
            )));
        assert_eq!(
            runtime.redraw(ShowcaseRole::Dock, empty_scenes()).unwrap(),
            SurfaceUpdate::RebuildAllRequired
        );
        recording()
            .resize_results
            .push_back(Err(windows::core::Error::from_hresult(
                recoverable_failure(),
            )));
        assert_eq!(
            runtime
                .resize(
                    ShowcaseRole::Dock,
                    SurfaceMetrics::new(21, 22, Dpi::from_raw(144)),
                    empty_scenes(),
                )
                .unwrap(),
            SurfaceUpdate::RebuildAllRequired
        );

        recording()
            .redraw_results
            .push_back(Err(windows::core::Error::from_hresult(
                unrecoverable_failure(),
            )));
        assert_eq!(
            runtime
                .redraw(ShowcaseRole::Dock, empty_scenes())
                .unwrap_err()
                .code(),
            unrecoverable_failure()
        );
        recording()
            .resize_results
            .push_back(Err(windows::core::Error::from_hresult(
                unrecoverable_failure(),
            )));
        assert_eq!(
            runtime
                .resize(
                    ShowcaseRole::Dock,
                    SurfaceMetrics::new(21, 22, Dpi::from_raw(144)),
                    empty_scenes(),
                )
                .unwrap_err()
                .code(),
            unrecoverable_failure()
        );
    }

    #[test]
    fn unit_operations_normalize_recoverable_and_fatal_failures_for_the_requested_role() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset([]);
        let mut runtime = runtime();
        runtime.build(plan()).unwrap();
        recording().events.clear();
        let visibility = SurfaceVisibilityAnimation {
            start_offset_y: 8.0,
            target_offset_y: 0.0,
            start_opacity: 0.25,
            target_opacity: 1.0,
            duration_seconds: 0.18,
        };

        recording()
            .unit_failures
            .push_back((UnitOperation::Opacity, recoverable_failure()));
        assert_eq!(
            runtime.set_opacity(ShowcaseRole::Preview, 0.5).unwrap(),
            SurfaceUpdate::RebuildAllRequired
        );
        recording()
            .unit_failures
            .push_back((UnitOperation::Entrance, unrecoverable_failure()));
        assert_eq!(
            runtime
                .animate_entrance(ShowcaseRole::Popover, false)
                .unwrap_err()
                .code(),
            unrecoverable_failure()
        );
        recording()
            .unit_failures
            .push_back((UnitOperation::Visibility, recoverable_failure()));
        assert_eq!(
            runtime
                .animate_visibility(ShowcaseRole::Dock, visibility)
                .unwrap(),
            SurfaceUpdate::RebuildAllRequired
        );
        recording()
            .unit_failures
            .push_back((UnitOperation::VisibilityState, recoverable_failure()));
        assert_eq!(
            runtime
                .set_visibility_state(ShowcaseRole::Dock, 6.0, 0.35)
                .unwrap(),
            SurfaceUpdate::RebuildAllRequired
        );
        assert!(events().contains(&Event::Opacity(1, ShowcaseRole::Preview, 0.5)));
        assert!(events().contains(&Event::EntranceAnimated(1, ShowcaseRole::Popover, false,)));
        assert!(events().contains(&Event::VisibilityAnimated(
            1,
            ShowcaseRole::Dock,
            visibility,
        )));
        assert!(events().contains(&Event::VisibilityState(1, ShowcaseRole::Dock, 6.0, 0.35,)));
    }

    #[test]
    fn build_rejects_a_surface_in_the_wrong_canonical_slot() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset([]);
        let mut runtime = runtime();
        let mut invalid_plan = plan();
        invalid_plan.topbar.target.role = ShowcaseRole::Dock;

        let error = runtime.build(invalid_plan).unwrap_err();

        assert_eq!(error.code(), windows::Win32::Foundation::E_INVALIDARG);
        assert!(!runtime.is_ready());
        assert!(events().is_empty());
    }

    #[test]
    fn invalid_rebuild_preserves_installed_resources_until_runtime_drop() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset([]);
        let mut runtime = runtime();
        runtime.build(plan()).unwrap();
        let events_before_invalid_rebuild = events();
        let mut invalid_plan = plan();
        invalid_plan.topbar.target.role = ShowcaseRole::Dock;

        let error = runtime.rebuild(invalid_plan).unwrap_err();

        assert_eq!(error.code(), windows::Win32::Foundation::E_INVALIDARG);
        assert!(runtime.is_ready());
        assert_eq!(events(), events_before_invalid_rebuild);

        drop(runtime);
        let dropped = events();
        assert!(dropped.contains(&Event::SurfaceDropped(1, ShowcaseRole::Topbar)));
        assert!(dropped.contains(&Event::RendererDropped(1)));
    }

    #[test]
    fn runtime_reports_not_ready_without_surface_resources() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset([]);
        let mut runtime = runtime();
        let visibility = SurfaceVisibilityAnimation {
            start_offset_y: 8.0,
            target_offset_y: 0.0,
            start_opacity: 0.25,
            target_opacity: 1.0,
            duration_seconds: 0.18,
        };

        assert_eq!(runtime.device_kind(), None);
        assert_eq!(runtime.size(ShowcaseRole::Dock), None);
        let errors = [
            runtime
                .redraw(ShowcaseRole::Dock, empty_scenes())
                .unwrap_err(),
            runtime
                .resize(
                    ShowcaseRole::Dock,
                    SurfaceMetrics::new(21, 22, Dpi::from_raw(144)),
                    empty_scenes(),
                )
                .unwrap_err(),
            runtime.set_opacity(ShowcaseRole::Dock, 0.7).unwrap_err(),
            runtime
                .animate_entrance(ShowcaseRole::Dock, true)
                .unwrap_err(),
            runtime
                .animate_visibility(ShowcaseRole::Dock, visibility)
                .unwrap_err(),
            runtime
                .set_visibility_state(ShowcaseRole::Dock, 6.0, 0.35)
                .unwrap_err(),
        ];

        for error in errors {
            assert_eq!(error.code(), windows::Win32::Foundation::E_UNEXPECTED);
            assert!(error.to_string().contains("not ready"));
        }
    }

    #[test]
    fn successful_build_creates_all_surfaces_in_role_order() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset([]);
        let mut runtime = runtime();

        runtime.build(plan()).unwrap();

        let installed = runtime
            .resources
            .as_ref()
            .and_then(|resources| resources.surfaces.as_ref())
            .unwrap();
        assert_eq!(
            [
                installed.topbar.role,
                installed.dock.role,
                installed.popover.role,
                installed.app_menu.role,
                installed.preview.role,
                installed.settings.role,
            ],
            [
                ShowcaseRole::Topbar,
                ShowcaseRole::Dock,
                ShowcaseRole::Popover,
                ShowcaseRole::AppMenu,
                ShowcaseRole::Preview,
                ShowcaseRole::Settings,
            ]
        );
        let created = events()
            .into_iter()
            .filter_map(|event| match event {
                Event::Created(_, role) => Some(role),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            created,
            [
                ShowcaseRole::Topbar,
                ShowcaseRole::Dock,
                ShowcaseRole::Popover,
                ShowcaseRole::AppMenu,
                ShowcaseRole::Preview,
                ShowcaseRole::Settings,
            ]
        );
        assert_eq!(
            recording()
                .scene_records
                .iter()
                .map(|(role, _)| *role)
                .collect::<Vec<_>>(),
            [
                ShowcaseRole::Topbar,
                ShowcaseRole::Dock,
                ShowcaseRole::Popover,
                ShowcaseRole::AppMenu,
                ShowcaseRole::Preview,
                ShowcaseRole::Settings,
            ]
        );
        assert!(runtime.is_ready());
        assert_eq!(runtime.device_kind(), Some(DeviceKind::Hardware));
        assert_eq!(runtime_device_kind(&runtime), DeviceKind::Hardware);
    }

    #[test]
    fn runtime_device_kind_delegates_to_the_recording_adapter() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset([]);
        recording().device_kind = Some(DeviceKind::Warp);
        let mut runtime = runtime();
        runtime.build(plan()).unwrap();

        assert_eq!(runtime_device_kind(&runtime), DeviceKind::Warp);
    }

    #[test]
    fn successive_rebuilds_observe_fresh_preview_scene_generations() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset([]);
        let first_scene = WindowPreviewScene::new(DockItemId::new(301), "First", Vec::new());
        let second_scene = WindowPreviewScene::new(DockItemId::new(302), "Second", Vec::new());
        let mut first = plan();
        first.preview.target.hwnd = HWND(301_usize as *mut std::ffi::c_void);
        first.preview.scenes.preview = Some(&first_scene);
        let mut second = plan();
        second.preview.target.hwnd = HWND(302_usize as *mut std::ffi::c_void);
        second.preview.scenes.preview = Some(&second_scene);
        let mut runtime = runtime();

        runtime.build(first).unwrap();
        runtime.rebuild(second).unwrap();

        let preview_creates = recording()
            .create_records
            .iter()
            .filter(|record| record.role == ShowcaseRole::Preview)
            .map(|record| (record.hwnd, record.preview_item))
            .collect::<Vec<_>>();
        assert_eq!(preview_creates, [(301, Some(301)), (302, Some(302))]);
    }

    #[test]
    fn preview_opacity_must_succeed_before_transaction_becomes_ready() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset([]);
        recording()
            .unit_failures
            .push_back((UnitOperation::Opacity, unrecoverable_failure()));
        let mut runtime = runtime();

        assert!(runtime.build(plan()).is_err());

        assert!(!runtime.is_ready());
        assert!(events().contains(&Event::Opacity(1, ShowcaseRole::Preview, 1.0)));
    }

    #[test]
    fn build_applies_transient_dock_and_preview_state_before_becoming_ready() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset([]);
        let visibility = SurfaceVisibilityAnimation {
            start_offset_y: 48.0,
            target_offset_y: 172.0,
            start_opacity: 0.6,
            target_opacity: 0.0,
            duration_seconds: 0.09,
        };
        let mut transient = plan();
        transient.dock_visibility_offset_y = 48.0;
        transient.dock_visibility_opacity = 0.6;
        transient.dock_visibility_animation = Some(visibility);
        transient.preview_opacity = 0.35;
        let mut runtime = runtime();

        runtime.build(transient).unwrap();

        assert!(runtime.is_ready());
        let events = events();
        let dock_state = events
            .iter()
            .position(|event| *event == Event::VisibilityState(1, ShowcaseRole::Dock, 48.0, 0.6))
            .unwrap();
        let dock_animation = events
            .iter()
            .position(|event| {
                *event == Event::VisibilityAnimated(1, ShowcaseRole::Dock, visibility)
            })
            .unwrap();
        let preview_opacity = events
            .iter()
            .position(|event| *event == Event::Opacity(1, ShowcaseRole::Preview, 0.35))
            .unwrap();
        assert!(dock_state < dock_animation);
        assert!(dock_animation < preview_opacity);
    }

    #[test]
    fn recoverable_transient_state_failure_retries_the_whole_build() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset([]);
        recording()
            .unit_failures
            .push_back((UnitOperation::VisibilityState, recoverable_failure()));
        let mut runtime = runtime();

        runtime.build(plan()).unwrap();

        assert!(runtime.is_ready());
        assert!(events().contains(&Event::VisibilityState(1, ShowcaseRole::Dock, 0.0, 0.42,)));
        assert!(events().contains(&Event::VisibilityState(2, ShowcaseRole::Dock, 0.0, 0.42,)));
    }

    #[test]
    fn fatal_transient_state_failure_leaves_runtime_not_ready() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset([]);
        recording()
            .unit_failures
            .push_back((UnitOperation::Opacity, unrecoverable_failure()));
        let mut runtime = runtime();

        assert_eq!(
            runtime.build(plan()).unwrap_err().code(),
            unrecoverable_failure()
        );
        assert!(!runtime.is_ready());
        assert!(events().contains(&Event::Opacity(1, ShowcaseRole::Preview, 1.0,)));
    }

    #[test]
    fn recording_adapter_records_surface_operations_deterministically() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset([]);
        let renderer = RecordingRenderer { attempt: 7 };
        let mut surface = RecordingSurface {
            attempt: 7,
            role: ShowcaseRole::Preview,
            metrics: SurfaceMetrics::new(1, 1, Dpi::from_raw(96)),
        };
        let visibility = SurfaceVisibilityAnimation {
            start_offset_y: 8.0,
            target_offset_y: 0.0,
            start_opacity: 0.25,
            target_opacity: 1.0,
            duration_seconds: 0.18,
        };

        RecordingAdapter::redraw_surface(
            &renderer,
            &surface,
            ShowcaseRole::Preview,
            empty_scenes(),
        )
        .unwrap();
        RecordingAdapter::resize_surface(
            &renderer,
            &mut surface,
            SurfaceMetrics::new(2, 3, Dpi::from_raw(144)),
            ShowcaseRole::Preview,
            empty_scenes(),
        )
        .unwrap();
        RecordingAdapter::animate_entrance(&surface, false).unwrap();
        RecordingAdapter::animate_visibility(&surface, visibility).unwrap();
        RecordingAdapter::set_visibility_state(&surface, 6.0, 0.35).unwrap();

        assert_eq!(
            events(),
            [
                Event::Redrawn(7, ShowcaseRole::Preview, ShowcaseRole::Preview),
                Event::Resized(7, ShowcaseRole::Preview, ShowcaseRole::Preview),
                Event::EntranceAnimated(7, ShowcaseRole::Preview, false),
                Event::VisibilityAnimated(7, ShowcaseRole::Preview, visibility),
                Event::VisibilityState(7, ShowcaseRole::Preview, 6.0, 0.35),
            ]
        );
        drop(surface);
        drop(renderer);
    }

    #[test]
    fn recoverable_partial_build_drops_surfaces_then_renderer_before_retry() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset([(ShowcaseRole::Popover, recoverable_failure())]);
        let mut runtime = runtime();

        runtime.build(plan()).unwrap();

        let events = events();
        let topbar_drop = events
            .iter()
            .position(|event| *event == Event::SurfaceDropped(1, ShowcaseRole::Topbar))
            .unwrap();
        let dock_drop = events
            .iter()
            .position(|event| *event == Event::SurfaceDropped(1, ShowcaseRole::Dock))
            .unwrap();
        let renderer_drop = events
            .iter()
            .position(|event| *event == Event::RendererDropped(1))
            .unwrap();
        let retry = events
            .iter()
            .position(|event| *event == Event::Attempt(2))
            .unwrap();
        assert!(topbar_drop < renderer_drop);
        assert!(dock_drop < renderer_drop);
        assert!(renderer_drop < retry);
    }

    #[test]
    fn installed_resources_drop_every_surface_before_the_renderer() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset([]);
        let mut runtime = runtime();
        runtime.build(plan()).unwrap();

        drop(runtime);

        let events = events();
        let renderer_drop = events
            .iter()
            .position(|event| *event == Event::RendererDropped(1))
            .unwrap();
        for role in [
            ShowcaseRole::Topbar,
            ShowcaseRole::Dock,
            ShowcaseRole::Popover,
            ShowcaseRole::AppMenu,
            ShowcaseRole::Preview,
            ShowcaseRole::Settings,
        ] {
            let surface_drop = events
                .iter()
                .position(|event| *event == Event::SurfaceDropped(1, role))
                .unwrap();
            assert!(surface_drop < renderer_drop, "{role:?} outlived renderer");
        }
    }

    #[test]
    fn failed_rebuild_discards_installed_resources_and_finishes_not_ready() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset([]);
        let mut runtime = runtime();
        runtime.build(plan()).unwrap();
        recording()
            .failures
            .push_back((ShowcaseRole::Topbar, unrecoverable_failure()));

        let error = runtime.rebuild(plan()).unwrap_err();

        assert_eq!(error.code(), HRESULT(0x8000_4005_u32 as i32));
        assert!(!runtime.is_ready());
        let events = events();
        let rebuild_attempt = events
            .iter()
            .position(|event| *event == Event::Attempt(2))
            .unwrap();
        for role in [
            ShowcaseRole::Topbar,
            ShowcaseRole::Dock,
            ShowcaseRole::Popover,
            ShowcaseRole::AppMenu,
            ShowcaseRole::Preview,
            ShowcaseRole::Settings,
        ] {
            let old_surface_drop = events
                .iter()
                .position(|event| *event == Event::SurfaceDropped(1, role))
                .unwrap();
            assert!(old_surface_drop < rebuild_attempt);
        }
        let old_renderer_drop = events
            .iter()
            .position(|event| *event == Event::RendererDropped(1))
            .unwrap();
        assert!(old_renderer_drop < rebuild_attempt);
    }

    #[test]
    fn one_recoverable_failure_makes_exactly_two_total_attempts() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset([(ShowcaseRole::Topbar, recoverable_failure())]);
        let mut runtime = runtime();

        runtime.build(plan()).unwrap();

        let attempts = events()
            .into_iter()
            .filter(|event| matches!(event, Event::Attempt(_)))
            .count();
        assert_eq!(attempts, 2);
    }

    #[test]
    fn second_recoverable_failure_returns_error_without_ready_resources() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset([
            (ShowcaseRole::Topbar, recoverable_failure()),
            (ShowcaseRole::Topbar, recoverable_failure()),
        ]);
        let mut runtime = runtime();

        let error = runtime.build(plan()).unwrap_err();

        assert!(shell_renderer::native::is_recoverable_hresult(error.code()));
        assert!(!runtime.is_ready());
        assert_eq!(recording().attempts, 2);
    }

    #[test]
    fn unrecoverable_failure_does_not_retry() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset([(ShowcaseRole::Topbar, unrecoverable_failure())]);
        let mut runtime = runtime();

        let error = runtime.build(plan()).unwrap_err();

        assert_eq!(error.code(), HRESULT(0x8000_4005_u32 as i32));
        assert!(!runtime.is_ready());
        assert_eq!(recording().attempts, 1);
    }
}
