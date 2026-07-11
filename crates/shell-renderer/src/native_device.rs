use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP, D3D_FEATURE_LEVEL_11_0,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
};
use windows::core::Result;

use crate::native::DeviceKind;

pub(crate) fn create_d3d_device(force_warp: bool) -> Result<(ID3D11Device, DeviceKind)> {
    let requested = if force_warp {
        DeviceKind::Warp
    } else {
        DeviceKind::Hardware
    };
    match try_create_d3d(requested) {
        Ok(device) => Ok((device, requested)),
        Err(_) if requested == DeviceKind::Hardware => {
            try_create_d3d(DeviceKind::Warp).map(|device| (device, DeviceKind::Warp))
        }
        Err(error) => Err(error),
    }
}

fn try_create_d3d(kind: DeviceKind) -> Result<ID3D11Device> {
    let mut device = None;
    let driver = match kind {
        DeviceKind::Hardware => D3D_DRIVER_TYPE_HARDWARE,
        DeviceKind::Warp => D3D_DRIVER_TYPE_WARP,
    };
    // SAFETY: Category 8 (FFI boundary). Output storage is valid for the call; no
    // adapter or software module is supplied for hardware or WARP drivers.
    unsafe {
        D3D11CreateDevice(
            None,
            driver,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&[D3D_FEATURE_LEVEL_11_0]),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        )
    }?;
    device.ok_or_else(|| {
        windows::core::Error::new(
            windows::core::HRESULT(-2_147_467_259),
            "D3D11 returned no device",
        )
    })
}
