#![deny(unsafe_code)]

#[must_use]
pub const fn crate_identity() -> &'static str {
    "shell-renderer"
}
