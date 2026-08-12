use std::collections::HashMap;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{GENERIC_READ, SIZE};
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::{
    D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC, ID2D1Bitmap1, ID2D1DeviceContext,
};
use windows::Win32::Graphics::Gdi::{DeleteObject, HBITMAP, HGDIOBJ, HPALETTE};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, IWICImagingFactory,
    WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom, WICBitmapUsePremultipliedAlpha,
    WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SHCreateMemStream, SHFILEINFOW,
    SHGFI_ICON, SHGFI_LARGEICON, SHGetFileInfoW, SIIGBF, SIIGBF_BIGGERSIZEOK, SIIGBF_SCALEUP,
};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, HICON};
use windows::core::{Interface, PCWSTR, Result};

pub(crate) struct NativeIconCache {
    context: ID2D1DeviceContext,
    wic: IWICImagingFactory,
    bitmaps: DomainIconEntries<CachedIcon>,
    encoded_bitmaps: HashMap<u64, ID2D1Bitmap1>,
}

enum CachedIcon {
    Bitmap(ID2D1Bitmap1),
    Missing(Instant),
}

const MISSING_ICON_RETRY: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeIconDomain {
    Shared,
    Dock,
}

struct DomainIconEntries<T> {
    shared: HashMap<String, T>,
    dock: HashMap<String, T>,
}

impl<T> Default for DomainIconEntries<T> {
    fn default() -> Self {
        Self {
            shared: HashMap::new(),
            dock: HashMap::new(),
        }
    }
}

impl<T> DomainIconEntries<T> {
    fn get(&self, domain: NativeIconDomain, source: &str) -> Option<&T> {
        match domain {
            NativeIconDomain::Shared => self.shared.get(source),
            NativeIconDomain::Dock => self.dock.get(source),
        }
    }

    fn map_mut(&mut self, domain: NativeIconDomain) -> &mut HashMap<String, T> {
        match domain {
            NativeIconDomain::Shared => &mut self.shared,
            NativeIconDomain::Dock => &mut self.dock,
        }
    }
}

fn retain_domain_entries<T>(
    entries: &mut DomainIconEntries<T>,
    domain: NativeIconDomain,
    mut is_active: impl FnMut(&str) -> bool,
) {
    entries
        .map_mut(domain)
        .retain(|source, _| is_active(source.as_str()));
}

impl NativeIconCache {
    pub(crate) fn new(context: &ID2D1DeviceContext) -> Result<Self> {
        // SAFETY: Category 8 (FFI boundary). COM is initialized on the UI thread;
        // the CLSID and requested interface are the documented WIC factory pair.
        let wic =
            unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }?;
        Ok(Self {
            context: context.clone(),
            wic,
            bitmaps: DomainIconEntries::default(),
            encoded_bitmaps: HashMap::new(),
        })
    }

    pub(crate) fn draw(&mut self, source: &str, destination: D2D_RECT_F) -> bool {
        self.draw_in_domain(NativeIconDomain::Shared, source, destination)
    }

    pub(crate) fn draw_dock(&mut self, source: &str, destination: D2D_RECT_F) -> bool {
        self.draw_in_domain(NativeIconDomain::Dock, source, destination)
    }

    fn draw_in_domain(
        &mut self,
        domain: NativeIconDomain,
        source: &str,
        destination: D2D_RECT_F,
    ) -> bool {
        let Some(bitmap) = self.bitmap_for(domain, source) else {
            return false;
        };
        // SAFETY: Category 8 (FFI boundary). The cached bitmap belongs to this
        // device context and the finite destination is live for the draw call.
        unsafe {
            self.context.DrawBitmap(
                &bitmap,
                Some(&destination),
                1.0,
                D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC,
                None,
                None,
            )
        };
        true
    }

    pub(crate) fn draw_contained(&mut self, source: &str, optical_bounds: D2D_RECT_F) -> bool {
        let Some(bitmap) = self.bitmap_for(NativeIconDomain::Shared, source) else {
            return false;
        };
        // SAFETY: The cached bitmap belongs to this device context and the
        // returned size is read synchronously before the draw call.
        let size = unsafe { bitmap.GetSize() };
        let Some(destination) = aspect_fit_rect(size.width, size.height, optical_bounds) else {
            return false;
        };
        // SAFETY: The cached bitmap belongs to this device context and the
        // finite destination is contained within the requested optical box.
        unsafe {
            self.context.DrawBitmap(
                &bitmap,
                Some(&destination),
                1.0,
                D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC,
                None,
                None,
            )
        };
        true
    }

    pub(crate) fn retain_dock(&mut self, is_active: impl FnMut(&str) -> bool) {
        retain_domain_entries(&mut self.bitmaps, NativeIconDomain::Dock, is_active);
    }

    pub(crate) fn draw_encoded(
        &mut self,
        generation: u64,
        encoded: &[u8],
        destination: D2D_RECT_F,
    ) -> bool {
        if !self.encoded_bitmaps.contains_key(&generation) {
            let Ok(bitmap) = self.load_encoded(encoded) else {
                return false;
            };
            self.encoded_bitmaps.insert(generation, bitmap);
        }
        let Some(bitmap) = self.encoded_bitmaps.get(&generation) else {
            return false;
        };
        // SAFETY: the bitmap belongs to this device context and the destination is finite.
        unsafe {
            self.context.DrawBitmap(
                bitmap,
                Some(&destination),
                1.0,
                D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC,
                None,
                None,
            )
        };
        true
    }

    pub(crate) fn retain_encoded(&mut self, active_generations: &[u64]) {
        self.encoded_bitmaps
            .retain(|generation, _| active_generations.contains(generation));
    }

    fn bitmap_for(&mut self, domain: NativeIconDomain, source: &str) -> Option<ID2D1Bitmap1> {
        let reload = match self.bitmaps.get(domain, source) {
            Some(CachedIcon::Bitmap(_)) => false,
            Some(CachedIcon::Missing(attempted_at)) => attempted_at.elapsed() >= MISSING_ICON_RETRY,
            None => true,
        };
        if reload {
            let icon = self
                .load(source)
                .map_or_else(|_| CachedIcon::Missing(Instant::now()), CachedIcon::Bitmap);
            self.bitmaps.map_mut(domain).insert(source.to_owned(), icon);
        }
        match self.bitmaps.get(domain, source) {
            Some(CachedIcon::Bitmap(bitmap)) => Some(bitmap.clone()),
            Some(CachedIcon::Missing(_)) | None => None,
        }
    }

    fn load(&self, source: &str) -> Result<ID2D1Bitmap1> {
        let bitmap: IWICBitmapSource = if let Some(path) = source.strip_prefix("image:") {
            let wide = path.encode_utf16().chain([0]).collect::<Vec<_>>();
            // SAFETY: Category 8 (FFI boundary). The package image path is
            // null-terminated and WIC owns the decoder and returned frame.
            let decoder = unsafe {
                self.wic.CreateDecoderFromFilename(
                    PCWSTR(wide.as_ptr()),
                    None,
                    GENERIC_READ,
                    WICDecodeMetadataCacheOnDemand,
                )
            }?;
            // SAFETY: Category 8 (FFI boundary). Package logos are single-frame
            // images and the decoder remains live through frame acquisition.
            unsafe { decoder.GetFrame(0) }?.cast()?
        } else if source
            .get(.."shell:appsfolder".len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("shell:appsfolder"))
        {
            let image = ShellBitmapHandle::load(source)?;
            // SAFETY: Category 8 (FFI boundary). The bitmap guard remains alive
            // through the synchronous WIC copy and the source uses premultiplied alpha.
            let bitmap = unsafe {
                self.wic.CreateBitmapFromHBITMAP(
                    image.0,
                    HPALETTE::default(),
                    WICBitmapUsePremultipliedAlpha,
                )
            }?;
            bitmap.cast()?
        } else {
            let icon = IconHandle::load(source)?;
            // SAFETY: Category 8 (FFI boundary). The icon guard remains alive
            // through the synchronous WIC bitmap creation.
            let bitmap = unsafe { self.wic.CreateBitmapFromHICON(icon.0) }?;
            bitmap.cast()?
        };
        self.convert_wic_bitmap(&bitmap)
    }

    fn load_encoded(&self, encoded: &[u8]) -> Result<ID2D1Bitmap1> {
        // SAFETY: the buffer remains live through the synchronous WIC decode.
        let stream = unsafe { SHCreateMemStream(Some(encoded)) }
            .ok_or_else(windows::core::Error::from_thread)?;
        // SAFETY: the COM stream is valid and WIC returns an owned decoder.
        let decoder = unsafe {
            self.wic.CreateDecoderFromStream(
                &stream,
                core::ptr::null(),
                WICDecodeMetadataCacheOnDemand,
            )
        }?;
        // SAFETY: media artwork is decoded from its first frame while the decoder is live.
        let frame: IWICBitmapSource = unsafe { decoder.GetFrame(0) }?.cast()?;
        self.convert_wic_bitmap(&frame)
    }

    fn convert_wic_bitmap(&self, bitmap: &IWICBitmapSource) -> Result<ID2D1Bitmap1> {
        // SAFETY: Category 8 (FFI boundary). The factory returns an owned converter.
        let converter = unsafe { self.wic.CreateFormatConverter() }?;
        // SAFETY: Category 8 (FFI boundary). Source and converter remain live;
        // the fixed PBGRA format matches the composition swap chain alpha mode.
        unsafe {
            converter.Initialize(
                bitmap,
                &GUID_WICPixelFormat32bppPBGRA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )
        }?;
        // SAFETY: Category 8 (FFI boundary). The converter is a live WIC source
        // and Direct2D retains the returned device-dependent bitmap.
        unsafe { self.context.CreateBitmapFromWicBitmap(&converter, None) }
    }
}

#[must_use]
fn aspect_fit_rect(
    source_width: f32,
    source_height: f32,
    destination: D2D_RECT_F,
) -> Option<D2D_RECT_F> {
    let destination_width = destination.right - destination.left;
    let destination_height = destination.bottom - destination.top;
    if !source_width.is_finite()
        || !source_height.is_finite()
        || source_width <= 0.0
        || source_height <= 0.0
        || !destination.left.is_finite()
        || !destination.top.is_finite()
        || !destination_width.is_finite()
        || !destination_height.is_finite()
        || destination_width <= 0.0
        || destination_height <= 0.0
    {
        return None;
    }
    let scale = (destination_width / source_width).min(destination_height / source_height);
    let width = source_width * scale;
    let height = source_height * scale;
    Some(D2D_RECT_F {
        left: destination.left + (destination_width - width) / 2.0,
        top: destination.top + (destination_height - height) / 2.0,
        right: destination.left + (destination_width + width) / 2.0,
        bottom: destination.top + (destination_height + height) / 2.0,
    })
}

struct ShellBitmapHandle(HBITMAP);

impl ShellBitmapHandle {
    fn load(source: &str) -> Result<Self> {
        let wide = source.encode_utf16().chain([0]).collect::<Vec<_>>();
        // SAFETY: Category 8 (FFI boundary). The parsing name is null-terminated;
        // COM is initialized and returns an owned Shell image factory interface.
        let factory: IShellItemImageFactory =
            unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None) }?;
        let flags = SIIGBF(SIIGBF_BIGGERSIZEOK.0 | SIIGBF_SCALEUP.0);
        // SAFETY: Category 8 (FFI boundary). The Shell factory returns an owned
        // 64px bitmap suitable for the dock's largest supported icon size.
        let bitmap = unsafe { factory.GetImage(SIZE { cx: 64, cy: 64 }, flags) }?;
        Ok(Self(bitmap))
    }
}

impl Drop for ShellBitmapHandle {
    fn drop(&mut self) {
        // SAFETY: Category 8 (FFI boundary). GetImage transferred this GDI object
        // and the guard deletes it exactly once after WIC finishes copying it.
        let _ = unsafe { DeleteObject(HGDIOBJ(self.0.0)) };
    }
}

struct IconHandle(HICON);

impl IconHandle {
    fn load(source: &str) -> Result<Self> {
        let wide = source.encode_utf16().chain([0]).collect::<Vec<_>>();
        let mut info = SHFILEINFOW::default();
        // SAFETY: Category 8 (FFI boundary). The path is null-terminated and the
        // output structure is writable for the synchronous Shell query.
        let result = unsafe {
            SHGetFileInfoW(
                PCWSTR(wide.as_ptr()),
                FILE_FLAGS_AND_ATTRIBUTES(0),
                Some(&mut info),
                std::mem::size_of::<SHFILEINFOW>() as u32,
                SHGFI_ICON | SHGFI_LARGEICON,
            )
        };
        if result == 0 || info.hIcon.0.is_null() {
            return Err(windows::core::Error::from_thread());
        }
        Ok(Self(info.hIcon))
    }
}

impl Drop for IconHandle {
    fn drop(&mut self) {
        // SAFETY: Category 8 (FFI boundary). SHGetFileInfoW transferred ownership
        // of this icon handle and it is destroyed exactly once by the guard.
        let _ = unsafe { DestroyIcon(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;

    use super::{DomainIconEntries, NativeIconDomain, aspect_fit_rect, retain_domain_entries};

    const OPTICAL_BOX: D2D_RECT_F = D2D_RECT_F {
        left: 0.0,
        top: 0.0,
        right: 28.0,
        bottom: 28.0,
    };

    #[test]
    fn retaining_dock_icons_preserves_shared_icons() {
        let mut entries = DomainIconEntries::default();
        entries
            .map_mut(NativeIconDomain::Shared)
            .insert("popover".to_owned(), 1);
        entries
            .map_mut(NativeIconDomain::Dock)
            .insert("active".to_owned(), 2);
        entries
            .map_mut(NativeIconDomain::Dock)
            .insert("stale".to_owned(), 3);

        retain_domain_entries(&mut entries, NativeIconDomain::Dock, |source| {
            source == "active"
        });

        assert_eq!(entries.get(NativeIconDomain::Shared, "popover"), Some(&1));
        assert_eq!(entries.get(NativeIconDomain::Dock, "active"), Some(&2));
        assert_eq!(entries.get(NativeIconDomain::Dock, "stale"), None);
    }

    #[test]
    fn aspect_fit_keeps_square_source_centered_in_optical_box() {
        assert_eq!(aspect_fit_rect(64.0, 64.0, OPTICAL_BOX), Some(OPTICAL_BOX));
    }

    #[test]
    fn aspect_fit_letterboxes_landscape_source() {
        assert_eq!(
            aspect_fit_rect(200.0, 100.0, OPTICAL_BOX),
            Some(D2D_RECT_F {
                left: 0.0,
                top: 7.0,
                right: 28.0,
                bottom: 21.0,
            })
        );
    }

    #[test]
    fn aspect_fit_pillarboxes_portrait_source() {
        assert_eq!(
            aspect_fit_rect(100.0, 200.0, OPTICAL_BOX),
            Some(D2D_RECT_F {
                left: 7.0,
                top: 0.0,
                right: 21.0,
                bottom: 28.0,
            })
        );
    }

    #[test]
    fn aspect_fit_rejects_invalid_source_or_destination_dimensions() {
        assert_eq!(aspect_fit_rect(0.0, 64.0, OPTICAL_BOX), None);
        assert_eq!(aspect_fit_rect(f32::NAN, 64.0, OPTICAL_BOX), None);
        assert_eq!(
            aspect_fit_rect(
                64.0,
                64.0,
                D2D_RECT_F {
                    left: 28.0,
                    top: 0.0,
                    right: 0.0,
                    bottom: 28.0,
                }
            ),
            None
        );
    }
}
