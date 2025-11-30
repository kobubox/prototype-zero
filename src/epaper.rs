use std::sync::mpsc::{self, Receiver, Sender};

use anyhow::{Context, Result};
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
    UpdateLine { line_number: u8, text: String },
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
        .context("EPD init failed")?;

    // Set to quick refresh mode for partial updates
    epd.set_refresh(&mut spi, delay, RefreshLut::Quick)
        .context("Set refresh mode failed")?;

    // Persistent framebuffer to track what's on screen
    let mut framebuffer = Display2in13::default();
    framebuffer.set_rotation(DisplayRotation::Rotate90);
    framebuffer.clear(Color::White).ok();

    // Set initial base buffer for partial updates
    epd.set_partial_base_buffer(&mut spi, delay, framebuffer.buffer())
        .context("Set base buffer failed")?;

    loop {
        let job = rx.recv()?;

        match job {
            DisplayJob::Clear => {
                // Switch to full refresh mode for proper clear
                epd.set_refresh(&mut spi, delay, RefreshLut::Full)
                    .context("Set refresh mode to Full failed")?;

                framebuffer.clear(Color::White).ok();
                epd.clear_frame(&mut spi, delay)
                    .context("Clear frame failed")?;
                epd.display_frame(&mut spi, delay)
                    .context("Display frame failed")?;

                // Switch back to quick refresh mode for partial updates
                epd.set_refresh(&mut spi, delay, RefreshLut::Quick)
                    .context("Set refresh mode to Quick failed")?;

                // Update base buffer after full refresh
                epd.set_partial_base_buffer(&mut spi, delay, framebuffer.buffer())
                    .context("Set base buffer failed after Clear")?;
            }
            DisplayJob::ShowText(text) => {
                // Switch to full refresh mode for complete screen update
                epd.set_refresh(&mut spi, delay, RefreshLut::Full)
                    .context("Set refresh mode to Full failed")?;

                // Clear framebuffer and draw text
                framebuffer.clear(Color::White).ok();

                let style = MonoTextStyleBuilder::new()
                    .font(&FONT_6X10)
                    .text_color(Color::Black)
                    .build();

                Text::new(&text, Point::new(10, 30), style)
                    .draw(&mut framebuffer)
                    .ok();

                // Full refresh
                epd.update_frame(&mut spi, framebuffer.buffer(), delay)
                    .context("Update frame failed")?;
                epd.display_frame(&mut spi, delay)
                    .context("Display frame failed")?;

                // Switch back to quick refresh mode for partial updates
                epd.set_refresh(&mut spi, delay, RefreshLut::Quick)
                    .context("Set refresh mode to Quick failed")?;

                // Update base buffer after full refresh
                epd.set_partial_base_buffer(&mut spi, delay, framebuffer.buffer())
                    .context("Set base buffer failed after ShowText")?;
            }
            DisplayJob::UpdateLine { line_number, text } => {
                use embedded_graphics::primitives::Rectangle;

                // Calculate line position
                let line_height = 12;
                let y_offset = 10 + (line_number as i32 * line_height);

                // Clear only the specific line region (local update)
                let clear_rect = Rectangle::new(
                    Point::new(0, y_offset - 2),
                    Size::new(122, line_height as u32), // Full width, one line height
                );

                framebuffer.fill_solid(&clear_rect, Color::White).ok();

                // Draw new text at the line position
                let style = MonoTextStyleBuilder::new()
                    .font(&FONT_6X10)
                    .text_color(Color::Black)
                    .build();

                Text::new(&text, Point::new(10, y_offset), style)
                    .draw(&mut framebuffer)
                    .ok();

                // Quick partial refresh - only updates changed pixels
                epd.update_and_display_frame(&mut spi, framebuffer.buffer(), delay)
                    .context("Partial update failed")?;

                // Update the base buffer to keep it in sync
                epd.set_partial_base_buffer(&mut spi, delay, framebuffer.buffer())
                    .context("Set base buffer failed after UpdateLine")?;
            }
        }
    }
}
