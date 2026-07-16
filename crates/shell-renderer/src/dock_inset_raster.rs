use crate::{DockInsetShadow, dock_inset_shadows};

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct InsetShadowRaster {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl InsetShadowRaster {
    pub(crate) fn new(width: f32, height: f32, radius: f32) -> Self {
        let width = width.ceil().max(1.0) as u32;
        let height = height.ceil().max(1.0) as u32;
        let mut pixels = vec![0; width as usize * height as usize * 4];
        let shadows = dock_inset_shadows();
        let body = RoundedBody {
            left: 1.0,
            top: 1.0,
            right: width as f32 - 1.0,
            bottom: height as f32 - 1.0,
            radius: (radius - 0.5).max(0.0),
        };

        for y in 0..height {
            for x in 0..width {
                let point_x = x as f32 + 0.5;
                let point_y = y as f32 + 0.5;
                let body_coverage = (0.5 - body.signed_distance(point_x, point_y)).clamp(0.0, 1.0);
                if body_coverage == 0.0 {
                    continue;
                }
                let mut output = [0.0; 4];
                for shadow in shadows.iter().rev() {
                    composite_shadow(&mut output, body, point_x, point_y, *shadow);
                }
                for channel in &mut output {
                    *channel *= body_coverage;
                }
                write_premultiplied_bgra(&mut pixels, width, x, y, output);
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

    pub(crate) fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

#[derive(Clone, Copy)]
struct RoundedBody {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    radius: f32,
}

impl RoundedBody {
    fn signed_distance(self, x: f32, y: f32) -> f32 {
        let center_x = (self.left + self.right) * 0.5;
        let center_y = (self.top + self.bottom) * 0.5;
        let half_width = (self.right - self.left) * 0.5;
        let half_height = (self.bottom - self.top) * 0.5;
        let radius = self.radius.min(half_width).min(half_height);
        let qx = (x - center_x).abs() - (half_width - radius);
        let qy = (y - center_y).abs() - (half_height - radius);
        let outside = qx.max(0.0).hypot(qy.max(0.0));
        outside + qx.max(qy).min(0.0) - radius
    }
}

fn composite_shadow(
    output: &mut [f32; 4],
    body: RoundedBody,
    x: f32,
    y: f32,
    shadow: DockInsetShadow,
) {
    let shifted_distance = body.signed_distance(x, y - shadow.offset_y());
    let sigma = (shadow.blur() * 0.5).max(0.25);
    let coverage = normal_cdf((shifted_distance + shadow.spread()) / sigma);
    let source_alpha = coverage * f32::from(shadow.color().a) / 255.0;
    let inverse = 1.0 - source_alpha;
    let color = shadow.color();
    output[0] = f32::from(color.r) / 255.0 * source_alpha + output[0] * inverse;
    output[1] = f32::from(color.g) / 255.0 * source_alpha + output[1] * inverse;
    output[2] = f32::from(color.b) / 255.0 * source_alpha + output[2] * inverse;
    output[3] = source_alpha + output[3] * inverse;
}

fn normal_cdf(value: f32) -> f32 {
    let scaled = 0.797_884_6 * (value + 0.044_715 * value.powi(3));
    0.5 * (1.0 + scaled.tanh())
}

fn write_premultiplied_bgra(pixels: &mut [u8], width: u32, x: u32, y: u32, color: [f32; 4]) {
    let index = (y as usize * width as usize + x as usize) * 4;
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
    use super::InsetShadowRaster;

    #[test]
    fn raster_clips_corners_and_keeps_symmetric_edge_falloff() {
        let raster = InsetShadowRaster::new(55.0, 55.0, 15.0);
        assert_eq!(alpha(&raster, 0, 0), 0);
        assert_eq!(alpha(&raster, 54, 0), 0);
        assert_eq!(alpha(&raster, 27, 2), alpha(&raster, 27, 52));
        assert!(alpha(&raster, 27, 2) > alpha(&raster, 27, 27));
        assert!(alpha(&raster, 2, 27) > alpha(&raster, 27, 27));
    }

    #[test]
    fn raster_blends_shadow_alpha_across_the_rounded_boundary() {
        let raster = InsetShadowRaster::new(55.0, 55.0, 15.0);
        let outside = alpha(&raster, 9, 1);
        let boundary = alpha(&raster, 11, 1);
        let inside = alpha(&raster, 12, 1);

        assert_eq!(outside, 0);
        assert!(boundary > outside);
        assert!(boundary < inside);
    }

    fn alpha(raster: &InsetShadowRaster, x: u32, y: u32) -> u8 {
        let index = (y as usize * raster.width() as usize + x as usize) * 4;
        raster.pixels()[index + 3]
    }
}
