use std::error::Error;

use shell_core::{
    ArmDeniedReason, ArmRequestId, FixedHexError, MAX_TASKBAR_ARMING_SNAPSHOTS,
    MAX_TASKBAR_DEVICE_ID_BYTES, MAX_WATCHDOG_CONTROL_FRAME_BYTES, RecoveryTransactionId,
    TaskbarArmingFingerprintError, TaskbarFingerprintBounds, TaskbarFingerprintSnapshot,
    TaskbarStateFingerprint, TaskbarWindowClass, WatchdogControlFrame, WatchdogFrameDirection,
    WatchdogProtocolError, taskbar_arming_fingerprint_v1,
};

fn request_id() -> Result<ArmRequestId, FixedHexError> {
    ArmRequestId::try_new(0x2a)
}

fn transaction_id() -> Result<RecoveryTransactionId, FixedHexError> {
    let mut bytes = [0_u8; 16];
    bytes[15] = 0x2a;
    RecoveryTransactionId::try_from_bytes(bytes)
}

fn fingerprint_bytes() -> TaskbarStateFingerprint {
    TaskbarStateFingerprint::from_bytes([0xab; 32])
}

fn primary(device: &str) -> TaskbarFingerprintSnapshot {
    TaskbarFingerprintSnapshot::new(
        device,
        TaskbarWindowClass::Primary,
        true,
        TaskbarFingerprintBounds::new(0, 1_040, 1_920, 1_080),
    )
}

fn secondary(device: &str) -> TaskbarFingerprintSnapshot {
    TaskbarFingerprintSnapshot::new(
        device,
        TaskbarWindowClass::Secondary,
        false,
        TaskbarFingerprintBounds::new(-1_280, 984, 0, 1_024),
    )
}

#[test]
fn every_control_frame_has_a_bounded_canonical_round_trip() -> Result<(), Box<dyn Error>> {
    let request_id = request_id()?;
    let transaction_id = transaction_id()?;
    let fingerprint = fingerprint_bytes();
    let frames = [
        WatchdogControlFrame::Arm {
            request_id,
            fingerprint,
        },
        WatchdogControlFrame::Applied { transaction_id },
        WatchdogControlFrame::Cancel { request_id },
        WatchdogControlFrame::Disarm { transaction_id },
        WatchdogControlFrame::Armed {
            request_id,
            transaction_id,
        },
        WatchdogControlFrame::Denied {
            request_id,
            reason: ArmDeniedReason::PendingRecovery,
        },
        WatchdogControlFrame::Lease { transaction_id },
        WatchdogControlFrame::Disarmed { transaction_id },
    ];

    for frame in frames {
        let encoded = frame.encode();
        assert!(encoded.len() <= MAX_WATCHDOG_CONTROL_FRAME_BYTES);
        assert!(encoded.is_ascii());
        assert!(!encoded.contains(['\r', '\n', '\t']));
        assert_eq!(WatchdogControlFrame::parse(encoded.as_bytes())?, frame);
    }
    assert_eq!(
        frames[0].encode(),
        concat!(
            "MINHA_UI_CONTROL v1 ARM 000000000000002a ",
            "abababababababababababababababababababababababababababababababab"
        )
    );
    Ok(())
}

#[test]
fn directions_are_explicit_for_pipe_adapters() -> Result<(), Box<dyn Error>> {
    let request_id = request_id()?;
    let transaction_id = transaction_id()?;

    assert_eq!(
        WatchdogControlFrame::Arm {
            request_id,
            fingerprint: fingerprint_bytes(),
        }
        .direction(),
        WatchdogFrameDirection::ChildToWatchdog
    );
    assert_eq!(
        WatchdogControlFrame::Disarm { transaction_id }.direction(),
        WatchdogFrameDirection::ChildToWatchdog
    );
    assert_eq!(
        WatchdogControlFrame::Armed {
            request_id,
            transaction_id,
        }
        .direction(),
        WatchdogFrameDirection::WatchdogToChild
    );
    assert_eq!(
        WatchdogControlFrame::Lease { transaction_id }.direction(),
        WatchdogFrameDirection::WatchdogToChild
    );
    Ok(())
}

#[test]
fn parser_rejects_unbounded_or_noncanonical_input() {
    let oversized = vec![b'x'; MAX_WATCHDOG_CONTROL_FRAME_BYTES + 1];
    assert_eq!(
        WatchdogControlFrame::parse(&oversized),
        Err(WatchdogProtocolError::Oversized {
            actual: MAX_WATCHDOG_CONTROL_FRAME_BYTES + 1,
            maximum: MAX_WATCHDOG_CONTROL_FRAME_BYTES,
        })
    );
    assert_eq!(
        WatchdogControlFrame::parse(b"MINHA_UI_CONTROL v1 CANCEL 000000000000002a\n"),
        Err(WatchdogProtocolError::InvalidSpacing)
    );
    assert_eq!(
        WatchdogControlFrame::parse(b"MINHA_UI_CONTROL  v1 CANCEL 000000000000002a"),
        Err(WatchdogProtocolError::InvalidSpacing)
    );
    assert_eq!(
        WatchdogControlFrame::parse(b"MINHA_UI_CONTROL v2 CANCEL 000000000000002a"),
        Err(WatchdogProtocolError::UnsupportedVersion)
    );
    assert_eq!(
        WatchdogControlFrame::parse(b"MINHA_UI_CONTROL v1 UNKNOWN 000000000000002a"),
        Err(WatchdogProtocolError::UnknownFrameKind)
    );
    assert_eq!(
        WatchdogControlFrame::parse(b"MINHA_UI_CONTROL v1 CANCEL 000000000000002A"),
        Err(WatchdogProtocolError::InvalidRequestId {
            source: FixedHexError::NonCanonical,
        })
    );
    assert_eq!(
        WatchdogControlFrame::parse(b"MINHA_UI_CONTROL v1 DENIED 000000000000002a future_reason"),
        Err(WatchdogProtocolError::UnknownDenialReason)
    );
    assert_eq!(
        WatchdogControlFrame::parse(b"MINHA_UI_CONTROL v1 LEASE 00"),
        Err(WatchdogProtocolError::InvalidTransactionId {
            source: FixedHexError::InvalidLength {
                expected: 32,
                actual: 2,
            },
        })
    );
}

#[test]
fn identifiers_are_nonzero_fixed_width_lowercase_hex() -> Result<(), Box<dyn Error>> {
    let request = request_id()?;
    let transaction = transaction_id()?;

    assert_eq!(request.to_string(), "000000000000002a");
    assert_eq!(request.to_string().parse::<ArmRequestId>()?, request);
    assert_eq!(transaction.to_string(), "0000000000000000000000000000002a");
    assert_eq!(
        transaction.to_string().parse::<RecoveryTransactionId>()?,
        transaction
    );
    assert_eq!(ArmRequestId::try_new(0), Err(FixedHexError::Zero));
    assert_eq!(
        "0000000000000000".parse::<ArmRequestId>(),
        Err(FixedHexError::Zero)
    );
    assert_eq!(
        "00000000000000000000000000000000".parse::<RecoveryTransactionId>(),
        Err(FixedHexError::Zero)
    );
    assert_eq!(
        "000000000000002A".parse::<ArmRequestId>(),
        Err(FixedHexError::NonCanonical)
    );
    Ok(())
}

#[test]
fn fingerprint_is_independent_of_enumeration_order_and_ascii_device_case()
-> Result<(), Box<dyn Error>> {
    let first = [primary(r"\\.\DISPLAY1"), secondary(r"\\.\display2")];
    let reordered = [secondary(r"\\.\DISPLAY2"), primary(r"\\.\display1")];

    let first = taskbar_arming_fingerprint_v1(3, 4_242, 1, &first)?;
    let reordered = taskbar_arming_fingerprint_v1(3, 4_242, 1, &reordered)?;

    assert_eq!(first, reordered);
    assert_eq!(first.to_string().len(), 64);
    assert_eq!(first.to_string().parse::<TaskbarStateFingerprint>()?, first);
    Ok(())
}

#[test]
fn fingerprint_domain_includes_session_process_state_and_every_snapshot_field()
-> Result<(), Box<dyn Error>> {
    let baseline_snapshots = [primary(r"\\.\DISPLAY1")];
    let baseline = taskbar_arming_fingerprint_v1(3, 4_242, 0, &baseline_snapshots)?;
    let changed_device = [primary(r"\\.\DISPLAY2")];
    let changed_visibility = [TaskbarFingerprintSnapshot::new(
        r"\\.\DISPLAY1",
        TaskbarWindowClass::Primary,
        false,
        TaskbarFingerprintBounds::new(0, 1_040, 1_920, 1_080),
    )];
    let changed_bounds = [TaskbarFingerprintSnapshot::new(
        r"\\.\DISPLAY1",
        TaskbarWindowClass::Primary,
        true,
        TaskbarFingerprintBounds::new(0, 1_039, 1_920, 1_080),
    )];

    assert_ne!(
        baseline,
        taskbar_arming_fingerprint_v1(4, 4_242, 0, &baseline_snapshots)?
    );
    assert_ne!(
        baseline,
        taskbar_arming_fingerprint_v1(3, 4_243, 0, &baseline_snapshots)?
    );
    assert_ne!(
        baseline,
        taskbar_arming_fingerprint_v1(3, 4_242, 1, &baseline_snapshots)?
    );
    assert_ne!(
        baseline,
        taskbar_arming_fingerprint_v1(3, 4_242, 0, &changed_device)?
    );
    assert_ne!(
        baseline,
        taskbar_arming_fingerprint_v1(3, 4_242, 0, &changed_visibility)?
    );
    assert_ne!(
        baseline,
        taskbar_arming_fingerprint_v1(3, 4_242, 0, &changed_bounds)?
    );
    Ok(())
}

#[test]
fn fingerprint_has_a_frozen_v1_vector() -> Result<(), Box<dyn Error>> {
    let snapshots = [primary(r"\\.\DISPLAY1"), secondary(r"\\.\DISPLAY2")];

    let fingerprint = taskbar_arming_fingerprint_v1(3, 4_242, 1, &snapshots)?;

    // Updated only by an intentional V2/domain change, never by refactoring.
    assert_eq!(
        fingerprint.to_string(),
        "e55f5b5949ad40b9fd86757b0742129da2908775f0891b1874cedeeaf500dff4"
    );
    Ok(())
}

#[test]
fn fingerprint_rejects_states_that_v1_cannot_compare_safely() {
    let empty: [TaskbarFingerprintSnapshot; 0] = [];
    assert_eq!(
        taskbar_arming_fingerprint_v1(1, 0, 0, &[primary(r"\\.\DISPLAY1")]),
        Err(TaskbarArmingFingerprintError::InvalidExplorerProcessId)
    );
    assert_eq!(
        taskbar_arming_fingerprint_v1(1, 42, 2, &[primary(r"\\.\DISPLAY1")]),
        Err(TaskbarArmingFingerprintError::UnsupportedAppbarState { state: 2 })
    );
    assert_eq!(
        taskbar_arming_fingerprint_v1(1, 42, 0, &empty),
        Err(TaskbarArmingFingerprintError::EmptySnapshots)
    );
    assert_eq!(
        taskbar_arming_fingerprint_v1(
            1,
            42,
            0,
            &[primary(r"\\.\DISPLAY1"), primary(r"\\.\display1")],
        ),
        Err(TaskbarArmingFingerprintError::DuplicateSnapshot {
            first: 0,
            second: 1,
        })
    );
    assert_eq!(
        taskbar_arming_fingerprint_v1(1, 42, 0, &[secondary(r"\\.\DISPLAY2")],),
        Err(TaskbarArmingFingerprintError::InvalidPrimaryCount { actual: 0 })
    );
    assert_eq!(
        taskbar_arming_fingerprint_v1(
            1,
            42,
            0,
            &[TaskbarFingerprintSnapshot::new(
                "bad device",
                TaskbarWindowClass::Primary,
                true,
                TaskbarFingerprintBounds::new(0, 0, 1, 1),
            )],
        ),
        Err(TaskbarArmingFingerprintError::InvalidDevice { index: 0 })
    );
}

#[test]
fn fingerprint_limits_are_fixed_and_tested_at_the_boundary() -> Result<(), Box<dyn Error>> {
    let mut maximum = Vec::with_capacity(MAX_TASKBAR_ARMING_SNAPSHOTS);
    maximum.push(primary(r"\\.\DISPLAY1"));
    for index in 1..MAX_TASKBAR_ARMING_SNAPSHOTS {
        maximum.push(secondary(&format!(r"\\.\DISPLAY{}", index + 1)));
    }
    assert!(taskbar_arming_fingerprint_v1(1, 42, 0, &maximum).is_ok());

    maximum.push(secondary(r"\\.\DISPLAY65"));
    assert_eq!(
        taskbar_arming_fingerprint_v1(1, 42, 0, &maximum),
        Err(TaskbarArmingFingerprintError::TooManySnapshots {
            actual: MAX_TASKBAR_ARMING_SNAPSHOTS + 1,
            maximum: MAX_TASKBAR_ARMING_SNAPSHOTS,
        })
    );

    let maximum_device = "D".repeat(MAX_TASKBAR_DEVICE_ID_BYTES);
    assert!(taskbar_arming_fingerprint_v1(1, 42, 0, &[primary(&maximum_device)]).is_ok());
    let oversized_device = "D".repeat(MAX_TASKBAR_DEVICE_ID_BYTES + 1);
    assert_eq!(
        taskbar_arming_fingerprint_v1(1, 42, 0, &[primary(&oversized_device)]),
        Err(TaskbarArmingFingerprintError::DeviceTooLong {
            index: 0,
            actual: MAX_TASKBAR_DEVICE_ID_BYTES + 1,
            maximum: MAX_TASKBAR_DEVICE_ID_BYTES,
        })
    );
    Ok(())
}
