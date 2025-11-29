# 2.13" Waveshare e-Paper Rust Driver – Design Spec

## Status Update (Nov 2025)

✅ **Resolved**: `esp-idf-hal` 0.45.2 supports both `embedded-hal` 0.2 **and** 1.0 traits. We're now using:
- `epd-waveshare` 0.6 (with `embedded-hal` 1.0)
- `embedded-hal` 1.0
- `esp-idf-hal` 0.45.2

The `epaper` module in `src/epaper.rs` compiles cleanly and is ready to use once you wire SPI + GPIO pins.

---

## Goals
- Control the Waveshare 2.13" e-Paper HAT (B) from an ESP32 using Rust.
- Wrap low-level SPI + GPIO details behind a safe, async-friendly API.
- Provide primitives for full refreshes, partial updates (if supported), and simple drawing (clear screen, draw pixels, text later).
- Keep it `no_std`-friendly where possible and re-usable outside this specific project.

## Hardware Overview (High Level)
- **Panel size**: 2.13" e-Paper HAT (B) (Waveshare).
- **Resolution**: 212 × 104 pixels (landscape) – controller-specific, confirm from datasheet.
- **Color**: B/W (possibly B/W/Red depending on exact variant – confirm your board silk and wiki section).
- **Interface**: 4-wire SPI plus control pins.
- **Typical pins** (per Waveshare ESP32/ESP8266 example):
  - `VCC`, `GND` – power.
  - `DIN`  – MOSI.
  - `CLK`  – SCK.
  - `CS`   – chip select.
  - `DC`   – data/command select.
  - `RST`  – hardware reset.
  - `BUSY` – panel busy indicator (high/low depending on controller).

We will mirror the naming in the Rust driver API: `cs`, `dc`, `rst`, `busy`, `spi`.

## Target Rust Environment
- **MCU**: ESP32 (esp-idf, Rust).
- **HAL**: `esp-idf-hal` for SPI + GPIO + delays.
- **Runtime**: `std` available (for now) via `esp-idf` Rust setup, but design should not assume `std` where not needed.
- **Build system**: current `blink` crate, but driver should be factored as a separate module / future crate (e.g. `epd213_waveshare`).

## MVP Feature Set

### 1. Initialization
- Provide an `Epd213` struct wrapping the panel.
- Methods:
  - `fn new(spi, cs, dc, rst, busy, delay) -> Result<Self, Error>`
    - Takes ownership of an SPI device and the required GPIO pins.
    - Runs the controller init sequence:
      - Hardware reset (RST pulse).
      - Send required register/config commands over SPI (per Waveshare docs).
      - Wait on `BUSY` between phases as required.
  - `fn sleep(&mut self) -> Result<(), Error>`
    - Put display into deep sleep (for power saving).

### 2. Framebuffer & Drawing
- Represent the display as a 1-bit framebuffer (for B/W variant):
  - Likely a `Vec<u8>` or fixed-size `[u8; N]` where `N = (WIDTH * HEIGHT) / 8`.
- Methods:
  - `fn clear(&mut self, color: Color) -> Result<(), Error>`
    - Fill framebuffer with white/black and update display.
  - `fn draw_pixel(&mut self, x: u16, y: u16, color: Color)`
    - Update only the local framebuffer.
  - `fn update_full(&mut self) -> Result<(), Error>`
    - Transfer full framebuffer to the panel and trigger a full refresh.

> MVP note: partial refresh is trickier and more controller-specific; we can design for it but implement later.

### 3. Basic Types
- `enum Color { Black, White }` (and maybe `Red` if your HAT is tri-color).
- `struct Epd213<SPI, CS, DC, RST, BUSY, DELAY>`
  - `SPI`: an embedded-hal compatible blocking SPI.
  - `CS`, `DC`, `RST`: output pins.
  - `BUSY`: input pin.
  - `DELAY`: blocking delay provider (e.g. `FreeRtos` wrapper or `embedded_hal::delay::DelayMs`).

### 4. Error Handling
- Custom error enum wrapping:
  - SPI errors.
  - GPIO errors (pin set/read).
  - Timing/invalid state (optional).
- Implement `From` where appropriate to make construction ergonomic.

## API Sketch

```rust
pub enum Color {
    Black,
    White,
}

pub struct Epd213<SPI, CS, DC, RST, BUSY, DELAY> {
    spi: SPI,
    cs: CS,
    dc: DC,
    rst: RST,
    busy: BUSY,
    delay: DELAY,
    buffer: [u8; BUFFER_SIZE],
}

impl<SPI, CS, DC, RST, BUSY, DELAY> Epd213<SPI, CS, DC, RST, BUSY, DELAY>
where
    SPI: embedded_hal::blocking::spi::Write<u8>,
    CS: embedded_hal::digital::v2::OutputPin,
    DC: embedded_hal::digital::v2::OutputPin,
    RST: embedded_hal::digital::v2::OutputPin,
    BUSY: embedded_hal::digital::v2::InputPin,
    DELAY: embedded_hal::blocking::delay::DelayMs<u16>,
{
    pub fn new(
        spi: SPI,
        cs: CS,
        dc: DC,
        rst: RST,
        busy: BUSY,
        delay: DELAY,
    ) -> Result<Self, Error> {
        // hw reset + controller init sequence
        // (will use values documented in controller manual)
        # unimplemented!()
    }

    pub fn clear(&mut self, color: Color) -> Result<(), Error> {
        // fill buffer + send to display
        # unimplemented!()
    }

    pub fn draw_pixel(&mut self, x: u16, y: u16, color: Color) {
        // set bit in buffer only
        # unimplemented!()
    }

    pub fn update_full(&mut self) -> Result<(), Error> {
        // send full buffer over SPI
        # unimplemented!()
    }
}
```

This is a sketch, not final code – we will refine once we have the exact controller commands from the Waveshare docs.

## Waveshare Docs & Data We Need to Pull In

From the Waveshare wiki and datasheets, we need to capture into separate docs files:

1. **Panel & Controller Info** (`docs/epaper-hardware.md`)
   - Exact panel variant (B/W or tri-color).
   - Resolution (WIDTH, HEIGHT).
   - Coordinate system (origin, orientation for the HAT).
   - Typical connection diagram for ESP32 (pin mapping).

2. **SPI & Pin Timing** (`docs/epaper-spi-timing.md`)
   - SPI mode (CPOL/CPHA), max frequency.
   - Data/command protocol: how bytes are framed.
   - Meaning of `DC`, `CS`, `RST`, `BUSY`.
   - Reset timing (how long RST low/high, BUSY wait sequences).

3. **Controller Command Set** (`docs/epaper-controller-commands.md`)
   - Controller model (e.g. IL3895/SSD1680/UC8151 or similar).
   - Init sequence: ordered list of commands and params.
   - Memory layout: how to write RAM for the panel (addressing).
   - Update/refresh commands and waveform modes (full/partial if available).

4. **Rust Integration Notes** (`docs/epaper-rust-integration.md`)
   - How we map ESP32 pins to the driver generics.
   - How we construct the SPI bus/device using `esp-idf-hal`.
   - Example: initialize driver in `main` and render a test pattern.

## Next Steps

1. Copy relevant details from Waveshare wiki & controller datasheet into the `docs/` markdown files listed above.
2. Finalise constants: WIDTH, HEIGHT, buffer size, SPI settings.
3. Lock down the `Epd213` public API based on what the hardware supports.
4. Implement a first-pass driver module under `src/epaper.rs` using the agreed spec.
5. Add a simple demo in `main.rs` that clears the screen and draws a small pattern.
