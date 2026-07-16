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
    IShellItemImageFactory, SHCreateItemFromParsingName, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON,
    SHGetFileInfoW, SIIGBF, SIIGBF_BIGGERSIZEOK, SIIGBF_SCALEUP,
};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, HICON};
use windows::core::{Interface, PCWSTR, Result};

pub(crate) struct NativeIconCache {
    context: ID2D1DeviceContext,
    wic: IWICImagingFactory,
    bitmaps: HashMap<String, CachedIcon>,
}

enum CachedIcon {
    Bitmap(ID2D1Bitmap1),
    Missing(Instant),
}

const MISSING_ICON_RETRY: Duration = Duration::from_secs(5);

impl NativeIconCache {
    pub(crate) fn new(context: &ID2D1DeviceContext) -> Result<Self> {
        // SAFETY: Category 8 (FFI boundary). COM is initialized on the UI thread;
        // the CLSID and requested interface are the documented WIC factory pair.
        let wic =
            unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }?;
        Ok(Self {
            context: context.clone(),
            wic,
            bitmaps: HashMap::new(),
        })
    }

    pub(crate) fn draw(&mut self, source: &str, destination: D2D_RECT_F) -> bool {
        let reload = match self.bitmaps.get(source) {
            Some(CachedIcon::Bitmap(_)) => false,
            Some(CachedIcon::Missing(attempted_at)) => attempted_at.elapsed() >= MISSING_ICON_RETRY,
            None => true,
        };
        if reload {
            let icon = self
                .load(source)
                .map_or_else(|_| CachedIcon::Missing(Instant::now()), CachedIcon::Bitmap);
            self.bitmaps.insert(source.to_owned(), icon);
        }
        let Some(CachedIcon::Bitmap(bitmap)) = self.bitmaps.get(source) else {
            return false;
        };
        // SAFETY: Category 8 (FFI boundary). The cached bitmap belongs to this
        // device context and the finite destination is live for the draw call.
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

    pub(crate) fn retain(&mut self, active_sources: &[&str]) {
        self.bitmaps
            .retain(|source, _| active_sources.contains(&source.as_str()));
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
        // SAFETY: Category 8 (FFI boundary). The factory returns an owned converter.
        let converter = unsafe { self.wic.CreateFormatConverter() }?;
        // SAFETY: Category 8 (FFI boundary). Source and converter remain live;
        // the fixed PBGRA format matches the composition swap chain alpha mode.
        unsafe {
            converter.Initialize(
                &bitmap,
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
