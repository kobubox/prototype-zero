use std::thread;

use anyhow::{Context, Result};
use esp_idf_hal::uart::UartDriver;

/// Events produced by the barcode scanner worker.
#[derive(Debug, Clone)]
pub enum BarcodeEvent {
    /// A successfully scanned barcode as text.
    Scanned(String),

    /// A non‑fatal error while reading or parsing.
    Error(String),
}

/// Handle for interacting with the scanner subsystem.
///
/// Currently this is a placeholder so we can extend it later
/// with control operations (e.g. command‑trigger, reconfigure, etc.).
#[derive(Clone)]
pub struct BarcodeHandle {}

/// Owns the scanner worker thread and UART.
pub struct BarcodeScanner {
    handle: BarcodeHandle,
}

impl BarcodeScanner {
    /// Start the scanner worker.
    ///
    /// `uart` must be configured for the GM65 default: 9600 8N1, no flow control.
    /// `on_event` is invoked from the worker thread whenever a barcode or error occurs.
    pub fn start<F>(mut uart: UartDriver<'static>, mut on_event: F) -> Result<Self>
    where
        F: 'static + Send + FnMut(BarcodeEvent),
    {
        // Use a modest stack; this worker does simple I/O + small buffers.
        thread::Builder::new()
            .name("barcode-worker".into())
            .stack_size(4096)
            .spawn(move || {
                if let Err(e) = run_worker(&mut uart, &mut on_event) {
                    // Avoid heavy logging if that causes issues; if you see problems,
                    // you can remove this log and surface via another mechanism.
                    log::error!("Barcode worker exited with error: {e:?}");
                }
            })
            .context("Failed to spawn barcode worker thread")?;

        Ok(Self {
            handle: BarcodeHandle {},
        })
    }

    pub fn handle(&self) -> BarcodeHandle {
        self.handle.clone()
    }
}

const MAX_CODE_LEN: usize = 128;
const READ_TIMEOUT_MS: u32 = 200; // small timeout to keep loop responsive

fn run_worker<F>(uart: &mut UartDriver<'static>, on_event: &mut F) -> Result<()>
where
    F: Send + FnMut(BarcodeEvent),
{
    let mut buf = [0u8; 1];
    let mut line: Vec<u8> = Vec::with_capacity(MAX_CODE_LEN);

    loop {
        match uart.read(&mut buf, READ_TIMEOUT_MS) {
            Ok(0) => {
                // Timeout, no data – just continue.
            }
            Ok(1) => {
                let b = buf[0];

                if b == b'\r' || b == b'\n' {
                    // End of code – normalize and emit if non‑empty.
                    if !line.is_empty() {
                        let s = String::from_utf8_lossy(&line).trim().to_string();
                        line.clear();

                        if !s.is_empty() {
                            on_event(BarcodeEvent::Scanned(s));
                        }
                    } else {
                        // Empty line; ignore.
                    }
                } else if line.len() < MAX_CODE_LEN {
                    line.push(b);
                } else {
                    // Overflow – discard and report an error.
                    line.clear();
                    on_event(BarcodeEvent::Error("Barcode too long".into()));
                }
            }
            Ok(_) => {
                // Should not happen with len=1, but ignore.
            }
            Err(e) => {
                on_event(BarcodeEvent::Error(format!("UART read error: {e:?}")));
            }
        }
    }
}
