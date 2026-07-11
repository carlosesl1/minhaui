use windows::Win32::Foundation::D2DERR_RECREATE_TARGET;
use windows::Win32::Graphics::Direct3D11::ID3D11Device;
use windows::Win32::Graphics::Dxgi::{
    DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET, DXGI_PRESENT, IDXGISwapChain1,
};
use windows::core::{HRESULT, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceLossKind {
    Removed,
    Reset,
    RecreateTarget,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresentOutcome {
    Presented,
    DeviceLost(DeviceLossKind),
    Failed(HRESULT),
}

#[must_use]
pub const fn classify_present_hresult(code: HRESULT) -> PresentOutcome {
    if code.0 >= 0 {
        PresentOutcome::Presented
    } else if code.0 == DXGI_ERROR_DEVICE_REMOVED.0 {
        PresentOutcome::DeviceLost(DeviceLossKind::Removed)
    } else if code.0 == DXGI_ERROR_DEVICE_RESET.0 {
        PresentOutcome::DeviceLost(DeviceLossKind::Reset)
    } else if code.0 == D2DERR_RECREATE_TARGET.0 {
        PresentOutcome::DeviceLost(DeviceLossKind::RecreateTarget)
    } else {
        PresentOutcome::Failed(code)
    }
}

#[must_use]
pub const fn is_recoverable_hresult(code: HRESULT) -> bool {
    matches!(
        classify_present_hresult(code),
        PresentOutcome::DeviceLost(_)
    )
}

pub const fn device_loss_hresult(kind: DeviceLossKind) -> HRESULT {
    match kind {
        DeviceLossKind::Removed => DXGI_ERROR_DEVICE_REMOVED,
        DeviceLossKind::Reset => DXGI_ERROR_DEVICE_RESET,
        DeviceLossKind::RecreateTarget => D2DERR_RECREATE_TARGET,
    }
}

pub(crate) fn present_swap_chain(
    swap_chain: &IDXGISwapChain1,
    device: &ID3D11Device,
) -> Result<PresentOutcome> {
    // SAFETY: Category 8 (FFI boundary). The owned swap chain is live and default
    // presentation flags do not carry additional pointers.
    let code = unsafe { swap_chain.Present(1, DXGI_PRESENT(0)) };
    let outcome = classify_present_hresult(code);
    if let PresentOutcome::DeviceLost(kind) = outcome {
        // SAFETY: Category 8 (FFI boundary). The D3D11 device is retained by the
        // surface and exposes a diagnostic HRESULT without returning user data.
        let reason = unsafe { device.GetDeviceRemovedReason() }
            .err()
            .map_or(HRESULT(0), |error| error.code());
        eprintln!("DEVICE_LOST kind={kind:?} reason=0x{:08X}", reason.0 as u32);
    }
    match outcome {
        PresentOutcome::Failed(error) => Err(windows::core::Error::from_hresult(error)),
        value => Ok(value),
    }
}
