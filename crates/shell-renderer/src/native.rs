use std::cell::RefCell;

mod animation;
mod resize;

pub use animation::SurfaceVisibilityAnimation;

use windows::Win32::Foundation::{E_INVALIDARG, HWND};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES1,
    D2D1_DEVICE_CONTEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1CreateFactory,
    ID2D1Bitmap1, ID2D1DeviceContext, ID2D1Factory1,
};
use windows::Win32::Graphics::Direct3D11::ID3D11Device;
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice, IDCompositionDevice, IDCompositionEffectGroup, IDCompositionTarget,
    IDCompositionVisual,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWriteCreateFactory, IDWriteFactory,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
    DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIDevice, IDXGIFactory2, IDXGISurface, IDXGISwapChain1,
    IDXGISwapChain3,
};
use windows::core::{Interface, Result};

use crate::ShowcaseTokens;
use crate::native_device::create_d3d_device;
use crate::native_icons::NativeIconCache;
use crate::native_present::{present_swap_chain, present_swap_chain_blocking};
use crate::native_showcase::{ShowcaseStyle, draw_showcase};
use crate::native_showcase_resources::{DockInsetBitmap, create_dock_inset_bitmap};
use crate::{
    ContextMenuScene, DockScene, Dpi, PopoverScene, SettingsScene, TopbarScene, WindowPreviewScene,
    logical_surface_rect,
};

pub use crate::native_present::{
    DeviceLossKind, PresentOutcome, classify_present_hresult, device_loss_hresult,
    is_recoverable_hresult,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceKind {
    Hardware,
    Warp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShowcaseRole {
    Topbar,
    Dock,
    Popover,
    Preview,
    Settings,
}

#[derive(Clone, Copy)]
pub struct ShellScenes<'a> {
    pub topbar: Option<&'a TopbarScene>,
    pub dock: Option<&'a DockScene>,
    pub popover: Option<&'a PopoverScene>,
    pub context_menu: Option<&'a ContextMenuScene>,
    pub settings: Option<&'a SettingsScene>,
    pub preview: Option<&'a WindowPreviewScene>,
}

pub struct CompositionRenderer {
    _d3d: ID3D11Device,
    pub(super) d2d_context: ID2D1DeviceContext,
    dwrite: IDWriteFactory,
    dcomp: IDCompositionDevice,
    device_kind: DeviceKind,
    icons: RefCell<NativeIconCache>,
    solid_material: bool,
}

pub struct WindowSurface {
    pub(super) swap_chain: IDXGISwapChain1,
    pub(super) back_buffers: RefCell<BackBufferCache<ID2D1Bitmap1>>,
    device: ID3D11Device,
    dcomp: IDCompositionDevice,
    _target: IDCompositionTarget,
    visual: IDCompositionVisual,
    opacity_effect: IDCompositionEffectGroup,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) dpi: Dpi,
    dock_inset: Option<DockInsetBitmap>,
}

pub(super) struct BackBufferCache<T> {
    slots: [Option<T>; 2],
}

impl<T> Default for BackBufferCache<T> {
    fn default() -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
        }
    }
}

impl<T> BackBufferCache<T> {
    fn get_or_try_insert_with<E>(
        &mut self,
        index: usize,
        create: impl FnOnce() -> std::result::Result<T, E>,
    ) -> std::result::Result<Option<&T>, E> {
        let Some(slot) = self.slots.get_mut(index) else {
            return Ok(None);
        };
        if slot.is_none() {
            *slot = Some(create()?);
        }
        Ok(slot.as_ref())
    }

    pub(super) fn clear(&mut self) {
        self.slots.fill_with(|| None);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SurfaceMetrics {
    width: u32,
    height: u32,
    dpi: Dpi,
}

impl SurfaceMetrics {
    #[must_use]
    pub const fn new(width: u32, height: u32, dpi: Dpi) -> Self {
        Self { width, height, dpi }
    }

    #[must_use]
    pub const fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

impl CompositionRenderer {
    pub fn new(force_warp: bool, solid_material: bool) -> Result<Self> {
        let (d3d, device_kind) = create_d3d_device(force_warp)?;
        let dxgi_device: IDXGIDevice = d3d.cast()?;

        // SAFETY: Category 8 (FFI boundary). The typed windows bindings provide the
        // factory IID and initialize the returned COM interface on success.
        let d2d_factory: ID2D1Factory1 =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None) }?;
        // SAFETY: Category 8 (FFI boundary). `dxgi_device` is a live COM interface
        // created from the BGRA-capable D3D11 device above.
        let d2d_device = unsafe { d2d_factory.CreateDevice(&dxgi_device) }?;
        // SAFETY: Category 8 (FFI boundary). `d2d_device` remains owned by the
        // returned context, and the option value is a documented constant.
        let d2d_context =
            unsafe { d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE) }?;
        // SAFETY: Category 8 (FFI boundary). The generic return type supplies the
        // official DirectWrite factory IID and receives an initialized interface.
        let dwrite: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }?;
        // SAFETY: Category 8 (FFI boundary). DirectComposition accepts the live
        // DXGI device and the generic interface type supplies its documented IID.
        let dcomp: IDCompositionDevice = unsafe { DCompositionCreateDevice(&dxgi_device) }?;
        let icons = NativeIconCache::new(&d2d_context)?;

        Ok(Self {
            _d3d: d3d,
            d2d_context,
            dwrite,
            dcomp,
            device_kind,
            icons: RefCell::new(icons),
            solid_material,
        })
    }

    #[must_use]
    pub const fn device_kind(&self) -> DeviceKind {
        self.device_kind
    }

    pub fn create_surface(
        &self,
        hwnd: HWND,
        role: ShowcaseRole,
        metrics: SurfaceMetrics,
        scenes: ShellScenes<'_>,
    ) -> Result<WindowSurface> {
        let SurfaceMetrics { width, height, dpi } = metrics;
        let dxgi_device: IDXGIDevice = self._d3d.cast()?;
        // SAFETY: Category 8 (FFI boundary). `dxgi_device` is live and its adapter
        // and factory parent interfaces are queried through COM QueryInterface.
        let adapter = unsafe { dxgi_device.GetAdapter() }?;
        // SAFETY: Category 8 (FFI boundary). The adapter has a DXGI factory parent;
        // the requested `IDXGIFactory2` is required by composition swap chains.
        let factory: IDXGIFactory2 = unsafe { adapter.GetParent() }?;
        let description = DXGI_SWAP_CHAIN_DESC1 {
            Width: width,
            Height: height,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            Stereo: false.into(),
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            Scaling: DXGI_SCALING_STRETCH,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
            Flags: 0,
        };
        // SAFETY: Category 8 (FFI boundary). The descriptor uses the documented
        // flip-model composition combination and the device outlives the chain.
        let swap_chain =
            unsafe { factory.CreateSwapChainForComposition(&self._d3d, &description, None) }?;
        let index = Self::current_back_buffer_index(&swap_chain)?;
        let mut back_buffers = BackBufferCache::default();
        let bitmap = back_buffers
            .get_or_try_insert_with(index, || {
                self.create_target_bitmap(&swap_chain, dpi, index as u32)
            })?
            .cloned()
            .ok_or_else(|| windows::core::Error::from_hresult(E_INVALIDARG))?;
        // SAFETY: Category 8 (FFI boundary). The bitmap and context are live COM
        // interfaces from the same D2D device.
        unsafe {
            self.d2d_context.SetTarget(&bitmap);
            self.d2d_context.SetDpi(dpi.raw() as f32, dpi.raw() as f32);
        }
        let mut icons = self.icons.borrow_mut();
        let logical_surface = logical_surface_rect(width, height, dpi);
        let dock_inset = if role == ShowcaseRole::Dock && !self.solid_material {
            Some(create_dock_inset_bitmap(
                &self.d2d_context,
                logical_surface.width,
                logical_surface.height,
                ShowcaseTokens::obsidian_glass().dock_radius,
            )?)
        } else {
            None
        };
        draw_showcase(
            &self.d2d_context,
            &self.dwrite,
            &mut icons,
            ShowcaseStyle::new(role, self.solid_material, dock_inset.as_ref()),
            logical_surface,
            scenes,
        )?;

        // SAFETY: Category 8 (FFI boundary). `hwnd` is a live top-level window owned
        // by the caller and remains valid for the lifetime of the returned surface.
        let target = unsafe { self.dcomp.CreateTargetForHwnd(hwnd, true) }?;
        // SAFETY: Category 8 (FFI boundary). The device initializes the returned
        // visual interface and retains it via the target root assignment below.
        let visual = unsafe { self.dcomp.CreateVisual() }?;
        // SAFETY: Category 8 (FFI boundary). A composition swap chain is a supported
        // visual content object and remains owned by this surface.
        unsafe { visual.SetContent(&swap_chain) }?;
        // SAFETY: Category 8 (FFI boundary). Both COM objects come from this
        // composition device and remain retained by the returned surface.
        let opacity_effect = unsafe {
            let effect = self.dcomp.CreateEffectGroup()?;
            visual.SetEffect(&effect)?;
            effect
        };
        // SAFETY: Category 8 (FFI boundary). `visual` belongs to the same composition
        // device as `target` and remains alive in the returned owner.
        unsafe { target.SetRoot(&visual) }?;
        // SAFETY: Category 8 (FFI boundary). All pending operations reference live
        // resources retained in this renderer/surface pair.
        unsafe { self.dcomp.Commit() }?;
        let surface = WindowSurface {
            swap_chain,
            back_buffers: RefCell::new(back_buffers),
            device: self._d3d.clone(),
            dcomp: self.dcomp.clone(),
            _target: target,
            opacity_effect,
            visual,
            width,
            height,
            dpi,
            dock_inset,
        };
        match surface.present_blocking()? {
            PresentOutcome::Presented | PresentOutcome::FrameSkipped => Ok(surface),
            PresentOutcome::DeviceLost(kind) => Err(windows::core::Error::new(
                device_loss_hresult(kind),
                format!("recoverable device loss during initial present: {kind:?}"),
            )),
            PresentOutcome::Failed(code) => Err(windows::core::Error::from_hresult(code)),
        }
    }

    pub fn redraw_surface(
        &self,
        surface: &WindowSurface,
        role: ShowcaseRole,
        scenes: ShellScenes<'_>,
    ) -> Result<PresentOutcome> {
        let bitmap = self.current_target_bitmap(surface)?;
        // SAFETY: Category 8 (FFI boundary). The freshly acquired back-buffer
        // bitmap belongs to this D2D device; its DPI matches the logical scene.
        unsafe {
            self.d2d_context.SetTarget(&bitmap);
            self.d2d_context
                .SetDpi(surface.dpi.raw() as f32, surface.dpi.raw() as f32);
        }
        let mut icons = self.icons.borrow_mut();
        draw_showcase(
            &self.d2d_context,
            &self.dwrite,
            &mut icons,
            ShowcaseStyle::new(role, self.solid_material, surface.dock_inset.as_ref()),
            logical_surface_rect(surface.width, surface.height, surface.dpi),
            scenes,
        )?;
        surface.present()
    }

    fn current_target_bitmap(&self, surface: &WindowSurface) -> Result<ID2D1Bitmap1> {
        let index = Self::current_back_buffer_index(&surface.swap_chain)?;
        surface
            .back_buffers
            .borrow_mut()
            .get_or_try_insert_with(index, || {
                self.create_target_bitmap(&surface.swap_chain, surface.dpi, index as u32)
            })?
            .cloned()
            .ok_or_else(|| windows::core::Error::from_hresult(E_INVALIDARG))
    }

    fn current_back_buffer_index(swap_chain: &IDXGISwapChain1) -> Result<usize> {
        let rotating_chain: IDXGISwapChain3 = swap_chain.cast()?;
        // SAFETY: Category 8 (FFI boundary). The typed swapchain is live and
        // reports the current writable back-buffer index without mutation.
        let index = unsafe { rotating_chain.GetCurrentBackBufferIndex() };
        Ok(index as usize)
    }

    fn create_target_bitmap(
        &self,
        swap_chain: &IDXGISwapChain1,
        dpi: Dpi,
        index: u32,
    ) -> Result<ID2D1Bitmap1> {
        // SAFETY: Category 8 (FFI boundary). The reported index belongs to this
        // live flip-model chain and is queried as its documented DXGI surface.
        let surface: IDXGISurface = unsafe { swap_chain.GetBuffer(index) }?;
        let properties = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: dpi.raw() as f32,
            dpiY: dpi.raw() as f32,
            bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
            ..Default::default()
        };
        // SAFETY: Category 8 (FFI boundary). The current DXGI back buffer and
        // bitmap properties share format, alpha mode, device and effective DPI.
        unsafe {
            self.d2d_context
                .CreateBitmapFromDxgiSurface(&surface, Some(&properties))
        }
    }
}

impl WindowSurface {
    #[must_use]
    pub const fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    #[must_use]
    pub const fn metrics(&self) -> SurfaceMetrics {
        SurfaceMetrics::new(self.width, self.height, self.dpi)
    }

    pub fn present(&self) -> Result<PresentOutcome> {
        present_swap_chain(&self.swap_chain, &self.device)
    }

    fn present_blocking(&self) -> Result<PresentOutcome> {
        present_swap_chain_blocking(&self.swap_chain, &self.device)
    }

    pub fn set_opacity(&self, opacity: f32) -> Result<()> {
        let opacity = if opacity.is_finite() {
            opacity.clamp(0.0, 1.0)
        } else {
            1.0
        };
        // SAFETY: Category 8 (FFI boundary). The visual belongs to this live
        // composition device and receives a finite normalized opacity.
        unsafe { self.opacity_effect.SetOpacity2(opacity) }?;
        // SAFETY: Category 8 (FFI boundary). The visual update references only
        // resources owned by this surface.
        unsafe { self.dcomp.Commit() }
    }

    pub fn animate_entrance(&self, reduced_motion: bool) -> Result<()> {
        if reduced_motion {
            // SAFETY: Category 8 (FFI boundary). The visual belongs to this live
            // composition device and accepts a finite immediate offset.
            unsafe { self.visual.SetOffsetY2(0.0) }?;
        } else {
            let duration = 0.18_f64;
            let start = -6.0_f32;
            // SAFETY: Category 8 (FFI boundary). The device returns an owned
            // animation and all polynomial coefficients are finite.
            let animation = unsafe { self.dcomp.CreateAnimation() }?;
            // SAFETY: Category 8 (FFI boundary). The cubic segment and terminal
            // value form one bounded 180 ms ease-out animation.
            unsafe {
                animation.AddCubic(
                    0.0,
                    start,
                    0.0,
                    -3.0 * start / (duration * duration) as f32,
                    2.0 * start / (duration * duration * duration) as f32,
                )?;
                animation.End(duration, 0.0)?;
                self.visual.SetOffsetY(&animation)?;
            }
        }
        // SAFETY: Category 8 (FFI boundary). All pending animation operations
        // reference resources owned by this surface.
        unsafe { self.dcomp.Commit() }
    }
}

#[cfg(test)]
mod tests {
    use crate::Dpi;

    use super::{BackBufferCache, SurfaceMetrics, WindowSurface};

    #[test]
    fn back_buffer_cache_reuses_each_slot_and_clears_before_resize() {
        let mut cache = BackBufferCache::default();
        let mut creates = 0;

        assert_eq!(
            cache
                .get_or_try_insert_with(0, || Ok::<_, ()>({
                    creates += 1;
                    10
                }))
                .unwrap(),
            Some(&10)
        );
        assert_eq!(
            cache.get_or_try_insert_with(0, || Ok::<_, ()>(99)).unwrap(),
            Some(&10)
        );
        assert_eq!(
            cache
                .get_or_try_insert_with(1, || Ok::<_, ()>({
                    creates += 1;
                    20
                }))
                .unwrap(),
            Some(&20)
        );
        assert_eq!(creates, 2);

        cache.clear();
        assert_eq!(
            cache
                .get_or_try_insert_with(0, || Ok::<_, ()>({
                    creates += 1;
                    30
                }))
                .unwrap(),
            Some(&30)
        );
        assert_eq!(creates, 3);
    }

    #[test]
    fn surface_metrics_report_pixel_size() {
        let metrics = SurfaceMetrics::new(21, 22, Dpi::from_raw(144));

        assert_eq!(metrics.size(), (21, 22));
    }

    #[test]
    fn window_surface_exposes_complete_metrics() {
        let accessor: fn(&WindowSurface) -> SurfaceMetrics = WindowSurface::metrics;

        let _ = accessor;
    }
}
