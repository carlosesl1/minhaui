use std::io::{self, BufRead, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread::Builder;
use std::time::Duration;

use shell_core::{
    ArmRequestId, MAX_WATCHDOG_CONTROL_FRAME_BYTES, RecoveryTransactionId, TaskbarStateFingerprint,
    WatchdogControlFrame, WatchdogFrameDirection,
};
use windows::Win32::Foundation::{E_ACCESSDENIED, E_FAIL};
use windows::core::{Error, Result};

const ARMING_TIMEOUT: Duration = Duration::from_secs(5);
const RESPONSE_QUEUE_CAPACITY: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WatchdogResponse {
    Frame(WatchdogControlFrame),
    ProtocolViolation,
}

/// Capability required by every native taskbar mutation entry point.
///
/// Construction is private to the authenticated stdin/stdout handshake. The
/// watchdog persists `Prepared`, independently matches the fingerprint, moves
/// the journal to `Applied`, and only then emits the `LEASE` accepted here.
pub(super) struct TaskbarMutationLease {
    transaction_id: RecoveryTransactionId,
    fingerprint: TaskbarStateFingerprint,
    transport_alive: Arc<AtomicBool>,
    responses: Receiver<WatchdogResponse>,
}

impl TaskbarMutationLease {
    pub(super) fn acquire(
        fingerprint: TaskbarStateFingerprint,
        watchdog_supervised: bool,
    ) -> Result<Self> {
        if !watchdog_supervised {
            return Err(Error::new(
                E_ACCESSDENIED,
                "experimental taskbar replacement requires a watchdog-supervised child",
            ));
        }
        let request_id = ArmRequestId::try_new(1)
            .map_err(|error| protocol_error(format!("invalid arming request id: {error}")))?;
        let transport_alive = Arc::new(AtomicBool::new(true));
        let responses = response_reader(Arc::clone(&transport_alive))?;

        write_frame(WatchdogControlFrame::Arm {
            request_id,
            fingerprint,
        })?;
        let transaction_id = match recv_response(&responses)? {
            WatchdogControlFrame::Armed {
                request_id: found,
                transaction_id,
            } if found == request_id => transaction_id,
            WatchdogControlFrame::Denied {
                request_id: found,
                reason,
            } if found == request_id => {
                transport_alive.store(false, Ordering::Release);
                return Err(protocol_error(format!(
                    "watchdog denied taskbar arming: {reason:?}"
                )));
            }
            _ => {
                transport_alive.store(false, Ordering::Release);
                return Err(protocol_error(
                    "watchdog returned an unexpected response to ARM",
                ));
            }
        };

        // APPLIED is the child's commit request. The watchdog performs and
        // syncs Prepared -> Applied before replying with LEASE. Native mutation
        // begins only after that response is authenticated below.
        write_frame(WatchdogControlFrame::Applied { transaction_id })?;
        match recv_response(&responses)? {
            WatchdogControlFrame::Lease {
                transaction_id: found,
            } if found == transaction_id => Ok(Self {
                transaction_id,
                fingerprint,
                transport_alive,
                responses,
            }),
            _ => {
                transport_alive.store(false, Ordering::Release);
                Err(protocol_error(
                    "watchdog did not grant the matching taskbar lease",
                ))
            }
        }
    }

    #[must_use]
    pub(super) fn is_valid(&self) -> bool {
        self.transport_alive.load(Ordering::Acquire)
    }

    #[must_use]
    pub(super) const fn fingerprint(&self) -> TaskbarStateFingerprint {
        self.fingerprint
    }

    /// Requests verified restoration and waits for the watchdog to remove the
    /// authenticated journal before acknowledging shutdown.
    pub(super) fn disarm(self) -> Result<()> {
        if !self.is_valid() {
            return Err(Error::new(
                E_ACCESSDENIED,
                "the watchdog lease transport closed before DISARM",
            ));
        }
        write_frame(WatchdogControlFrame::Disarm {
            transaction_id: self.transaction_id,
        })?;
        match recv_response(&self.responses)? {
            WatchdogControlFrame::Disarmed {
                transaction_id: found,
            } if found == self.transaction_id => {
                self.transport_alive.store(false, Ordering::Release);
                Ok(())
            }
            _ => Err(protocol_error(
                "watchdog did not confirm the matching DISARM transaction",
            )),
        }
    }
}

fn write_frame(frame: WatchdogControlFrame) -> Result<()> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    writeln!(output, "{}", frame.encode()).map_err(transport_error)?;
    output.flush().map_err(transport_error)
}

fn recv_response(responses: &Receiver<WatchdogResponse>) -> Result<WatchdogControlFrame> {
    match responses.recv_timeout(ARMING_TIMEOUT) {
        Ok(WatchdogResponse::Frame(frame))
            if frame.direction() == WatchdogFrameDirection::WatchdogToChild =>
        {
            Ok(frame)
        }
        Ok(WatchdogResponse::Frame(_)) | Ok(WatchdogResponse::ProtocolViolation) => {
            Err(protocol_error("watchdog sent an invalid control response"))
        }
        Err(error) => Err(protocol_error(format!(
            "watchdog response timed out or disconnected: {error}"
        ))),
    }
}

fn response_reader(transport_alive: Arc<AtomicBool>) -> Result<Receiver<WatchdogResponse>> {
    let (sender, receiver) = mpsc::sync_channel(RESPONSE_QUEUE_CAPACITY);
    Builder::new()
        .name("minha-ui-watchdog-control".to_owned())
        .spawn(move || {
            let stdin = io::stdin();
            pump_responses(stdin.lock(), &sender, &transport_alive);
        })
        .map_err(|error| protocol_error(format!("failed to start watchdog reader: {error}")))?;
    Ok(receiver)
}

fn pump_responses(
    mut input: impl BufRead,
    sender: &mpsc::SyncSender<WatchdogResponse>,
    transport_alive: &AtomicBool,
) {
    let mut frame = Vec::with_capacity(MAX_WATCHDOG_CONTROL_FRAME_BYTES);
    let mut oversized = false;
    while let Ok(buffer) = input.fill_buf() {
        if buffer.is_empty() {
            if !frame.is_empty() || oversized {
                let _ = sender.send(WatchdogResponse::ProtocolViolation);
            }
            break;
        }
        let consumed = buffer.len();
        for byte in buffer {
            if *byte == b'\n' {
                if frame.last() == Some(&b'\r') {
                    let _ = frame.pop();
                }
                let response = if oversized {
                    WatchdogResponse::ProtocolViolation
                } else {
                    WatchdogControlFrame::parse(&frame)
                        .map(WatchdogResponse::Frame)
                        .unwrap_or(WatchdogResponse::ProtocolViolation)
                };
                let valid_direction = matches!(
                    response,
                    WatchdogResponse::Frame(frame)
                        if frame.direction() == WatchdogFrameDirection::WatchdogToChild
                );
                if sender.send(response).is_err() || !valid_direction {
                    transport_alive.store(false, Ordering::Release);
                    return;
                }
                frame.clear();
                oversized = false;
            } else if frame.len() < MAX_WATCHDOG_CONTROL_FRAME_BYTES {
                frame.push(*byte);
            } else {
                oversized = true;
            }
        }
        input.consume(consumed);
    }
    transport_alive.store(false, Ordering::Release);
}

fn transport_error(error: io::Error) -> Error {
    Error::new(
        E_FAIL,
        format!("watchdog control transport failed: {error}"),
    )
}

fn protocol_error(message: impl Into<String>) -> Error {
    Error::new(E_ACCESSDENIED, message.into())
}

#[cfg(test)]
mod tests {
    use super::{WatchdogResponse, pump_responses};
    use std::io::Cursor;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;

    #[test]
    fn malformed_response_immediately_invalidates_transport() {
        let (sender, receiver) = mpsc::sync_channel(2);
        let alive = AtomicBool::new(true);

        pump_responses(Cursor::new(b"not-a-control-frame\n"), &sender, &alive);

        assert_eq!(
            receiver.recv().ok(),
            Some(WatchdogResponse::ProtocolViolation)
        );
        assert!(!alive.load(Ordering::Acquire));
    }

    #[test]
    fn wrong_direction_response_immediately_invalidates_transport() {
        let (sender, receiver) = mpsc::sync_channel(2);
        let alive = AtomicBool::new(true);
        let input = b"MINHA_UI_CONTROL v1 APPLIED 01010101010101010101010101010101\n";

        pump_responses(Cursor::new(input), &sender, &alive);

        assert!(matches!(receiver.recv(), Ok(WatchdogResponse::Frame(_))));
        assert!(!alive.load(Ordering::Acquire));
    }
}
