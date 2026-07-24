use std::ops::Range;

use thiserror::Error;

pub(crate) const MAX_NATIVE_TRAY_CANDIDATES: usize = 256;
pub(crate) const MAX_TRAY_TOOLTIP_CHARS: usize = 160;

const WM_USER: u32 = 0x0400;
const X64_TBBUTTON_SIZE: usize = 32;
const X64_TRAY_DATA_SIZE: usize = 32;
const POINTER_BYTES: usize = 8;
const UTF16_CODE_UNIT_BYTES: usize = 2;

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum TrayDecodeError {
    #[error("the current pointer width does not support the x64 tray layout")]
    UnsupportedLayout,
    #[error("{structure} is truncated: expected at least {expected} bytes, got {actual}")]
    TruncatedStructure {
        structure: &'static str,
        expected: usize,
        actual: usize,
    },
    #[error("{0} is null")]
    NullPointer(&'static str),
    #[error("{0} is outside the validated remote range")]
    PointerOutOfRange(&'static str),
    #[error("the tray owner window is null")]
    InvalidOwnerWindow,
    #[error("the tray callback message is below WM_USER")]
    InvalidCallbackMessage,
    #[error("the tray owner process ID is zero")]
    InvalidOwnerProcessId,
    #[error("the tray tooltip exceeds {MAX_TRAY_TOOLTIP_CHARS} UTF-16 characters")]
    TooltipTooLong,
    #[error("the tray tooltip is not valid UTF-16")]
    InvalidTooltipEncoding,
    #[error("the bounded tooltip span size overflowed")]
    TooltipSpanOverflow,
    #[error("the remote tray-data address did not match the validated dwData pointer")]
    TrayDataAddressMismatch { expected: usize, actual: usize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TrayTooltipSpan {
    pointer: usize,
    byte_len: usize,
}

impl TrayTooltipSpan {
    #[must_use]
    pub(crate) const fn pointer(self) -> usize {
        self.pointer
    }

    #[must_use]
    pub(crate) const fn byte_len(self) -> usize {
        self.byte_len
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DecodedTrayButton {
    tray_data_pointer: usize,
    tooltip_span: Option<TrayTooltipSpan>,
}

impl DecodedTrayButton {
    #[must_use]
    pub(crate) const fn tray_data_pointer(self) -> usize {
        self.tray_data_pointer
    }

    #[must_use]
    pub(crate) const fn tooltip_span(self) -> Option<TrayTooltipSpan> {
        self.tooltip_span
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DecodedTrayRecord {
    pub(crate) owner_window: usize,
    pub(crate) icon_id: u32,
    pub(crate) callback_message: u32,
    pub(crate) icon_handle: usize,
    pub(crate) tooltip_pointer: usize,
}

impl DecodedTrayRecord {
    #[must_use]
    pub(crate) const fn owner_window(self) -> usize {
        self.owner_window
    }

    #[must_use]
    pub(crate) const fn icon_id(self) -> u32 {
        self.icon_id
    }

    #[must_use]
    pub(crate) const fn callback_message(self) -> u32 {
        self.callback_message
    }

    #[must_use]
    pub(crate) const fn icon_handle(self) -> usize {
        self.icon_handle
    }

    #[must_use]
    pub(crate) const fn tooltip_pointer(self) -> usize {
        self.tooltip_pointer
    }
}

pub(crate) fn decode_x64_tray_record(
    button: &[u8],
    tray_data: &[u8],
    valid_remote_range: Range<usize>,
) -> Result<DecodedTrayRecord, TrayDecodeError> {
    let decoded_button = decode_x64_button(button, valid_remote_range)?;
    let tray_data_pointer = decoded_button.tray_data_pointer();
    decode_x64_tray_data(decoded_button, tray_data_pointer, tray_data)
}

pub(crate) fn decode_x64_button(
    button: &[u8],
    valid_remote_range: Range<usize>,
) -> Result<DecodedTrayButton, TrayDecodeError> {
    validate_pointer_width(usize::BITS)?;
    ensure_minimum_size(button, X64_TBBUTTON_SIZE, "TBBUTTON")?;

    let tray_data_pointer = read_pointer(button, 16, "TBBUTTON")?;
    if tray_data_pointer == 0 {
        return Err(TrayDecodeError::NullPointer("dwData"));
    }
    ensure_remote_pointer(
        tray_data_pointer,
        X64_TRAY_DATA_SIZE,
        "dwData",
        &valid_remote_range,
    )?;

    let tooltip_pointer = read_pointer(button, 24, "TBBUTTON")?;
    let tooltip_span = if tooltip_pointer == 0 {
        None
    } else {
        let byte_len = bounded_tooltip_read_bytes()?;
        ensure_remote_pointer(tooltip_pointer, byte_len, "iString", &valid_remote_range)?;
        Some(TrayTooltipSpan {
            pointer: tooltip_pointer,
            byte_len,
        })
    };

    Ok(DecodedTrayButton {
        tray_data_pointer,
        tooltip_span,
    })
}

pub(crate) fn decode_x64_tray_data(
    decoded_button: DecodedTrayButton,
    actual_tray_data_address: usize,
    tray_data: &[u8],
) -> Result<DecodedTrayRecord, TrayDecodeError> {
    if actual_tray_data_address != decoded_button.tray_data_pointer {
        return Err(TrayDecodeError::TrayDataAddressMismatch {
            expected: decoded_button.tray_data_pointer,
            actual: actual_tray_data_address,
        });
    }

    ensure_minimum_size(tray_data, X64_TRAY_DATA_SIZE, "TRAYDATA")?;

    let owner_window = read_pointer(tray_data, 0, "TRAYDATA")?;
    if owner_window == 0 {
        return Err(TrayDecodeError::InvalidOwnerWindow);
    }

    let icon_id = read_u32(tray_data, 8, "TRAYDATA")?;
    let callback_message = read_u32(tray_data, 12, "TRAYDATA")?;
    if callback_message < WM_USER {
        return Err(TrayDecodeError::InvalidCallbackMessage);
    }
    let icon_handle = read_pointer(tray_data, 24, "TRAYDATA")?;

    Ok(DecodedTrayRecord {
        owner_window,
        icon_id,
        callback_message,
        icon_handle,
        tooltip_pointer: decoded_button
            .tooltip_span
            .map_or(0, TrayTooltipSpan::pointer),
    })
}

pub(crate) const fn bounded_candidate_count(candidate_count: usize) -> usize {
    if candidate_count > MAX_NATIVE_TRAY_CANDIDATES {
        MAX_NATIVE_TRAY_CANDIDATES
    } else {
        candidate_count
    }
}

pub(crate) const fn validate_pointer_width(bits: u32) -> Result<(), TrayDecodeError> {
    if bits == 64 {
        Ok(())
    } else {
        Err(TrayDecodeError::UnsupportedLayout)
    }
}

pub(crate) const fn validate_owner_process_id(process_id: u32) -> Result<(), TrayDecodeError> {
    if process_id == 0 {
        Err(TrayDecodeError::InvalidOwnerProcessId)
    } else {
        Ok(())
    }
}

pub(crate) fn validate_bounded_tooltip_length(tooltip: &[u16]) -> Result<usize, TrayDecodeError> {
    let length = tooltip
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(tooltip.len());

    if length > MAX_TRAY_TOOLTIP_CHARS {
        Err(TrayDecodeError::TooltipTooLong)
    } else {
        Ok(length)
    }
}

pub(crate) fn decode_bounded_tooltip(tooltip: &[u16]) -> Result<String, TrayDecodeError> {
    let length = validate_bounded_tooltip_length(tooltip)?;
    String::from_utf16(&tooltip[..length]).map_err(|_| TrayDecodeError::InvalidTooltipEncoding)
}

fn ensure_minimum_size(
    bytes: &[u8],
    minimum: usize,
    structure: &'static str,
) -> Result<(), TrayDecodeError> {
    if bytes.len() < minimum {
        return Err(TrayDecodeError::TruncatedStructure {
            structure,
            expected: minimum,
            actual: bytes.len(),
        });
    }
    Ok(())
}

fn bounded_tooltip_read_bytes() -> Result<usize, TrayDecodeError> {
    let code_units = MAX_TRAY_TOOLTIP_CHARS
        .checked_add(1)
        .ok_or(TrayDecodeError::TooltipSpanOverflow)?;
    code_units
        .checked_mul(UTF16_CODE_UNIT_BYTES)
        .ok_or(TrayDecodeError::TooltipSpanOverflow)
}

fn read_u32(bytes: &[u8], offset: usize, structure: &'static str) -> Result<u32, TrayDecodeError> {
    let width = std::mem::size_of::<u32>();
    let end = offset
        .checked_add(width)
        .ok_or(TrayDecodeError::TruncatedStructure {
            structure,
            expected: offset,
            actual: bytes.len(),
        })?;
    let raw = bytes
        .get(offset..end)
        .ok_or(TrayDecodeError::TruncatedStructure {
            structure,
            expected: end,
            actual: bytes.len(),
        })?;
    let raw: [u8; 4] = raw
        .try_into()
        .map_err(|_| TrayDecodeError::TruncatedStructure {
            structure,
            expected: end,
            actual: bytes.len(),
        })?;
    Ok(u32::from_le_bytes(raw))
}

fn read_pointer(
    bytes: &[u8],
    offset: usize,
    structure: &'static str,
) -> Result<usize, TrayDecodeError> {
    let end = offset
        .checked_add(POINTER_BYTES)
        .ok_or(TrayDecodeError::TruncatedStructure {
            structure,
            expected: offset,
            actual: bytes.len(),
        })?;
    let raw = bytes
        .get(offset..end)
        .ok_or(TrayDecodeError::TruncatedStructure {
            structure,
            expected: end,
            actual: bytes.len(),
        })?;
    let raw: [u8; 8] = raw
        .try_into()
        .map_err(|_| TrayDecodeError::TruncatedStructure {
            structure,
            expected: end,
            actual: bytes.len(),
        })?;
    usize::try_from(u64::from_le_bytes(raw)).map_err(|_| TrayDecodeError::UnsupportedLayout)
}

fn ensure_remote_pointer(
    pointer: usize,
    bytes: usize,
    field: &'static str,
    valid_remote_range: &Range<usize>,
) -> Result<(), TrayDecodeError> {
    let Some(end) = pointer.checked_add(bytes) else {
        return Err(TrayDecodeError::PointerOutOfRange(field));
    };
    if valid_remote_range.start > valid_remote_range.end
        || pointer < valid_remote_range.start
        || end > valid_remote_range.end
    {
        return Err(TrayDecodeError::PointerOutOfRange(field));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        DecodedTrayButton, DecodedTrayRecord, MAX_NATIVE_TRAY_CANDIDATES, MAX_TRAY_TOOLTIP_CHARS,
        TrayDecodeError, bounded_candidate_count, decode_bounded_tooltip, decode_x64_button,
        decode_x64_tray_data, decode_x64_tray_record, read_pointer, read_u32,
        validate_bounded_tooltip_length, validate_owner_process_id, validate_pointer_width,
    };

    const VALID_REMOTE_RANGE: std::ops::Range<usize> = 0x1000..0x10_000;
    const BOUNDED_TOOLTIP_BYTES: usize = (MAX_TRAY_TOOLTIP_CHARS + 1) * 2;

    fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
        let encoded = value.to_le_bytes();
        let destination = &mut bytes[offset..offset + encoded.len()];
        destination.copy_from_slice(&encoded);
    }

    fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
        let encoded = value.to_le_bytes();
        let destination = &mut bytes[offset..offset + encoded.len()];
        destination.copy_from_slice(&encoded);
    }

    fn write_pointer(bytes: &mut [u8], offset: usize, value: usize) {
        let Ok(encoded) = u64::try_from(value) else {
            return;
        };
        write_u64(bytes, offset, encoded);
    }

    fn button(dw_data: usize, tooltip_pointer: usize) -> Vec<u8> {
        let mut bytes = vec![0_u8; 32];
        write_pointer(&mut bytes, 16, dw_data);
        write_pointer(&mut bytes, 24, tooltip_pointer);
        bytes
    }

    fn tray_data(owner_window: usize, icon_id: u32, callback_message: u32, icon: usize) -> Vec<u8> {
        let mut bytes = vec![0_u8; 32];
        write_pointer(&mut bytes, 0, owner_window);
        write_u32(&mut bytes, 8, icon_id);
        write_u32(&mut bytes, 12, callback_message);
        write_pointer(&mut bytes, 24, icon);
        bytes
    }

    fn valid_record() -> Result<DecodedTrayRecord, TrayDecodeError> {
        decode_x64_tray_record(
            &button(0x2000, 0x3000),
            &tray_data(0x1234, 7, 0x0400, 0x5678),
            VALID_REMOTE_RANGE,
        )
    }

    fn valid_button() -> Result<DecodedTrayButton, TrayDecodeError> {
        decode_x64_button(&button(0x2000, 0x3000), VALID_REMOTE_RANGE)
    }

    #[test]
    fn decodes_valid_x64_button_and_tray_data_without_reinterpreting_bytes() {
        let decoded = valid_record();
        assert!(decoded.is_ok());
        if let Ok(decoded) = decoded {
            assert_eq!(decoded.owner_window(), 0x1234);
            assert_eq!(decoded.icon_id(), 7);
            assert_eq!(decoded.callback_message(), 0x0400);
            assert_eq!(decoded.icon_handle(), 0x5678);
            assert_eq!(decoded.tooltip_pointer(), 0x3000);
        }
    }

    #[test]
    fn rejects_null_dw_data() {
        let result = decode_x64_tray_record(
            &button(0, 0x3000),
            &tray_data(0x1234, 7, 0x0400, 0x5678),
            VALID_REMOTE_RANGE,
        );

        assert!(matches!(
            result,
            Err(TrayDecodeError::NullPointer("dwData"))
        ));
    }

    #[test]
    fn rejects_truncated_button_and_tray_data() {
        let mut button_bytes = button(0x2000, 0x3000);
        button_bytes.truncate(31);
        let button_result = decode_x64_tray_record(
            &button_bytes,
            &tray_data(0x1234, 7, 0x0400, 0x5678),
            VALID_REMOTE_RANGE,
        );
        assert!(matches!(
            button_result,
            Err(TrayDecodeError::TruncatedStructure {
                structure: "TBBUTTON",
                ..
            })
        ));

        let mut tray_bytes = tray_data(0x1234, 7, 0x0400, 0x5678);
        tray_bytes.truncate(31);
        let tray_result =
            decode_x64_tray_record(&button(0x2000, 0x3000), &tray_bytes, VALID_REMOTE_RANGE);
        assert!(matches!(
            tray_result,
            Err(TrayDecodeError::TruncatedStructure {
                structure: "TRAYDATA",
                ..
            })
        ));
    }

    #[test]
    fn rejects_zero_owner_and_callback_before_wm_user() {
        let zero_owner = decode_x64_tray_record(
            &button(0x2000, 0x3000),
            &tray_data(0, 7, 0x0400, 0x5678),
            VALID_REMOTE_RANGE,
        );
        assert!(matches!(
            zero_owner,
            Err(TrayDecodeError::InvalidOwnerWindow)
        ));

        let low_callback = decode_x64_tray_record(
            &button(0x2000, 0x3000),
            &tray_data(0x1234, 7, 0x03ff, 0x5678),
            VALID_REMOTE_RANGE,
        );
        assert!(matches!(
            low_callback,
            Err(TrayDecodeError::InvalidCallbackMessage)
        ));
    }

    #[test]
    fn rejects_dw_data_and_i_string_pointers_outside_validated_remote_range() {
        let outside_dw_data = decode_x64_tray_record(
            &button(0x20_000, 0x3000),
            &tray_data(0x1234, 7, 0x0400, 0x5678),
            VALID_REMOTE_RANGE,
        );
        assert!(matches!(
            outside_dw_data,
            Err(TrayDecodeError::PointerOutOfRange("dwData"))
        ));

        let outside_i_string = decode_x64_tray_record(
            &button(0x2000, 0x20_000),
            &tray_data(0x1234, 7, 0x0400, 0x5678),
            VALID_REMOTE_RANGE,
        );
        assert!(matches!(
            outside_i_string,
            Err(TrayDecodeError::PointerOutOfRange("iString"))
        ));
    }

    #[test]
    fn phase_a_returns_checked_addresses_without_reading_tray_payload() {
        let decoded = valid_button();
        assert!(decoded.is_ok());
        if let Ok(decoded) = decoded {
            assert_eq!(decoded.tray_data_pointer(), 0x2000);
            let tooltip = decoded.tooltip_span();
            assert!(tooltip.is_some());
            if let Some(tooltip) = tooltip {
                assert_eq!(tooltip.pointer(), 0x3000);
                assert_eq!(tooltip.byte_len(), BOUNDED_TOOLTIP_BYTES);
            }
        }
    }

    #[test]
    fn phase_b_rejects_a_remote_address_mismatch_before_payload_decode() {
        let descriptor = valid_button();
        assert!(descriptor.is_ok());
        let descriptor = match descriptor {
            Ok(descriptor) => descriptor,
            Err(_) => return,
        };
        let result = decode_x64_tray_data(descriptor, 0x2001, &[]);
        assert!(matches!(
            result,
            Err(TrayDecodeError::TrayDataAddressMismatch { .. })
        ));
    }

    #[test]
    fn phase_b_decodes_payload_only_at_the_validated_address() {
        let descriptor = valid_button();
        assert!(descriptor.is_ok());
        let descriptor = match descriptor {
            Ok(descriptor) => descriptor,
            Err(_) => return,
        };
        let actual_address = descriptor.tray_data_pointer();
        let result = decode_x64_tray_data(
            descriptor,
            actual_address,
            &tray_data(0x1234, 7, 0x0400, 0x5678),
        );
        assert!(result.is_ok());
        if let Ok(decoded) = result {
            assert_eq!(decoded.owner_window(), 0x1234);
            assert_eq!(decoded.icon_id(), 7);
            assert_eq!(decoded.callback_message(), 0x0400);
            assert_eq!(decoded.icon_handle(), 0x5678);
        }
    }

    #[test]
    fn accepts_pointer_at_range_start_but_rejects_end_overflow_empty_and_reversed_ranges() {
        let exact_start = decode_x64_button(&button(0x1000, 0), 0x1000..0x1000 + 32);
        assert!(exact_start.is_ok());

        let exact_end = decode_x64_button(&button(0x1000, 0), 0x1000..0x1000 + 31);
        assert!(matches!(
            exact_end,
            Err(TrayDecodeError::PointerOutOfRange("dwData"))
        ));

        let overflowing = decode_x64_button(&button(usize::MAX - 15, 0), 0..usize::MAX);
        assert!(matches!(
            overflowing,
            Err(TrayDecodeError::PointerOutOfRange("dwData"))
        ));

        let empty = decode_x64_button(&button(0x1000, 0), 0x1000..0x1000);
        assert!(matches!(
            empty,
            Err(TrayDecodeError::PointerOutOfRange("dwData"))
        ));

        let reversed = decode_x64_button(&button(0x1000, 0), 0x2000..0x1000);
        assert!(matches!(
            reversed,
            Err(TrayDecodeError::PointerOutOfRange("dwData"))
        ));
    }

    #[test]
    fn requires_the_full_bounded_tooltip_span_and_allows_null_tooltip() {
        let null_tooltip = decode_x64_button(&button(0x1000, 0), VALID_REMOTE_RANGE);
        assert!(null_tooltip.is_ok());
        if let Ok(null_tooltip) = null_tooltip {
            assert!(null_tooltip.tooltip_span().is_none());
        }

        let near_end = 0x1030;
        let range_end = near_end + BOUNDED_TOOLTIP_BYTES - 2;
        let near_end_result = decode_x64_button(&button(0x1000, near_end), 0x1000..range_end);
        assert!(matches!(
            near_end_result,
            Err(TrayDecodeError::PointerOutOfRange("iString"))
        ));
    }

    #[test]
    fn validates_owner_process_id_without_resolving_processes() {
        assert!(validate_owner_process_id(42).is_ok());
        assert!(matches!(
            validate_owner_process_id(0),
            Err(TrayDecodeError::InvalidOwnerProcessId)
        ));
    }

    #[test]
    fn bounds_and_decodes_utf16_tooltips() {
        let mut valid = vec![u16::from(b'A'); MAX_TRAY_TOOLTIP_CHARS];
        valid.push(0);
        assert_eq!(
            validate_bounded_tooltip_length(&valid),
            Ok(MAX_TRAY_TOOLTIP_CHARS)
        );
        assert_eq!(
            decode_bounded_tooltip(&valid),
            Ok("A".repeat(MAX_TRAY_TOOLTIP_CHARS))
        );

        let too_long = vec![u16::from(b'A'); MAX_TRAY_TOOLTIP_CHARS + 1];
        assert!(matches!(
            validate_bounded_tooltip_length(&too_long),
            Err(TrayDecodeError::TooltipTooLong)
        ));
        assert!(matches!(
            decode_bounded_tooltip(&too_long),
            Err(TrayDecodeError::TooltipTooLong)
        ));

        let invalid_before_nul = [0xD800, 0];
        assert!(matches!(
            decode_bounded_tooltip(&invalid_before_nul),
            Err(TrayDecodeError::InvalidTooltipEncoding)
        ));

        let early_nul_with_invalid_trailing = [u16::from(b'A'), 0, 0xD800];
        assert_eq!(
            decode_bounded_tooltip(&early_nul_with_invalid_trailing),
            Ok(String::from("A"))
        );
    }

    #[test]
    fn caps_native_candidate_count() {
        assert_eq!(bounded_candidate_count(0), 0);
        assert_eq!(
            bounded_candidate_count(MAX_NATIVE_TRAY_CANDIDATES),
            MAX_NATIVE_TRAY_CANDIDATES
        );
        assert_eq!(
            bounded_candidate_count(MAX_NATIVE_TRAY_CANDIDATES + 1),
            MAX_NATIVE_TRAY_CANDIDATES
        );
    }

    #[test]
    fn unsupported_pointer_width_is_explicit() {
        if usize::BITS != 64 {
            let result = valid_record();
            assert!(matches!(result, Err(TrayDecodeError::UnsupportedLayout)));
        }
    }

    #[test]
    fn validates_pointer_width_without_platform_simulation() {
        assert_eq!(validate_pointer_width(64), Ok(()));
        assert_eq!(
            validate_pointer_width(32),
            Err(TrayDecodeError::UnsupportedLayout)
        );
        assert_eq!(
            validate_pointer_width(128),
            Err(TrayDecodeError::UnsupportedLayout)
        );
    }

    #[test]
    fn rejects_overflowing_read_offsets_as_truncated() {
        let word = read_u32(&[], usize::MAX, "TEST");
        assert!(matches!(
            word,
            Err(TrayDecodeError::TruncatedStructure {
                structure: "TEST",
                expected: usize::MAX,
                actual: 0,
            })
        ));

        let pointer = read_pointer(&[], usize::MAX, "TEST");
        assert!(matches!(
            pointer,
            Err(TrayDecodeError::TruncatedStructure {
                structure: "TEST",
                expected: usize::MAX,
                actual: 0,
            })
        ));
    }
}
