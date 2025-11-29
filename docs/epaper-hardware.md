# 2.13" Waveshare e-Paper HAT (B) – Hardware Notes

> Source: Waveshare wiki for 2.13" e-Paper HAT (B) + linked controller datasheet. Please verify against your exact board silkscreen and documentation.

## Panel & Controller
- **Module**: 2.13" e-Paper HAT (B).
- **Resolution**: 212 × 104 pixels (to be confirmed from datasheet).
- **Type**: e-Paper, black/white (some variants add a third color – red).
- **Controller**: model varies by revision (e.g. IL3895/SSD1680/UC8151-like). Exact command set must be looked up in Waveshare docs.

## Electrical Interface
- **Power**:
  - `VCC` (typically 3.3 V for ESP32 use case).
  - `GND`.
- **SPI**:
  - `CLK`  – SPI clock.
  - `DIN`  – SPI MOSI.
  - `CS`   – chip select for the display.
- **Control pins**:
  - `DC`   – Data/Command select.
  - `RST`  – hardware reset for the e-Paper controller.
  - `BUSY` – status output from controller (busy/idle).

## Typical ESP32 Pin Mapping

Waveshare’s ESP32 example uses a specific pinout. We will define our own mapping in Rust but document a recommended default here (adapt after checking the wiki section):

- `CLK`  → `GPIO18` (SPI SCK)
- `DIN`  → `GPIO23` (SPI MOSI)
- `CS`   → e.g. `GPIO5`
- `DC`   → e.g. `GPIO17`
- `RST`  → e.g. `GPIO16`
- `BUSY` → e.g. `GPIO4`

> These are examples—check the Waveshare wiki ESP32 table and your board wiring.

## Orientation & Coordinates

- Default coordinate system (assumed, to verify):
  - X: 0..(WIDTH-1) left to right.
  - Y: 0..(HEIGHT-1) top to bottom.
- Physical HAT orientation may be landscape (212 wide × 104 high) or portrait; we’ll pick one canonical orientation in the driver and document it.

## TODO / To-Confirm
- Confirm exact controller IC model used by your specific 2.13" HAT (B).
- Confirm resolution in the official datasheet.
- Confirm pin mapping and logic levels for BUSY (high when busy vs. low when busy).
- Capture any special power-up or power-down sequences from the docs.
