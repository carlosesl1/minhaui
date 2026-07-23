use crate::DipRect;
use crate::native::DeviceKind;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_BITMAP_OPTIONS_NONE, D2D1_BITMAP_PROPERTIES1, D2D1_INTERPOLATION_MODE_LINEAR,
    ID2D1Bitmap1, ID2D1DeviceContext,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::core::Result;

const SPECULAR_WIDTH_DIP: f32 = 96.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LiquidGlassMode {
    Disabled,
    Static,
    Dynamic,
}

pub(crate) const fn liquid_glass_mode(
    requested: bool,
    solid_material: bool,
    device_kind: DeviceKind,
    reduced_motion: bool,
) -> LiquidGlassMode {
    if !requested || solid_material {
        LiquidGlassMode::Disabled
    } else if reduced_motion || matches!(device_kind, DeviceKind::Warp) {
        LiquidGlassMode::Static
    } else {
        LiquidGlassMode::Dynamic
    }
}

pub(crate) struct LiquidGlassRaster {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl LiquidGlassRaster {
    pub(crate) fn new(width: f32, height: f32, radius: f32) -> Self {
        let width = raster_dimension(width);
        let height = raster_dimension(height);
        let radius = finite(radius, 0.0).clamp(0.0, width.min(height) as f32 / 2.0);
        let mut pixels = vec![0; width as usize * height as usize * 4];
        for y in 0..height {
            for x in 0..width {
                let point_x = x as f32 + 0.5;
                let point_y = y as f32 + 0.5;
                let distance = rounded_rect_signed_distance(
                    point_x,
                    point_y,
                    width as f32,
                    height as f32,
                    radius,
                );
                if distance > 0.0 {
                    continue;
                }
                let edge = ((distance + 4.0) / 4.0).clamp(0.0, 1.0);
                let horizontal = point_x / width as f32;
                let vertical = point_y / height as f32;
                let top_light = edge * (1.0 - vertical).powf(2.2) * 0.34;
                let inner_rim = edge * 0.16;
                let cool = edge * (1.0 - horizontal).powf(3.0) * 0.10;
                let warm = edge * horizontal.powf(3.0) * 0.075;
                let mut color = [0.0; 4];
                composite(&mut color, [0.62, 0.82, 1.0], cool);
                composite(&mut color, [1.0, 0.72, 0.48], warm);
                composite(&mut color, [1.0, 1.0, 1.0], inner_rim);
                composite(&mut color, [1.0, 1.0, 1.0], top_light);
                write_bgra(&mut pixels, width, x, y, color);
            }
        }
        Self {
            width,
            height,
            pixels,
        }
    }

    pub(crate) fn specular(height: f32, radius: f32) -> Self {
        let width = raster_dimension(SPECULAR_WIDTH_DIP);
        let height = raster_dimension(height);
        let radius = finite(radius, 0.0).clamp(0.0, width.min(height) as f32 / 2.0);
        let mut pixels = vec![0; width as usize * height as usize * 4];
        for y in 0..height {
            for x in 0..width {
                let point_x = x as f32 + 0.5;
                let point_y = y as f32 + 0.5;
                let distance = rounded_rect_signed_distance(
                    point_x,
                    point_y,
                    width as f32,
                    height as f32,
                    radius,
                );
                if distance > 0.0 {
                    continue;
                }
                let horizontal = ((point_x / width as f32) - 0.5).abs() * 2.0;
                let vertical = point_y / height as f32;
                let alpha = (1.0 - horizontal).powf(2.8) * (1.0 - vertical).powf(2.0) * 0.46;
                write_bgra(&mut pixels, width, x, y, [alpha, alpha, alpha, alpha]);
            }
        }
        Self {
            width,
            height,
            pixels,
        }
    }

    pub(crate) const fn width(&self) -> u32 {
        self.width
    }

    pub(crate) const fn height(&self) -> u32 {
        self.height
    }

    pub(crate) const fn stride(&self) -> u32 {
        self.width * 4
    }

    pub(crate) fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

pub(crate) fn specular_left(
    bounds: DipRect,
    hover_position_x: Option<f32>,
    specular_width: f32,
) -> f32 {
    let width = finite(specular_width, SPECULAR_WIDTH_DIP)
        .max(1.0)
        .min(bounds.width.max(1.0));
    let centered = hover_position_x
        .filter(|value| value.is_finite())
        .unwrap_or(bounds.x + bounds.width / 2.0)
        - width / 2.0;
    centered.clamp(bounds.x, bounds.x + bounds.width - width)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct LiquidGlassPresentation {
    overlay_bounds: DipRect,
    overlay_opacity: f32,
    specular_opacity: f32,
    specular_left: f32,
}

fn presentation(
    mode: LiquidGlassMode,
    bounds: DipRect,
    hover_position_x: Option<f32>,
    material_strength: f32,
    specular_width: f32,
) -> LiquidGlassPresentation {
    let strength = if matches!(mode, LiquidGlassMode::Dynamic) {
        finite(material_strength, 0.0).clamp(0.0, 1.0)
    } else {
        0.0
    };
    LiquidGlassPresentation {
        overlay_bounds: bounds,
        overlay_opacity: 0.72 + 0.18 * strength,
        specular_opacity: if matches!(mode, LiquidGlassMode::Dynamic) {
            0.12 + 0.28 * strength
        } else {
            0.0
        },
        specular_left: specular_left(bounds, hover_position_x, specular_width),
    }
}

pub(crate) struct DockLiquidGlassResources {
    overlay: ID2D1Bitmap1,
    specular: ID2D1Bitmap1,
    specular_width: f32,
    mode: LiquidGlassMode,
}

impl DockLiquidGlassResources {
    pub(crate) fn create(
        context: &ID2D1DeviceContext,
        width: f32,
        height: f32,
        radius: f32,
        mode: LiquidGlassMode,
    ) -> Result<Option<Self>> {
        if matches!(mode, LiquidGlassMode::Disabled) {
            return Ok(None);
        }
        let overlay = create_bitmap(context, &LiquidGlassRaster::new(width, height, radius))?;
        let specular_raster = LiquidGlassRaster::specular(height, radius);
        let specular_width = specular_raster.width() as f32;
        let specular = create_bitmap(context, &specular_raster)?;
        Ok(Some(Self {
            overlay,
            specular,
            specular_width,
            mode,
        }))
    }

    pub(crate) fn draw(
        &self,
        context: &ID2D1DeviceContext,
        bounds: DipRect,
        hover_position_x: Option<f32>,
        material_strength: f32,
    ) {
        let frame = presentation(
            self.mode,
            bounds,
            hover_position_x,
            material_strength,
            self.specular_width,
        );
        let overlay = rect(frame.overlay_bounds);
        // SAFETY: Category 8 (FFI boundary). Cached bitmaps belong to this
        // Direct2D device and finite destinations remain live for each draw.
        unsafe {
            context.DrawBitmap(
                &self.overlay,
                Some(&overlay),
                frame.overlay_opacity,
                D2D1_INTERPOLATION_MODE_LINEAR,
                None,
                None,
            )
        };
        if frame.specular_opacity <= 0.0 {
            return;
        }
        let specular = D2D_RECT_F {
            left: frame.specular_left,
            top: bounds.y,
            right: frame.specular_left + self.specular_width.min(bounds.width),
            bottom: bounds.y + bounds.height,
        };
        // SAFETY: Category 8 (FFI boundary). The cached specular bitmap and
        // bounded destination are valid for the synchronous Direct2D draw.
        unsafe {
            context.DrawBitmap(
                &self.specular,
                Some(&specular),
                frame.specular_opacity,
                D2D1_INTERPOLATION_MODE_LINEAR,
                None,
                None,
            )
        };
    }
}

fn create_bitmap(context: &ID2D1DeviceContext, raster: &LiquidGlassRaster) -> Result<ID2D1Bitmap1> {
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
    // SAFETY: Category 8 (FFI boundary). The premultiplied BGRA raster remains
    // live for the synchronous upload and matches its dimensions and stride.
    unsafe {
        context.CreateBitmap(
            D2D_SIZE_U {
                width: raster.width(),
                height: raster.height(),
            },
            Some(raster.pixels().as_ptr().cast()),
            raster.stride(),
            &properties,
        )
    }
}

const fn rect(bounds: DipRect) -> D2D_RECT_F {
    D2D_RECT_F {
        left: bounds.x,
        top: bounds.y,
        right: bounds.x + bounds.width,
        bottom: bounds.y + bounds.height,
    }
}

const fn finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

fn raster_dimension(value: f32) -> u32 {
    finite(value, 1.0).round().clamp(1.0, 4096.0) as u32
}

fn rounded_rect_signed_distance(x: f32, y: f32, width: f32, height: f32, radius: f32) -> f32 {
    let half_width = width / 2.0;
    let half_height = height / 2.0;
    let qx = (x - half_width).abs() - (half_width - radius);
    let qy = (y - half_height).abs() - (half_height - radius);
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius
}

fn composite(destination: &mut [f32; 4], source_rgb: [f32; 3], source_alpha: f32) {
    let source_alpha = source_alpha.clamp(0.0, 1.0);
    let remaining = 1.0 - source_alpha;
    destination[0] = source_rgb[0] * source_alpha + destination[0] * remaining;
    destination[1] = source_rgb[1] * source_alpha + destination[1] * remaining;
    destination[2] = source_rgb[2] * source_alpha + destination[2] * remaining;
    destination[3] = source_alpha + destination[3] * remaining;
}

fn write_bgra(pixels: &mut [u8], width: u32, x: u32, y: u32, color: [f32; 4]) {
    let index = ((y * width + x) * 4) as usize;
    pixels[index] = channel(color[2]);
    pixels[index + 1] = channel(color[1]);
    pixels[index + 2] = channel(color[0]);
    pixels[index + 3] = channel(color[3]);
}

fn channel(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use crate::DipRect;
    use crate::native::DeviceKind;

    use super::{
        LiquidGlassMode, LiquidGlassRaster, liquid_glass_mode, presentation, specular_left,
    };

    #[test]
    fn liquid_glass_selects_dynamic_static_and_disabled_modes() {
        assert_eq!(
            liquid_glass_mode(false, false, DeviceKind::Hardware, false),
            LiquidGlassMode::Disabled
        );
        assert_eq!(
            liquid_glass_mode(true, false, DeviceKind::Hardware, false),
            LiquidGlassMode::Dynamic
        );
        assert_eq!(
            liquid_glass_mode(true, false, DeviceKind::Hardware, true),
            LiquidGlassMode::Static
        );
        assert_eq!(
            liquid_glass_mode(true, false, DeviceKind::Warp, false),
            LiquidGlassMode::Static
        );
        assert_eq!(
            liquid_glass_mode(true, true, DeviceKind::Hardware, false),
            LiquidGlassMode::Disabled
        );
    }

    #[test]
    fn overlay_raster_is_bounded_premultiplied_and_nonempty() {
        let raster = LiquidGlassRaster::new(513.0, 53.0, 19.0);

        assert_eq!(raster.stride(), raster.width() * 4);
        assert_eq!(
            raster.pixels().len(),
            (raster.width() * raster.height() * 4) as usize
        );
        assert!(raster.pixels().chunks_exact(4).any(|pixel| pixel[3] > 0));
        assert!(
            raster.pixels().chunks_exact(4).all(|pixel| {
                pixel[0] <= pixel[3] && pixel[1] <= pixel[3] && pixel[2] <= pixel[3]
            })
        );
    }

    #[test]
    fn dynamic_specular_position_clamps_inside_material_bounds() {
        let bounds = DipRect::new(10.0, 2.0, 500.0, 53.0);

        assert_eq!(specular_left(bounds, Some(-100.0), 96.0), 10.0);
        assert_eq!(specular_left(bounds, Some(900.0), 96.0), 414.0);
        assert_eq!(specular_left(bounds, None, 96.0), 212.0);
    }

    #[test]
    fn static_mode_ignores_hover_and_dynamic_mode_tracks_it() {
        let bounds = DipRect::new(0.0, 2.0, 500.0, 53.0);
        let static_frame = presentation(LiquidGlassMode::Static, bounds, Some(420.0), 1.0, 96.0);
        let dynamic_frame = presentation(LiquidGlassMode::Dynamic, bounds, Some(420.0), 1.0, 96.0);

        assert_eq!(static_frame.specular_opacity, 0.0);
        assert!(dynamic_frame.specular_opacity > 0.0);
        assert!(dynamic_frame.specular_left > bounds.x + bounds.width / 2.0);
    }

    #[test]
    fn liquid_glass_never_changes_material_bounds() {
        let bounds = DipRect::new(3.0, 2.0, 507.0, 53.0);
        let frame = presentation(LiquidGlassMode::Dynamic, bounds, Some(250.0), 1.0, 96.0);

        assert_eq!(frame.overlay_bounds, bounds);
    }
}
