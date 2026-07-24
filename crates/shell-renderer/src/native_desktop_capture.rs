use std::mem::ManuallyDrop;

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
    GetSystemMetrics, GetWindowRect, IsWindowVisible, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};
use windows::core::{Error, Interface, Result};
use windows_numerics::{Matrix3x2, Vector2};

use crate::native::ShowcaseRole;
use crate::{Dpi, PopoverLayoutStyle};

const PANEL_BODY_TOP_DIP: f32 = 8.0;
const BLUR_STANDARD_DEVIATION_DIP: f32 = 18.0;
const BLUR_KERNEL_RADIUS_MULTIPLIER: f32 = 3.0;

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
        (BLUR_STANDARD_DEVIATION_DIP * BLUR_KERNEL_RADIUS_MULTIPLIER * scale).ceil() as i64;
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

pub(crate) struct DesktopBlurCapture {
    _bitmap: ID2D1Bitmap1,
    effect: ID2D1Effect,
    plan: CapturePlan,
}

impl DesktopBlurCapture {
    pub(crate) fn capture(
        context: &ID2D1DeviceContext,
        hwnd: HWND,
        width: u32,
        height: u32,
        dpi: Dpi,
    ) -> Result<Option<Self>> {
        // SAFETY: The HWND is owned by the live surface and this read-only query
        // determines whether the capture can avoid sampling the popover itself.
        if unsafe { IsWindowVisible(hwnd) }.as_bool() {
            return Ok(None);
        }
        let mut window = RECT::default();
        // SAFETY: `window` is valid output storage and `hwnd` remains live.
        unsafe { GetWindowRect(hwnd, &mut window) }?;
        let virtual_desktop = virtual_desktop_rect()?;
        let Some(plan) = capture_plan(
            window.left,
            window.top,
            width,
            height,
            dpi.raw(),
            virtual_desktop,
        ) else {
            return Ok(None);
        };
        let pixels = capture_bgra(plan.source)?;
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
                &BLUR_STANDARD_DEVIATION_DIP.to_ne_bytes(),
            )?;
            effect.SetValue(
                D2D1_GAUSSIANBLUR_PROP_BORDER_MODE.0 as u32,
                D2D1_PROPERTY_TYPE_ENUM,
                &D2D1_BORDER_MODE_HARD.0.to_ne_bytes(),
            )?;
        }
        Ok(Some(Self {
            _bitmap: bitmap,
            effect,
            plan,
        }))
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
    let byte_len = usize::try_from(rect.width)
        .ok()
        .and_then(|width| {
            usize::try_from(rect.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(capture_error)?;
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
    use super::{CapturePlan, PANEL_BODY_TOP_DIP, PixelRect, capture_plan, wants_desktop_blur};
    use crate::PopoverLayoutStyle;
    use crate::native::ShowcaseRole;

    #[test]
    fn capture_plan_overscans_three_sigma_and_clamps_to_virtual_desktop() {
        let plan = capture_plan(100, 32, 288, 400, 96, PixelRect::new(0, 0, 1_920, 1_080)).unwrap();

        assert_eq!(
            plan,
            CapturePlan {
                source: PixelRect::new(46, 0, 396, 486),
                draw_origin_x_dip: -54.0,
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
}
