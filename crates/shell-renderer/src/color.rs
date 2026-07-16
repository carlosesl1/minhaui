#![deny(unsafe_code)]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba8 {
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

#[must_use]
#[cfg(test)]
pub const fn premultiply_srgb(color: Rgba8) -> Rgba8 {
    Rgba8::new(
        premultiply_channel(color.r, color.a),
        premultiply_channel(color.g, color.a),
        premultiply_channel(color.b, color.a),
        color.a,
    )
}

#[cfg(test)]
const fn premultiply_channel(channel: u8, alpha: u8) -> u8 {
    ((channel as u16 * alpha as u16 + 127) / 255) as u8
}
