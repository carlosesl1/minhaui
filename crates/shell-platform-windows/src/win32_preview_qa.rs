#![deny(unsafe_code)]

use shell_core::{AppId, WindowId};
use windows::core::Result;

use crate::{ObservedWindow, PreviewCapture, PreviewUnavailableReason};

pub(super) fn seed_restricted_preview_for_qa(observed: &mut Vec<ObservedWindow>) -> Result<()> {
    if std::env::var_os("MINHA_UI_QA_RESTRICTED_PREVIEW").is_none() {
        return Ok(());
    }
    let app = AppId::parse("notepad.exe")
        .map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))?;
    observed.push(
        ObservedWindow::new(WindowId::new(1), app, true, false).with_preview(
            PreviewCapture::restricted(PreviewUnavailableReason::CaptureRestricted),
        ),
    );
    Ok(())
}

const fn invalid_arg() -> windows::core::HRESULT {
    windows::core::HRESULT(0x8007_0057_u32 as i32)
}
