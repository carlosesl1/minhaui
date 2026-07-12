use windows::Win32::Foundation::HWND;
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
    DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget, IDCompositionVisual,
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
};
use windows::core::{Interface, Result};

use crate::native_device::create_d3d_device;
use crate::native_present::present_swap_chain;
use crate::native_showcase::draw_showcase;
use crate::{DockScene, PopoverScene, SettingsScene, TopbarScene};

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
    Settings,
}

#[derive(Clone, Copy)]
pub struct ShellScenes<'a> {
    pub topbar: Option<&'a TopbarScene>,
    pub dock: Option<&'a DockScene>,
    pub popover: Option<&'a PopoverScene>,
    pub settings: Option<&'a SettingsScene>,
}

pub struct CompositionRenderer {
    _d3d: ID3D11Device,
    d2d_context: ID2D1DeviceContext,
    dwrite: IDWriteFactory,
    dcomp: IDCompositionDevice,
    device_kind: DeviceKind,
}

pub struct WindowSurface {
    swap_chain: IDXGISwapChain1,
    device: ID3D11Device,
    bitmap: ID2D1Bitmap1,
    _target: IDCompositionTarget,
    _visual: IDCompositionVisual,
    width: u32,
    height: u32,
}

impl CompositionRenderer {
    pub fn new(force_warp: bool) -> Result<Self> {
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

        Ok(Self {
            _d3d: d3d,
            d2d_context,
            dwrite,
            dcomp,
            device_kind,
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
        width: u32,
        height: u32,
        scenes: ShellScenes<'_>,
    ) -> Result<WindowSurface> {
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
        // SAFETY: Category 8 (FFI boundary). Buffer zero exists because the swap
        // chain has two buffers and is queried as its documented DXGI surface type.
        let surface: IDXGISurface = unsafe { swap_chain.GetBuffer(0) }?;
        let bitmap_properties = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
            ..Default::default()
        };
        // SAFETY: Category 8 (FFI boundary). The DXGI surface format and alpha mode
        // exactly match the bitmap properties and remain alive through `swap_chain`.
        let bitmap = unsafe {
            self.d2d_context
                .CreateBitmapFromDxgiSurface(&surface, Some(&bitmap_properties))
        }?;
        // SAFETY: Category 8 (FFI boundary). The bitmap and context are live COM
        // interfaces from the same D2D device.
        unsafe { self.d2d_context.SetTarget(&bitmap) };
        draw_showcase(
            &self.d2d_context,
            &self.dwrite,
            role,
            width as f32,
            height as f32,
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
        // SAFETY: Category 8 (FFI boundary). `visual` belongs to the same composition
        // device as `target` and remains alive in the returned owner.
        unsafe { target.SetRoot(&visual) }?;
        // SAFETY: Category 8 (FFI boundary). All pending operations reference live
        // resources retained in this renderer/surface pair.
        unsafe { self.dcomp.Commit() }?;
        let surface = WindowSurface {
            swap_chain,
            device: self._d3d.clone(),
            bitmap,
            _target: target,
            _visual: visual,
            width,
            height,
        };
        match surface.present()? {
            PresentOutcome::Presented => Ok(surface),
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
        // SAFETY: Category 8 (FFI boundary). The retained bitmap was created from
        // this renderer's D2D device and remains owned by the live surface.
        unsafe { self.d2d_context.SetTarget(&surface.bitmap) };
        draw_showcase(
            &self.d2d_context,
            &self.dwrite,
            role,
            surface.width as f32,
            surface.height as f32,
            scenes,
        )?;
        surface.present()
    }
}

impl WindowSurface {
    #[must_use]
    pub const fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn present(&self) -> Result<PresentOutcome> {
        present_swap_chain(&self.swap_chain, &self.device)
    }
}
