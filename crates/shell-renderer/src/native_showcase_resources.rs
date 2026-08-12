use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_BITMAP_OPTIONS_NONE, D2D1_BITMAP_PROPERTIES1, D2D1_INTERPOLATION_MODE_LINEAR,
    ID2D1Bitmap1, ID2D1DeviceContext, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_MEDIUM,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TRIMMING,
    DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP, IDWriteFactory,
    IDWriteTextFormat,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::core::{Result, w};

use crate::dock_inset_raster::InsetShadowRaster;
use crate::native::ShowcaseRole;
use crate::native_icons::NativeIconCache;
use crate::{DipRect, Rgba8};

#[derive(Clone, Copy)]
pub(crate) struct ShowcaseFormats<'a> {
    pub text: &'a IDWriteTextFormat,
    pub icon: &'a IDWriteTextFormat,
}

pub(crate) struct DockBrushes<'a> {
    pub pressed: &'a ID2D1SolidColorBrush,
    pub primary: &'a ID2D1SolidColorBrush,
    pub secondary: &'a ID2D1SolidColorBrush,
    pub accent: &'a ID2D1SolidColorBrush,
    pub focus: &'a ID2D1SolidColorBrush,
}

pub(crate) struct DockRenderResources<'a> {
    pub formats: ShowcaseFormats<'a>,
    pub icons: &'a mut NativeIconCache,
}

pub(crate) struct DockInsetBitmap {
    bitmap: ID2D1Bitmap1,
}

impl DockInsetBitmap {
    pub(crate) fn draw_at(&self, context: &ID2D1DeviceContext, bounds: DipRect) {
        let destination = D2D_RECT_F {
            left: bounds.x,
            top: bounds.y,
            right: bounds.x + bounds.width,
            bottom: bounds.y + bounds.height,
        };
        // SAFETY: Category 8 (FFI boundary). The bitmap belongs to this device,
        // remains live for the call, and the finite destination covers the dock.
        unsafe {
            context.DrawBitmap(
                &self.bitmap,
                Some(&destination),
                1.0,
                D2D1_INTERPOLATION_MODE_LINEAR,
                None,
                None,
            )
        };
    }
}

pub(crate) fn create_icon_format(
    dwrite: &IDWriteFactory,
    role: ShowcaseRole,
) -> Result<IDWriteTextFormat> {
    // SAFETY: Category 8 (FFI boundary). Static strings remain valid for the call.
    let format = unsafe {
        dwrite.CreateTextFormat(
            w!("Segoe MDL2 Assets"),
            None,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            if role == ShowcaseRole::Dock {
                25.0
            } else {
                14.0
            },
            w!("en-US"),
        )?
    };
    // SAFETY: Category 8 (FFI boundary). The owned format remains live.
    unsafe {
        format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
    }
    Ok(format)
}

pub(crate) fn create_brush(
    context: &ID2D1DeviceContext,
    color: Rgba8,
) -> Result<ID2D1SolidColorBrush> {
    let color = color_f(color);
    // SAFETY: Category 8 (FFI boundary). Channels are finite normalized values.
    unsafe { context.CreateSolidColorBrush(&color, None) }
}

pub(crate) fn create_dock_inset_bitmap(
    context: &ID2D1DeviceContext,
    width: f32,
    height: f32,
    radius: f32,
) -> Result<DockInsetBitmap> {
    let raster = InsetShadowRaster::new(width, height, radius);
    let properties = D2D1_BITMAP_PROPERTIES1 {
        pixelFormat: D2D1_PIXEL_FORMAT {
            format: DXGI_FORMAT_B8G8R8A8_UNORM,
            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
        },
        dpiX: 96.0,
        dpiY: 96.0,
        bitmapOptions: D2D1_BITMAP_OPTIONS_NONE,
        ..Default::default()
    };
    // SAFETY: Category 8 (FFI boundary). The tightly packed raster remains live
    // for this synchronous copy and matches the declared premultiplied BGRA format.
    let bitmap = unsafe {
        context.CreateBitmap(
            D2D_SIZE_U {
                width: raster.width(),
                height: raster.height(),
            },
            Some(raster.pixels().as_ptr().cast()),
            raster.width() * 4,
            &properties,
        )?
    };
    Ok(DockInsetBitmap { bitmap })
}

fn color_f(color: Rgba8) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: f32::from(color.r) / 255.0,
        g: f32::from(color.g) / 255.0,
        b: f32::from(color.b) / 255.0,
        a: f32::from(color.a) / 255.0,
    }
}

pub(crate) fn create_text_format(
    dwrite: &IDWriteFactory,
    role: ShowcaseRole,
    text_scale: f32,
) -> Result<IDWriteTextFormat> {
    // SAFETY: Category 8 (FFI boundary). Static strings remain valid for the call.
    let format = unsafe {
        dwrite.CreateTextFormat(
            w!("Segoe UI Variable"),
            None,
            if role == ShowcaseRole::Topbar {
                DWRITE_FONT_WEIGHT_SEMI_BOLD
            } else {
                DWRITE_FONT_WEIGHT_MEDIUM
            },
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            if role == ShowcaseRole::Topbar {
                13.0 * text_scale.clamp(1.0, 2.5)
            } else {
                13.0
            },
            w!("en-US"),
        )?
    };
    // SAFETY: Category 8 (FFI boundary). The owned format remains live while
    // these setters establish one shared baseline for native shell labels.
    unsafe {
        format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING)?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
    }
    if role == ShowcaseRole::Preview {
        let trimming = preview_title_trimming();
        // SAFETY: Category 8 (FFI boundary). The format, trimming descriptor, and
        // ellipsis object remain live through the synchronous COM setter calls.
        let ellipsis = unsafe { dwrite.CreateEllipsisTrimmingSign(&format) }?;
        // SAFETY: Category 8 (FFI boundary). The descriptor and sign are valid.
        unsafe { format.SetTrimming(&trimming, &ellipsis) }?;
    }
    Ok(format)
}

const fn preview_title_trimming() -> DWRITE_TRIMMING {
    DWRITE_TRIMMING {
        granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
        delimiter: 0,
        delimiterCount: 0,
    }
}

pub(crate) fn create_detail_format(
    dwrite: &IDWriteFactory,
    role: ShowcaseRole,
    text_scale: f32,
) -> Result<IDWriteTextFormat> {
    let format = create_text_format(dwrite, role, text_scale)?;
    // SAFETY: Category 8 (FFI boundary). The owned format remains live and the
    // documented trailing alignment is applied before any draw call uses it.
    unsafe {
        format.SetTextAlignment(
            windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT_TRAILING,
        )?
    };
    Ok(format)
}

pub(crate) fn create_quick_settings_label_format(
    dwrite: &IDWriteFactory,
) -> Result<IDWriteTextFormat> {
    // SAFETY: Category 8 (FFI boundary). Static strings remain valid for the call.
    let format = unsafe {
        dwrite.CreateTextFormat(
            w!("Segoe UI Variable"),
            None,
            DWRITE_FONT_WEIGHT_MEDIUM,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            13.0,
            w!("en-US"),
        )?
    };
    configure_quick_settings_text(dwrite, &format)?;
    Ok(format)
}

pub(crate) fn create_quick_settings_detail_format(
    dwrite: &IDWriteFactory,
) -> Result<IDWriteTextFormat> {
    // SAFETY: Category 8 (FFI boundary). Static strings remain valid for the call.
    let format = unsafe {
        dwrite.CreateTextFormat(
            w!("Segoe UI Variable"),
            None,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            11.5,
            w!("en-US"),
        )?
    };
    configure_quick_settings_text(dwrite, &format)?;
    Ok(format)
}

fn configure_quick_settings_text(
    dwrite: &IDWriteFactory,
    format: &IDWriteTextFormat,
) -> Result<()> {
    // SAFETY: Category 8 (FFI boundary). The owned format remains live while
    // these setters establish compact, single-line quick-settings typography.
    unsafe {
        format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING)?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
    }
    let trimming = preview_title_trimming();
    // SAFETY: Category 8 (FFI boundary). The format and ellipsis object remain
    // live through the synchronous setter call.
    let ellipsis = unsafe { dwrite.CreateEllipsisTrimmingSign(format) }?;
    // SAFETY: Category 8 (FFI boundary). The trimming descriptor is valid.
    unsafe { format.SetTrimming(&trimming, &ellipsis) }?;
    Ok(())
}

pub(crate) fn create_popover_title_format(dwrite: &IDWriteFactory) -> Result<IDWriteTextFormat> {
    // SAFETY: Category 8 (FFI boundary). Static strings remain valid for the call.
    let format = unsafe {
        dwrite.CreateTextFormat(
            w!("Segoe UI Variable"),
            None,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            16.0,
            w!("en-US"),
        )?
    };
    // SAFETY: Category 8 (FFI boundary). The owned format remains live through
    // drawing and these setters only configure its immutable presentation.
    unsafe {
        format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING)?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
    }
    Ok(format)
}

#[cfg(test)]
mod tests {
    use windows::Win32::Graphics::DirectWrite::DWRITE_TRIMMING_GRANULARITY_CHARACTER;

    use super::preview_title_trimming;

    #[test]
    fn preview_title_trims_at_character_boundaries() {
        assert_eq!(
            preview_title_trimming().granularity,
            DWRITE_TRIMMING_GRANULARITY_CHARACTER
        );
    }
}
