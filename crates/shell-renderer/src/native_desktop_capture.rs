use std::collections::VecDeque;
use std::mem::ManuallyDrop;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::{E_INVALIDARG, HWND, RECT};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_BORDER_MODE_HARD,
    D2D1_COMPOSITE_MODE_SOURCE_OVER, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    CLSID_D2D1GaussianBlur, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_BITMAP_OPTIONS_NONE,
    D2D1_BITMAP_PROPERTIES1, D2D1_GAUSSIANBLUR_PROP_BORDER_MODE,
    D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION, D2D1_INTERPOLATION_MODE_LINEAR,
    D2D1_LAYER_OPTIONS1_NONE, D2D1_LAYER_PARAMETERS1, D2D1_PROPERTY_TYPE_ENUM,
    D2D1_PROPERTY_TYPE_FLOAT, D2D1_ROUNDED_RECT, ID2D1Bitmap1, ID2D1DeviceContext, ID2D1Effect,
    ID2D1Geometry,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleDC, CreateDIBSection,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, HBITMAP, HDC, HGDIOBJ, ROP_CODE, ReleaseDC,
    SRCCOPY, SelectObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, GetWindowRect, IsWindow, IsWindowVisible, PostMessageW, SM_CXVIRTUALSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, WM_APP,
};
use windows::core::{Error, Interface, Result};
use windows_numerics::{Matrix3x2, Vector2};

use crate::native::ShowcaseRole;
use crate::{Dpi, PopoverLayoutStyle};

const PANEL_BODY_TOP_DIP: f32 = 8.0;
const MAX_BLUR_STANDARD_DEVIATION_DIP: f32 = 32.0;
const BLUR_KERNEL_RADIUS_MULTIPLIER: f32 = 3.0;
const CAPTURE_CACHE_BYTE_BUDGET: usize = 32 * 1024 * 1024;
const COMPLETED_CAPTURE_BYTE_BUDGET: usize = 32 * 1024 * 1024;
const MAX_CAPTURE_SLOTS: usize = 16;

/// Pointer-free wake posted after a hidden desktop capture enters the bounded
/// CPU cache. The platform translates it into a UI-thread redraw when useful.
pub const DESKTOP_BLUR_WAKE_MESSAGE: u32 = WM_APP + 0x6A;

#[derive(Clone, Copy)]
pub(crate) struct DesktopBlurTarget {
    hwnd: HWND,
    width: u32,
    height: u32,
    dpi: Dpi,
    blur_radius: u8,
}

impl DesktopBlurTarget {
    pub(crate) const fn new(
        hwnd: HWND,
        width: u32,
        height: u32,
        dpi: Dpi,
        blur_radius: u8,
    ) -> Self {
        Self {
            hwnd,
            width,
            height,
            dpi,
            blur_radius,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PixelRect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl PixelRect {
    pub(super) const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct CapturePlan {
    source: PixelRect,
    draw_origin_x_dip: f32,
    draw_origin_y_dip: f32,
    body_top_dip: f32,
}

pub(super) fn capture_plan(
    window_x: i32,
    window_y: i32,
    width: u32,
    height: u32,
    dpi: u32,
    virtual_desktop: PixelRect,
) -> Option<CapturePlan> {
    let scale = dpi.max(96) as f32 / 96.0;
    let body_top_px = (PANEL_BODY_TOP_DIP * scale).round() as i32;
    let body_height = height.checked_sub(body_top_px.max(0) as u32)?;
    if width == 0 || body_height == 0 || virtual_desktop.width == 0 || virtual_desktop.height == 0 {
        return None;
    }
    let overscan =
        (MAX_BLUR_STANDARD_DEVIATION_DIP * BLUR_KERNEL_RADIUS_MULTIPLIER * scale).ceil() as i64;
    let requested_left = i64::from(window_x) - overscan;
    let requested_top = i64::from(window_y) + i64::from(body_top_px) - overscan;
    let requested_right = i64::from(window_x) + i64::from(width) + overscan;
    let requested_bottom = i64::from(window_y) + i64::from(height) + overscan;
    let desktop_left = i64::from(virtual_desktop.x);
    let desktop_top = i64::from(virtual_desktop.y);
    let desktop_right = desktop_left + i64::from(virtual_desktop.width);
    let desktop_bottom = desktop_top + i64::from(virtual_desktop.height);
    let source_left = requested_left.max(desktop_left);
    let source_top = requested_top.max(desktop_top);
    let source_right = requested_right.min(desktop_right);
    let source_bottom = requested_bottom.min(desktop_bottom);
    let source_width = u32::try_from(source_right.checked_sub(source_left)?).ok()?;
    let source_height = u32::try_from(source_bottom.checked_sub(source_top)?).ok()?;
    if source_width == 0 || source_height == 0 {
        return None;
    }
    Some(CapturePlan {
        source: PixelRect::new(
            i32::try_from(source_left).ok()?,
            i32::try_from(source_top).ok()?,
            source_width,
            source_height,
        ),
        draw_origin_x_dip: (source_left - i64::from(window_x)) as f32 / scale,
        draw_origin_y_dip: (source_top - i64::from(window_y)) as f32 / scale,
        body_top_dip: PANEL_BODY_TOP_DIP,
    })
}

pub(super) fn wants_desktop_blur(
    role: ShowcaseRole,
    style: Option<PopoverLayoutStyle>,
    quick_settings: bool,
    solid_material: bool,
) -> bool {
    role == ShowcaseRole::Popover
        && (matches!(style, Some(PopoverLayoutStyle::SystemPanel)) || quick_settings)
        && !solid_material
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CaptureKey {
    hwnd: isize,
    plan: CapturePlanKey,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CapturePlanKey {
    source: PixelRect,
    dpi: u32,
}

impl CaptureKey {
    fn new(hwnd: HWND, plan: CapturePlan, dpi: Dpi) -> Self {
        Self {
            hwnd: hwnd.0 as isize,
            plan: CapturePlanKey {
                source: plan.source,
                dpi: dpi.raw(),
            },
        }
    }
}

struct CaptureRequest {
    key: CaptureKey,
    plan: CapturePlan,
}

struct CaptureRaster {
    key: CaptureKey,
    plan: CapturePlan,
    pixels: Arc<[u8]>,
}

struct CachedCapture {
    key: CaptureKey,
    plan: CapturePlan,
    pixels: Arc<[u8]>,
}

struct CaptureCache {
    entries: VecDeque<CachedCapture>,
    byte_len: usize,
    byte_budget: usize,
}

impl Default for CaptureCache {
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
            byte_len: 0,
            byte_budget: CAPTURE_CACHE_BYTE_BUDGET,
        }
    }
}

impl CaptureCache {
    #[cfg(test)]
    fn with_budget(byte_budget: usize) -> Self {
        Self {
            byte_budget,
            ..Self::default()
        }
    }

    fn get(&mut self, key: CaptureKey) -> Option<(CapturePlan, Arc<[u8]>)> {
        let index = self.entries.iter().position(|entry| entry.key == key)?;
        let entry = self.entries.remove(index)?;
        let hit = (entry.plan, Arc::clone(&entry.pixels));
        self.entries.push_back(entry);
        Some(hit)
    }

    fn insert(&mut self, raster: CaptureRaster) {
        if raster.pixels.len() > self.byte_budget {
            return;
        }
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.key == raster.key)
            && let Some(replaced) = self.entries.remove(index)
        {
            self.byte_len = self.byte_len.saturating_sub(replaced.pixels.len());
        }
        self.byte_len = self.byte_len.saturating_add(raster.pixels.len());
        self.entries.push_back(CachedCapture {
            key: raster.key,
            plan: raster.plan,
            pixels: raster.pixels,
        });
        while self.byte_len > self.byte_budget {
            let Some(evicted) = self.entries.pop_front() else {
                break;
            };
            self.byte_len = self.byte_len.saturating_sub(evicted.pixels.len());
        }
    }
}

struct WorkerState {
    pending: VecDeque<CaptureRequest>,
    stopping: bool,
}

impl WorkerState {
    fn enqueue(&mut self, request: CaptureRequest) {
        if let Some(index) = self
            .pending
            .iter()
            .position(|entry| entry.key.hwnd == request.key.hwnd)
        {
            let _ = self.pending.remove(index);
        }
        self.pending.push_back(request);
        while self.pending.len() > MAX_CAPTURE_SLOTS {
            let _ = self.pending.pop_front();
        }
    }
}

struct CompletedCaptures {
    entries: VecDeque<CaptureRaster>,
    byte_len: usize,
    byte_budget: usize,
}

impl Default for CompletedCaptures {
    fn default() -> Self {
        Self {
            entries: VecDeque::with_capacity(MAX_CAPTURE_SLOTS),
            byte_len: 0,
            byte_budget: COMPLETED_CAPTURE_BYTE_BUDGET,
        }
    }
}

impl CompletedCaptures {
    #[cfg(test)]
    fn with_budget(byte_budget: usize) -> Self {
        Self {
            byte_budget,
            ..Self::default()
        }
    }

    fn push_latest(&mut self, raster: CaptureRaster) {
        if raster.pixels.len() > self.byte_budget {
            return;
        }
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.key.hwnd == raster.key.hwnd)
            && let Some(replaced) = self.entries.remove(index)
        {
            self.byte_len = self.byte_len.saturating_sub(replaced.pixels.len());
        }
        self.byte_len = self.byte_len.saturating_add(raster.pixels.len());
        self.entries.push_back(raster);
        while self.entries.len() > MAX_CAPTURE_SLOTS || self.byte_len > self.byte_budget {
            let Some(evicted) = self.entries.pop_front() else {
                break;
            };
            self.byte_len = self.byte_len.saturating_sub(evicted.pixels.len());
        }
    }

    fn pop_front(&mut self) -> Option<CaptureRaster> {
        let raster = self.entries.pop_front()?;
        self.byte_len = self.byte_len.saturating_sub(raster.pixels.len());
        Some(raster)
    }
}

struct WorkerShared {
    state: Mutex<WorkerState>,
    wake: Condvar,
    completed: Mutex<CompletedCaptures>,
}

pub(crate) struct DesktopBlurPipeline {
    shared: Arc<WorkerShared>,
    _worker: JoinHandle<()>,
    newly_ready: VecDeque<CaptureKey>,
    cache: CaptureCache,
}

impl std::fmt::Debug for DesktopBlurPipeline {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DesktopBlurPipeline")
            .field("ready_slots", &self.newly_ready.len())
            .field("cache_bytes", &self.cache.byte_len)
            .finish_non_exhaustive()
    }
}

impl DesktopBlurPipeline {
    pub(crate) fn new() -> std::io::Result<Self> {
        let shared = Arc::new(WorkerShared {
            state: Mutex::new(WorkerState {
                pending: VecDeque::with_capacity(MAX_CAPTURE_SLOTS),
                stopping: false,
            }),
            wake: Condvar::new(),
            completed: Mutex::new(CompletedCaptures::default()),
        });
        let worker_shared = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("desktop-blur-capture".to_owned())
            .spawn(move || capture_worker_loop(&worker_shared))?;
        Ok(Self {
            shared,
            _worker: worker,
            newly_ready: VecDeque::with_capacity(MAX_CAPTURE_SLOTS),
            cache: CaptureCache::default(),
        })
    }

    pub(crate) fn prepare_hidden(
        &mut self,
        context: &ID2D1DeviceContext,
        target: DesktopBlurTarget,
    ) -> Result<Option<DesktopBlurCapture>> {
        self.drain_completed();
        let Some((key, plan)) =
            capture_target(target.hwnd, target.width, target.height, target.dpi)?
        else {
            return Ok(None);
        };
        if let Some((cached_plan, pixels)) = self.cache.get(key) {
            remove_key(&mut self.newly_ready, key);
            return DesktopBlurCapture::from_pixels(
                context,
                cached_plan,
                target.dpi,
                target.blur_radius,
                &pixels,
            )
            .map(Some);
        }
        if !is_hidden_live_window(target.hwnd) {
            return Ok(None);
        }
        self.enqueue(key, plan);
        Ok(None)
    }

    pub(crate) fn request_hidden(
        &mut self,
        hwnd: HWND,
        width: u32,
        height: u32,
        dpi: Dpi,
    ) -> Result<()> {
        self.drain_completed();
        if !is_hidden_live_window(hwnd) {
            return Ok(());
        }
        let Some((key, plan)) = capture_target(hwnd, width, height, dpi)? else {
            return Ok(());
        };
        // Every confirmed hide refreshes the latest-only snapshot. An older
        // bounded cache entry remains available for an immediate next open until
        // this request replaces it, avoiding TTL-dependent blank material.
        self.enqueue(key, plan);
        Ok(())
    }

    fn enqueue(&mut self, key: CaptureKey, plan: CapturePlan) {
        let request = CaptureRequest { key, plan };
        let mut state = lock_recover(&self.shared.state);
        if !state.stopping {
            state.enqueue(request);
            self.shared.wake.notify_one();
        }
    }

    pub(crate) fn take_ready(
        &mut self,
        context: &ID2D1DeviceContext,
        target: DesktopBlurTarget,
    ) -> Result<Option<DesktopBlurCapture>> {
        self.drain_completed();
        let Some(key) = capture_key(target.hwnd, target.width, target.height, target.dpi)? else {
            return Ok(None);
        };
        if !remove_key(&mut self.newly_ready, key) {
            return Ok(None);
        }
        let Some((plan, pixels)) = self.cache.get(key) else {
            return Ok(None);
        };
        DesktopBlurCapture::from_pixels(context, plan, target.dpi, target.blur_radius, &pixels)
            .map(Some)
    }

    /// Reprocesses the last radius-independent CPU raster without waiting for
    /// a new hidden-window capture. This is used only when a visible surface
    /// turns blur back on; GDI must never capture the panel into itself.
    pub(crate) fn reprocess_cached(
        &mut self,
        context: &ID2D1DeviceContext,
        target: DesktopBlurTarget,
    ) -> Result<Option<DesktopBlurCapture>> {
        self.drain_completed();
        let Some(key) = capture_key(target.hwnd, target.width, target.height, target.dpi)? else {
            return Ok(None);
        };
        let Some((plan, pixels)) = self.cache.get(key) else {
            return Ok(None);
        };
        DesktopBlurCapture::from_pixels(context, plan, target.dpi, target.blur_radius, &pixels)
            .map(Some)
    }

    fn drain_completed(&mut self) {
        let mut completed = lock_recover(&self.shared.completed);
        while let Some(raster) = completed.pop_front() {
            remove_key(&mut self.newly_ready, raster.key);
            self.newly_ready.push_back(raster.key);
            while self.newly_ready.len() > MAX_CAPTURE_SLOTS {
                let _ = self.newly_ready.pop_front();
            }
            self.cache.insert(raster);
        }
    }
}

fn remove_key(keys: &mut VecDeque<CaptureKey>, key: CaptureKey) -> bool {
    let Some(index) = keys.iter().position(|entry| *entry == key) else {
        return false;
    };
    let _ = keys.remove(index);
    true
}

fn capture_key(hwnd: HWND, width: u32, height: u32, dpi: Dpi) -> Result<Option<CaptureKey>> {
    Ok(capture_target(hwnd, width, height, dpi)?.map(|(key, _plan)| key))
}

fn capture_target(
    hwnd: HWND,
    width: u32,
    height: u32,
    dpi: Dpi,
) -> Result<Option<(CaptureKey, CapturePlan)>> {
    if !is_live_window(hwnd) {
        return Ok(None);
    }
    let mut window = RECT::default();
    // SAFETY: `window` is valid output storage and `hwnd` remains live.
    unsafe { GetWindowRect(hwnd, &mut window) }?;
    let Some(plan) = capture_plan(
        window.left,
        window.top,
        width,
        height,
        dpi.raw(),
        virtual_desktop_rect()?,
    ) else {
        return Ok(None);
    };
    if pixel_byte_len(plan.source)? > CAPTURE_CACHE_BYTE_BUDGET {
        return Ok(None);
    }
    Ok(Some((CaptureKey::new(hwnd, plan, dpi), plan)))
}

fn capture_worker_loop(shared: &WorkerShared) {
    loop {
        let request = {
            let mut state = lock_recover(&shared.state);
            while state.pending.is_empty() && !state.stopping {
                state = shared
                    .wake
                    .wait(state)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
            if state.stopping {
                return;
            }
            state.pending.pop_front()
        };
        let Some(request) = request else {
            continue;
        };
        let hwnd = HWND(request.key.hwnd as *mut core::ffi::c_void);
        if !is_hidden_live_window(hwnd) {
            continue;
        }
        match capture_bgra(request.plan.source) {
            Ok(pixels) => {
                if !is_hidden_live_window(hwnd) {
                    continue;
                }
                let mut completed = lock_recover(&shared.completed);
                completed.push_latest(CaptureRaster {
                    key: request.key,
                    plan: request.plan,
                    pixels: pixels.into(),
                });
                drop(completed);
                // SAFETY: This pointer-free private message is only a wake hint;
                // stale HWND failures are benign and no capture bytes cross it.
                let _ = unsafe {
                    PostMessageW(
                        Some(hwnd),
                        DESKTOP_BLUR_WAKE_MESSAGE,
                        Default::default(),
                        Default::default(),
                    )
                };
            }
            Err(error) => eprintln!(
                "DESKTOP_BLUR stage=worker_capture status=fallback hresult={:#010X}",
                error.code().0 as u32,
            ),
        }
    }
}

fn lock_recover<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn is_live_window(hwnd: HWND) -> bool {
    // SAFETY: The copied HWND is used only for a read-only validity query. A
    // destroyed or recycled handle is treated as ineligible by the caller.
    unsafe { IsWindow(Some(hwnd)) }.as_bool()
}

fn is_hidden_live_window(hwnd: HWND) -> bool {
    if !is_live_window(hwnd) {
        return false;
    }
    // SAFETY: The HWND passed the best-effort validity check and visibility is
    // queried without mutation. Callers recheck after BitBlt to close the race.
    !unsafe { IsWindowVisible(hwnd) }.as_bool()
}

pub(crate) struct DesktopBlurCapture {
    _bitmap: ID2D1Bitmap1,
    effect: ID2D1Effect,
    plan: CapturePlan,
}

impl DesktopBlurCapture {
    fn from_pixels(
        context: &ID2D1DeviceContext,
        plan: CapturePlan,
        dpi: Dpi,
        blur_radius: u8,
        pixels: &[u8],
    ) -> Result<Self> {
        let expected_len = pixel_byte_len(plan.source)?;
        if pixels.len() != expected_len {
            return Err(capture_error());
        }
        let properties = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: dpi.raw() as f32,
            dpiY: dpi.raw() as f32,
            bitmapOptions: D2D1_BITMAP_OPTIONS_NONE,
            ..Default::default()
        };
        // SAFETY: The tightly packed BGRA bytes remain live for the synchronous
        // upload and match the declared dimensions, stride, format and alpha mode.
        let bitmap = unsafe {
            context.CreateBitmap(
                D2D_SIZE_U {
                    width: plan.source.width,
                    height: plan.source.height,
                },
                Some(pixels.as_ptr().cast()),
                plan.source.width * 4,
                &properties,
            )?
        };
        // SAFETY: The built-in effect CLSID is supported by Direct2D and the
        // input bitmap belongs to the same device context.
        let effect = unsafe { context.CreateEffect(&CLSID_D2D1GaussianBlur) }?;
        // SAFETY: The effect and input bitmap belong to the same live device context;
        // both property byte slices exactly match the declared scalar property types.
        unsafe {
            effect.SetInput(0, &bitmap, true);
            effect.SetValue(
                D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION.0 as u32,
                D2D1_PROPERTY_TYPE_FLOAT,
                &f32::from(blur_radius).to_ne_bytes(),
            )?;
            effect.SetValue(
                D2D1_GAUSSIANBLUR_PROP_BORDER_MODE.0 as u32,
                D2D1_PROPERTY_TYPE_ENUM,
                &D2D1_BORDER_MODE_HARD.0.to_ne_bytes(),
            )?;
        }
        Ok(Self {
            _bitmap: bitmap,
            effect,
            plan,
        })
    }

    pub(crate) fn set_blur_radius(&self, blur_radius: u8) -> Result<()> {
        // SAFETY: The effect remains owned by this capture and the byte slice
        // exactly represents the documented FLOAT standard-deviation value.
        unsafe {
            self.effect.SetValue(
                D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION.0 as u32,
                D2D1_PROPERTY_TYPE_FLOAT,
                &f32::from(blur_radius).to_ne_bytes(),
            )
        }
    }

    pub(crate) fn draw(
        &self,
        context: &ID2D1DeviceContext,
        width: f32,
        height: f32,
        radius: f32,
    ) -> Result<()> {
        let body = D2D_RECT_F {
            left: 0.0,
            top: self.plan.body_top_dip,
            right: width,
            bottom: height,
        };
        let rounded = D2D1_ROUNDED_RECT {
            rect: body,
            radiusX: radius,
            radiusY: radius,
        };
        // SAFETY: The context and factory are live for this synchronous draw and
        // the rounded bounds are finite panel geometry.
        let factory = unsafe { context.GetFactory() }?;
        // SAFETY: The factory is live and the rounded rectangle contains finite bounds.
        let geometry = unsafe { factory.CreateRoundedRectangleGeometry(&rounded) }?;
        let geometry: ID2D1Geometry = geometry.cast()?;
        // SAFETY: The layer is created by the same render target used below.
        let layer = unsafe { context.CreateLayer(None) }?;
        let mut parameters = D2D1_LAYER_PARAMETERS1 {
            contentBounds: body,
            geometricMask: ManuallyDrop::new(Some(geometry)),
            maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
            maskTransform: Matrix3x2::identity(),
            opacity: 1.0,
            opacityBrush: ManuallyDrop::new(None),
            layerOptions: D2D1_LAYER_OPTIONS1_NONE,
        };
        let target = Vector2 {
            X: self.plan.draw_origin_x_dip,
            Y: self.plan.draw_origin_y_dip,
        };
        // SAFETY: The effect and its retained bitmap input remain live for the
        // complete draw below.
        let output = unsafe { self.effect.GetOutput() }?;
        // SAFETY: The layer, geometry, effect input and target all belong to this
        // Direct2D device and are retained for the complete push/draw/pop sequence.
        unsafe {
            context.PushLayer(&parameters, &layer);
            context.DrawImage(
                &output,
                Some(&target),
                None,
                D2D1_INTERPOLATION_MODE_LINEAR,
                D2D1_COMPOSITE_MODE_SOURCE_OVER,
            );
            context.PopLayer();
            drop(ManuallyDrop::take(&mut parameters.geometricMask));
            drop(ManuallyDrop::take(&mut parameters.opacityBrush));
        }
        Ok(())
    }
}

fn virtual_desktop_rect() -> Result<PixelRect> {
    // SAFETY: These process-global system metrics are read-only and require no
    // additional lifetime or initialization contract.
    let (x, y, width, height) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    let width = u32::try_from(width).map_err(|_| capture_error())?;
    let height = u32::try_from(height).map_err(|_| capture_error())?;
    if width == 0 || height == 0 {
        return Err(capture_error());
    }
    Ok(PixelRect::new(x, y, width, height))
}

fn capture_bgra(rect: PixelRect) -> Result<Vec<u8>> {
    let byte_len = pixel_byte_len(rect)?;
    capture_bgra_with_len(rect, byte_len)
}

fn pixel_byte_len(rect: PixelRect) -> Result<usize> {
    let byte_len = usize::try_from(rect.width)
        .ok()
        .and_then(|width| {
            usize::try_from(rect.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(capture_error)?;
    Ok(byte_len)
}

fn capture_bgra_with_len(rect: PixelRect, byte_len: usize) -> Result<Vec<u8>> {
    // SAFETY: A null HWND requests the desktop DC. The guard releases it on the
    // same thread after the synchronous transfer.
    let screen = ScreenDc::acquire()?;
    let memory = CompatibleDc::create(screen.0)?;
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: i32::try_from(rect.width).map_err(|_| capture_error())?,
            biHeight: -i32::try_from(rect.height).map_err(|_| capture_error())?,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits = std::ptr::null_mut();
    // SAFETY: The bitmap description requests a top-down 32-bit DIB and `bits`
    // is valid output storage retained until the owned bitmap is dropped.
    let bitmap = OwnedBitmap(unsafe {
        CreateDIBSection(Some(screen.0), &info, DIB_RGB_COLORS, &mut bits, None, 0)?
    });
    if bits.is_null() {
        return Err(capture_error());
    }
    // SAFETY: Both DC and bitmap are live; the selection guard restores the
    // previous object before the bitmap or memory DC are released.
    let selected = unsafe { SelectObject(memory.0, HGDIOBJ(bitmap.0.0)) };
    if selected.0.is_null() {
        return Err(capture_error());
    }
    let _selection = SelectedObject {
        dc: memory.0,
        previous: selected,
    };
    // SAFETY: The source and destination DCs are live, rectangles are bounded by
    // the virtual desktop, and the destination DIB matches the transfer size.
    unsafe {
        BitBlt(
            memory.0,
            0,
            0,
            rect.width as i32,
            rect.height as i32,
            Some(screen.0),
            rect.x,
            rect.y,
            ROP_CODE(SRCCOPY.0 | CAPTUREBLT.0),
        )?;
    }
    // SAFETY: CreateDIBSection returned storage for exactly width*height 32-bit
    // pixels and all GDI writes have completed synchronously.
    let mut pixels = unsafe { std::slice::from_raw_parts(bits.cast::<u8>(), byte_len) }.to_vec();
    for alpha in pixels.iter_mut().skip(3).step_by(4) {
        *alpha = 0xFF;
    }
    Ok(pixels)
}

struct ScreenDc(HDC);

impl ScreenDc {
    fn acquire() -> Result<Self> {
        // SAFETY: A null HWND requests the full desktop device context.
        let dc = unsafe { GetDC(None) };
        if dc.0.is_null() {
            Err(Error::from_thread())
        } else {
            Ok(Self(dc))
        }
    }
}

impl Drop for ScreenDc {
    fn drop(&mut self) {
        // SAFETY: This balances the successful desktop GetDC call above.
        let _ = unsafe { ReleaseDC(None, self.0) };
    }
}

struct CompatibleDc(HDC);

impl CompatibleDc {
    fn create(source: HDC) -> Result<Self> {
        // SAFETY: The source desktop DC remains live for this compatible DC.
        let dc = unsafe { CreateCompatibleDC(Some(source)) };
        if dc.0.is_null() {
            Err(Error::from_thread())
        } else {
            Ok(Self(dc))
        }
    }
}

fn capture_error() -> Error {
    Error::new(E_INVALIDARG, "invalid bounded desktop capture geometry")
}

impl Drop for CompatibleDc {
    fn drop(&mut self) {
        // SAFETY: This DC was created by CreateCompatibleDC and is no longer used.
        let _ = unsafe { DeleteDC(self.0) };
    }
}

struct OwnedBitmap(HBITMAP);

impl Drop for OwnedBitmap {
    fn drop(&mut self) {
        // SAFETY: The selection guard restores the previous object before this
        // owned DIB section reaches Drop.
        let _ = unsafe { DeleteObject(HGDIOBJ(self.0.0)) };
    }
}

struct SelectedObject {
    dc: HDC,
    previous: HGDIOBJ,
}

impl Drop for SelectedObject {
    fn drop(&mut self) {
        // SAFETY: The memory DC and previous object remain live during guard drop.
        let _ = unsafe { SelectObject(self.dc, self.previous) };
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Arc;

    use super::{
        CaptureCache, CaptureKey, CapturePlan, CapturePlanKey, CaptureRaster, CaptureRequest,
        CompletedCaptures, MAX_CAPTURE_SLOTS, PANEL_BODY_TOP_DIP, PixelRect, WorkerState,
        capture_plan, remove_key, wants_desktop_blur,
    };
    use crate::PopoverLayoutStyle;
    use crate::native::ShowcaseRole;

    #[test]
    fn capture_plan_overscans_three_sigma_and_clamps_to_virtual_desktop() {
        let plan = capture_plan(100, 32, 288, 400, 96, PixelRect::new(0, 0, 1_920, 1_080)).unwrap();

        assert_eq!(
            plan,
            CapturePlan {
                source: PixelRect::new(4, 0, 480, 528),
                draw_origin_x_dip: -96.0,
                draw_origin_y_dip: -32.0,
                body_top_dip: PANEL_BODY_TOP_DIP,
            }
        );
    }

    #[test]
    fn desktop_blur_is_limited_to_translucent_system_panels() {
        assert!(wants_desktop_blur(
            ShowcaseRole::Popover,
            Some(PopoverLayoutStyle::SystemPanel),
            false,
            false,
        ));
        assert!(!wants_desktop_blur(
            ShowcaseRole::Popover,
            Some(PopoverLayoutStyle::Compact),
            false,
            false,
        ));
        assert!(!wants_desktop_blur(
            ShowcaseRole::Popover,
            Some(PopoverLayoutStyle::SystemPanel),
            false,
            true,
        ));
        assert!(wants_desktop_blur(ShowcaseRole::Popover, None, true, false,));
    }

    #[test]
    fn capture_cache_evicts_least_recently_used_raster_to_its_byte_budget() {
        let mut cache = CaptureCache::with_budget(8);
        cache.insert(raster(1, 4));
        cache.insert(raster(2, 4));
        assert!(cache.get(key(1)).is_some());

        cache.insert(raster(3, 4));

        assert!(cache.get(key(1)).is_some());
        assert!(cache.get(key(2)).is_none());
        assert!(cache.get(key(3)).is_some());
        assert_eq!(cache.byte_len, 8);
    }

    #[test]
    fn capture_cache_retains_last_raster_until_replaced_and_drops_oversize() {
        let mut cache = CaptureCache::with_budget(8);
        cache.insert(raster(1, 4));
        cache.insert(raster(2, 9));

        assert!(cache.get(key(1)).is_some());
        assert!(cache.get(key(2)).is_none());
        assert_eq!(cache.byte_len, 4);
    }

    #[test]
    fn completed_backlog_is_latest_per_monitor_and_byte_bounded() {
        let mut completed = CompletedCaptures::with_budget(8);
        completed.push_latest(raster(1, 4));
        completed.push_latest(raster(2, 4));
        completed.push_latest(raster_with_geometry(1, 2, 2, 144));

        assert_eq!(completed.byte_len, 6);
        assert_eq!(
            completed
                .entries
                .iter()
                .map(|entry| entry.key.hwnd)
                .collect::<Vec<_>>(),
            vec![2, 1]
        );
        assert_eq!(completed.entries.back().unwrap().key.plan.dpi, 144);
        assert_eq!(completed.entries.back().unwrap().key.plan.source.width, 2);

        completed.push_latest(raster(3, 4));
        completed.push_latest(raster(4, 9));

        assert_eq!(completed.byte_len, 6);
        assert_eq!(
            completed
                .entries
                .iter()
                .map(|entry| entry.key.hwnd)
                .collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert_eq!(completed.pop_front().unwrap().key.hwnd, 1);
        assert_eq!(completed.byte_len, 4);
    }

    #[test]
    fn keyed_ready_queue_consumes_only_the_matching_monitor() {
        let mut ready = VecDeque::from([key(1), key(2)]);

        assert!(remove_key(&mut ready, key(1)));
        assert_eq!(ready, VecDeque::from([key(2)]));
        assert!(!remove_key(&mut ready, key(3)));
    }

    #[test]
    fn pending_capture_queue_is_fair_bounded_and_latest_per_monitor() {
        let mut state = WorkerState {
            pending: VecDeque::new(),
            stopping: false,
        };
        state.enqueue(request(1, 1.0));
        state.enqueue(request(2, 2.0));
        state.enqueue(request_with_geometry(1, 3.0, 2, 144));

        assert_eq!(
            state
                .pending
                .iter()
                .map(|entry| entry.key.hwnd)
                .collect::<Vec<_>>(),
            vec![2, 1]
        );
        assert_eq!(state.pending.back().unwrap().plan.draw_origin_x_dip, 3.0);
        assert_eq!(state.pending.back().unwrap().key.plan.dpi, 144);
        assert_eq!(state.pending.back().unwrap().key.plan.source.width, 2);

        for hwnd in 3..=(MAX_CAPTURE_SLOTS + 2) as isize {
            state.enqueue(request(hwnd, hwnd as f32));
        }
        assert_eq!(state.pending.len(), MAX_CAPTURE_SLOTS);
        assert_eq!(
            state.pending.back().unwrap().key.hwnd,
            (MAX_CAPTURE_SLOTS + 2) as isize
        );
    }

    fn raster(hwnd: isize, byte_len: usize) -> CaptureRaster {
        raster_with_geometry(hwnd, byte_len, 1, 96)
    }

    fn raster_with_geometry(hwnd: isize, byte_len: usize, width: u32, dpi: u32) -> CaptureRaster {
        let key = key_with_geometry(hwnd, width, dpi);
        let mut plan = plan();
        plan.source = key.plan.source;
        CaptureRaster {
            key,
            plan,
            pixels: Arc::from(vec![0; byte_len]),
        }
    }

    fn request(hwnd: isize, draw_origin_x_dip: f32) -> CaptureRequest {
        request_with_geometry(hwnd, draw_origin_x_dip, 1, 96)
    }

    fn request_with_geometry(
        hwnd: isize,
        draw_origin_x_dip: f32,
        width: u32,
        dpi: u32,
    ) -> CaptureRequest {
        let key = key_with_geometry(hwnd, width, dpi);
        let mut plan = plan();
        plan.source = key.plan.source;
        plan.draw_origin_x_dip = draw_origin_x_dip;
        CaptureRequest { key, plan }
    }

    const fn key(hwnd: isize) -> CaptureKey {
        key_with_geometry(hwnd, 1, 96)
    }

    const fn key_with_geometry(hwnd: isize, width: u32, dpi: u32) -> CaptureKey {
        CaptureKey {
            hwnd,
            plan: CapturePlanKey {
                source: PixelRect::new(0, 0, width, 1),
                dpi,
            },
        }
    }

    const fn plan() -> CapturePlan {
        CapturePlan {
            source: PixelRect::new(0, 0, 1, 1),
            draw_origin_x_dip: 0.0,
            draw_origin_y_dip: 0.0,
            body_top_dip: PANEL_BODY_TOP_DIP,
        }
    }
}
