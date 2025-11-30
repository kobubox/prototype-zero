use anyhow::Result;
use esp_idf_hal::{
    delay::FreeRtos,
    gpio::{Output, PinDriver},
};
use std::sync::mpsc::{self, Receiver, Sender};

/// Events that control the blinker
#[derive(Debug, Clone)]
pub enum BlinkEvent {
    UpdateConfig { enabled: bool, period_ms: u64 },
}

/// Handle for sending events to the blinker
#[derive(Clone)]
pub struct BlinkHandle {
    sender: Sender<BlinkEvent>,
}

impl BlinkHandle {
    pub fn update_config(&self, enabled: bool, period_ms: u64) -> Result<()> {
        self.sender
            .send(BlinkEvent::UpdateConfig { enabled, period_ms })?;
        Ok(())
    }
}

pub struct Blinker;

impl Blinker {
    pub fn start<P: esp_idf_hal::gpio::Pin + Send + 'static>(
        led: PinDriver<'static, P, Output>,
        initial_enabled: bool,
        initial_period_ms: u64,
    ) -> Result<BlinkHandle> {
        let (tx, rx) = mpsc::channel();

        std::thread::Builder::new()
            .stack_size(4096)
            .spawn(move || {
                if let Err(e) = run_blinker(led, rx, initial_enabled, initial_period_ms) {
                    log::error!("Blinker thread exited with error: {:?}", e);
                }
            })?;

        Ok(BlinkHandle { sender: tx })
    }
}

fn run_blinker<P: esp_idf_hal::gpio::Pin>(
    mut led: PinDriver<'static, P, Output>,
    rx: Receiver<BlinkEvent>,
    mut enabled: bool,
    mut period_ms: u64,
) -> Result<()> {
    loop {
        // Check for new events (non-blocking)
        while let Ok(event) = rx.try_recv() {
            match event {
                BlinkEvent::UpdateConfig {
                    enabled: new_enabled,
                    period_ms: new_period,
                } => {
                    enabled = new_enabled;
                    period_ms = new_period;
                }
            }
        }

        // Execute blink cycle
        if enabled {
            let half = (period_ms / 2).max(10) as u32;
            led.set_high().ok();
            FreeRtos::delay_ms(half);
            led.set_low().ok();
            FreeRtos::delay_ms(half);
        } else {
            led.set_low().ok();
            FreeRtos::delay_ms(100);
        }
    }
}
