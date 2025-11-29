use std::sync::mpsc::{self, Receiver, Sender};

use anyhow::Result;
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyleBuilder},
    prelude::*,
    text::Text,
};
use embedded_hal::delay::DelayNs;
use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal::spi::SpiDevice;
use epd_waveshare::epd2in13_v2::{Display2in13, Epd2in13};
use epd_waveshare::prelude::*;

/// Delay implementation that works in threads
pub struct Delay;

impl DelayNs for Delay {
    fn delay_ns(&mut self, ns: u32) {
        let ms = (ns / 1_000_000).max(1);
        esp_idf_hal::delay::FreeRtos::delay_ms(ms);
    }

    fn delay_us(&mut self, us: u32) {
        let ms = (us / 1000).max(1);
        esp_idf_hal::delay::FreeRtos::delay_ms(ms);
    }

    fn delay_ms(&mut self, ms: u32) {
        esp_idf_hal::delay::FreeRtos::delay_ms(ms);
    }
}

/// Basic jobs the display worker can perform.
#[derive(Debug)]
pub enum DisplayJob {
    Clear,
    ShowText(String),
}

#[derive(Clone)]
pub struct DisplayHandle {
    sender: Sender<DisplayJob>,
}

impl DisplayHandle {
    pub fn submit(&self, job: DisplayJob) -> Result<()> {
        self.sender.send(job)?;
        Ok(())
    }
}

/// Manages the e-paper driver in a dedicated worker thread.
pub struct DisplayManager {
    handle: DisplayHandle,
}

impl DisplayManager {
    #[allow(clippy::too_many_arguments)]
    pub fn start<SPI, CS, DC, RST, BUSY>(
        spi: SPI,
        cs: CS,
        dc: DC,
        rst: RST,
        busy: BUSY,
    ) -> Result<Self>
    where
        SPI: 'static + SpiDevice + Send,
        SPI::Error: std::error::Error + Send + Sync + 'static,
        CS: 'static + OutputPin + Send,
        DC: 'static + OutputPin + Send,
        RST: 'static + OutputPin + Send,
        BUSY: 'static + InputPin + Send,
    {
        let (tx, rx): (Sender<DisplayJob>, Receiver<DisplayJob>) = mpsc::channel();

        // Move ownership of all hardware into the worker thread.
        // Use std::thread::Builder to set a larger stack size
        std::thread::Builder::new()
            .stack_size(8192) // 8KB stack for the display worker
            .spawn(move || {
                let mut delay = Delay;
                if let Err(e) = run_worker(spi, cs, dc, rst, busy, &mut delay, rx) {
                    log::error!("EPD worker exited with error: {:?}", e);
                }
            })?;

        Ok(Self {
            handle: DisplayHandle { sender: tx },
        })
    }

    pub fn handle(&self) -> DisplayHandle {
        self.handle.clone()
    }
}

fn run_worker<SPI, CS, DC, RST, BUSY, DELAY>(
    mut spi: SPI,
    _cs: CS,
    mut dc: DC,
    mut rst: RST,
    mut busy: BUSY,
    delay: &mut DELAY,
    rx: Receiver<DisplayJob>,
) -> Result<()>
where
    SPI: SpiDevice,
    SPI::Error: std::error::Error + Send + Sync + 'static,
    CS: OutputPin,
    DC: OutputPin,
    RST: OutputPin,
    BUSY: InputPin,
    DELAY: DelayNs,
{
    // Initialize EPD hardware (no logging to avoid mutex issues during early startup)
    let mut epd = Epd2in13::new(&mut spi, &mut busy, &mut dc, &mut rst, delay, None)
        .map_err(|e| anyhow::anyhow!("EPD init failed: {:?}", e))?;

    loop {
        let job = rx.recv()?;

        match job {
            DisplayJob::Clear => {
                epd.clear_frame(&mut spi, delay)
                    .map_err(|e| anyhow::anyhow!("Clear frame failed: {:?}", e))?;
                epd.display_frame(&mut spi, delay)
                    .map_err(|e| anyhow::anyhow!("Display frame failed: {:?}", e))?;
            }
            DisplayJob::ShowText(text) => {
                // Create a display buffer
                let mut display = Display2in13::default();
                display.set_rotation(DisplayRotation::Rotate90);
                display.clear(Color::White).ok();

                // Draw text using Color::Black
                let style = MonoTextStyleBuilder::new()
                    .font(&FONT_6X10)
                    .text_color(Color::Black)
                    .build();

                Text::new(&text, Point::new(10, 30), style)
                    .draw(&mut display)
                    .ok();

                // Update the display
                epd.update_frame(&mut spi, display.buffer(), delay)
                    .map_err(|e| anyhow::anyhow!("Update frame failed: {:?}", e))?;
                epd.display_frame(&mut spi, delay)
                    .map_err(|e| anyhow::anyhow!("Display frame failed: {:?}", e))?;
            }
        }
    }
}
