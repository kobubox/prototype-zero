# 2.13" Waveshare e-Paper Integration

## Current Implementation (Nov 2025)

✅ **Using `epd-waveshare` crate 0.6** with `embedded-hal` 1.0 support.

The e-paper display is fully integrated via:
- `epd-waveshare` 0.6 – pre-built driver for Waveshare e-Paper displays
- `embedded-graphics` 0.8 – for text rendering and drawing primitives
- `embedded-hal` 1.0 – trait abstractions
- `esp-idf-hal` 0.45.2 – ESP32 hardware support (dual embedded-hal 0.2/1.0 compatibility)

Our `src/epaper.rs` module wraps the `epd-waveshare::epd2in13_v2::Epd2in13` driver with:
- FreeRTOS-based worker thread for async display updates
- Event-driven architecture via `DisplayJob` enum
- Persistent framebuffer for partial refresh support
- `RefreshLut::Quick` mode for fast partial updates, `Full` for complete refreshes

---

## Hardware Overview

- **Panel**: Waveshare 2.13" e-Paper HAT (B) V4
- **Resolution**: 250 × 122 pixels (after 90° rotation)
- **Colors**: Black, White, Red (tri-color)
- **Interface**: 4-wire SPI + control pins
- **Pin mapping**:
  - GPIO13 → MOSI (DIN)
  - GPIO14 → SCLK
  - GPIO15 → CS
  - GPIO18 → DC
  - GPIO4  → RST
  - GPIO5  → BUSY

---

## Architecture

### DisplayJob System

```rust
pub enum DisplayJob {
    Clear,                                    // Full screen clear with full refresh
    ShowText(String),                         // Full screen text with full refresh
    UpdateLine { line_number: u8, text: String }, // Partial line update (fast)
}
```

### Refresh Modes

- **Full Refresh** (`RefreshLut::Full`): Used for `Clear` and `ShowText` jobs
  - Clears ghosting
  - Takes ~2 seconds
  - Resets display completely
  
- **Quick Refresh** (`RefreshLut::Quick`): Used for `UpdateLine` jobs
  - Fast partial updates (~200ms)
  - No screen flash
  - Requires base buffer synchronization via `set_partial_base_buffer()`

### Worker Thread Pattern

The display manager runs in a dedicated thread (8KB stack) to avoid blocking the main application:

```rust
let display_manager = DisplayManager::start(spi, cs, dc, rst, busy)?;
let display_handle = display_manager.handle();

display_handle.submit(DisplayJob::Clear)?;
display_handle.submit(DisplayJob::UpdateLine { line_number: 0, text: "Hello".into() })?;
```

---

## Integration Notes

The `epd-waveshare` crate handles all low-level SPI communication and controller commands. Our wrapper provides:

1. **Thread safety** – Move hardware into worker thread, communicate via channels
2. **Persistent framebuffer** – Maintain screen state across jobs for partial updates
3. **Smart refresh mode switching** – Automatic Full/Quick mode selection based on job type
4. **Error propagation** – Return errors via `Result` instead of panicking in worker thread

---

## Reference Documentation

For detailed hardware and protocol information, see:
- `docs/epaper-hardware.md` – Pin mapping and electrical specs
- `docs/barcode-scanner.md` – Barcode scanner integration (shares some GPIO space considerations)

For driver implementation details:
- [epd-waveshare crate docs](https://docs.rs/epd-waveshare/0.6)
- Source: `src/epaper.rs`
