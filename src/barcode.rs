use std::sync::mpsc::{self, Sender};
use std::thread;

use anyhow::{Context, Result};
use esp_idf_hal::gpio::{Output, Pin, PinDriver};
use esp_idf_hal::uart::UartDriver;

/// Control messages sent to the barcode scanner worker.
#[derive(Debug, Clone)]
enum ControlMessage {
    Trigger(bool), // true = active/scan, false = inactive
    Led(bool),     // true = on, false = off
    Beep(bool),    // true = on, false = off
}

/// Events produced by the barcode scanner worker.
#[derive(Debug, Clone)]
pub enum BarcodeEvent {
    /// A successfully scanned barcode as text.
    Scanned(String),

    /// A non‑fatal error while reading or parsing.
    Error(String),
}

/// Handle for interacting with the scanner subsystem.
#[derive(Clone)]
pub struct BarcodeHandle {
    control_tx: Sender<ControlMessage>,
}

impl BarcodeHandle {
    /// Set the trigger pin state.
    /// On GM65, trigger is typically active-low for manual trigger mode.
    pub fn set_trigger(&self, active: bool) -> Result<()> {
        self.control_tx.send(ControlMessage::Trigger(active))?;
        Ok(())
    }

    /// Control the LED pin (on/off).
    pub fn set_led(&self, on: bool) -> Result<()> {
        self.control_tx.send(ControlMessage::Led(on))?;
        Ok(())
    }

    /// Control the beep pin (on/off).
    pub fn set_beep(&self, on: bool) -> Result<()> {
        self.control_tx.send(ControlMessage::Beep(on))?;
        Ok(())
    }
}

/// Owns the scanner worker thread and UART.
pub struct BarcodeScanner {
    handle: BarcodeHandle,
}

impl BarcodeScanner {
    /// Start the scanner worker.
    ///
    /// `uart` must be configured for the GM65 default: 9600 8N1, no flow control.
    /// `trigger`, `led`, `beep` are optional GPIO control pins for the GM65.
    /// `on_event` is invoked from the worker thread whenever a barcode or error occurs.
    pub fn start<F, TRIG, LED, BEEP>(
        mut uart: UartDriver<'static>,
        trigger: Option<PinDriver<'static, TRIG, Output>>,
        led: Option<PinDriver<'static, LED, Output>>,
        beep: Option<PinDriver<'static, BEEP, Output>>,
        mut on_event: F,
    ) -> Result<Self>
    where
        F: 'static + Send + FnMut(BarcodeEvent),
        TRIG: Pin,
        LED: Pin,
        BEEP: Pin,
    {
        let (control_tx, control_rx) = mpsc::channel::<ControlMessage>();

        // Use a modest stack; this worker does simple I/O + small buffers.
        thread::Builder::new()
            .name("barcode-worker".into())
            .stack_size(4096)
            .spawn(move || {
                if let Err(e) = run_worker(&mut uart, trigger, led, beep, control_rx, &mut on_event)
                {
                    // Avoid heavy logging if that causes issues; if you see problems,
                    // you can remove this log and surface via another mechanism.
                    log::error!("Barcode worker exited with error: {e:?}");
                }
            })
            .context("Failed to spawn barcode worker thread")?;

        Ok(Self {
            handle: BarcodeHandle { control_tx },
        })
    }

    pub fn handle(&self) -> BarcodeHandle {
        self.handle.clone()
    }
}

const MAX_CODE_LEN: usize = 128;
const READ_TIMEOUT_MS: u32 = 200; // small timeout to keep loop responsive

fn run_worker<F, TRIG, LED, BEEP>(
    uart: &mut UartDriver<'static>,
    mut trigger: Option<PinDriver<'static, TRIG, Output>>,
    mut led: Option<PinDriver<'static, LED, Output>>,
    mut beep: Option<PinDriver<'static, BEEP, Output>>,
    control_rx: std::sync::mpsc::Receiver<ControlMessage>,
    on_event: &mut F,
) -> Result<()>
where
    F: Send + FnMut(BarcodeEvent),
    TRIG: Pin,
    LED: Pin,
    BEEP: Pin,
{
    let mut buf = [0u8; 1];
    let mut line: Vec<u8> = Vec::with_capacity(MAX_CODE_LEN);

    loop {
        // Check for control messages (non-blocking)
        if let Ok(msg) = control_rx.try_recv() {
            match msg {
                ControlMessage::Trigger(active) => {
                    if let Some(ref mut pin) = trigger {
                        // GM65 trigger is typically active-low
                        if active {
                            pin.set_low().ok();
                        } else {
                            pin.set_high().ok();
                        }
                    }
                }
                ControlMessage::Led(on) => {
                    if let Some(ref mut pin) = led {
                        if on {
                            pin.set_high().ok();
                        } else {
                            pin.set_low().ok();
                        }
                    }
                }
                ControlMessage::Beep(on) => {
                    if let Some(ref mut pin) = beep {
                        if on {
                            pin.set_high().ok();
                        } else {
                            pin.set_low().ok();
                        }
                    }
                }
            }
        }

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
