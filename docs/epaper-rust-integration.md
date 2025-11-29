# 2.13" e-Paper – Rust Integration Plan

## Goals
- Use `esp-idf-hal` to drive the Waveshare 2.13" e-Paper HAT (B).
- Keep the driver generic over `embedded-hal` traits so it can be re-used on non-ESP32 targets.

## Crate / Module Layout
- Short term: implement driver as `src/epaper.rs` inside the `blink` crate.
- Longer term: extract into a dedicated crate (e.g. `epd213-waveshare`) that depends on `embedded-hal` only.

## HAL Types (ESP-IDF)

On ESP32 with `esp-idf-hal`, the concrete types may look like:

- `SpiDeviceDriver` or `SpiDriver` + `SpiConfig` for the SPI bus.
- GPIO pins from `peripherals.pins` wrapped in `PinDriver` for input/output.
- Delay via `FreeRtos` or a thin wrapper that implements `embedded_hal::blocking::delay::DelayMs`.

We’ll create a small adapter layer if needed so the e-paper driver can depend only on the `embedded-hal` traits, not directly on `esp-idf-hal`.

## Example Usage Sketch

```rust
// Pseudo-code, not final
let peripherals = Peripherals::take()?;
let pins = peripherals.pins;

// Configure SPI
let spi = SpiDriver::new(
    peripherals.spi2,
    pins.gpio18,  // SCK
    pins.gpio23,  // MOSI
    None,         // MISO not used
    &spi_config,
)?;

let epd_cs = PinDriver::output(pins.gpio5)?;
let epd_dc = PinDriver::output(pins.gpio17)?;
let epd_rst = PinDriver::output(pins.gpio16)?;
let epd_busy = PinDriver::input(pins.gpio4)?;

let delay = FreeRtos; // or wrapper

let mut epd = Epd213::new(spi, epd_cs, epd_dc, epd_rst, epd_busy, delay)?;

// Clear to white and draw a simple pattern
epd.clear(Color::White)?;
// draw some pixels into buffer
// epd.draw_pixel(...)

epd.update_full()?;
```

## Steps To Implement
1. Finalise hardware constants and controller command list in other docs.
2. Implement `Epd213` struct and basic methods (`new`, `clear`, `draw_pixel`, `update_full`).
3. Add a small test/demo path in `main.rs` that initialises the display and performs a full refresh.
4. Iterate on error handling and ergonomics.
