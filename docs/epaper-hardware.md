# 2.13" Waveshare e-Paper HAT (B) – Hardware Specifications

## Display Module
- **Model**: Waveshare 2.13" e-Paper HAT (B)
- **Resolution**: 212 × 104 pixels
- **Display Type**: Black/White e-Paper
- **Controller**: SSD1680-based (handled by epd-waveshare crate)
- **Refresh Time**: ~2s full refresh, ~200ms partial refresh

## Electrical Interface

### Power
- **VCC**: 3.3V (from ESP32)
- **GND**: Ground

### SPI Interface
| Signal | Function | Notes |
|--------|----------|-------|
| CLK | SPI Clock | Driven by ESP32 SPI peripheral |
| DIN | SPI MOSI | Data to display |
| CS | Chip Select | Active low |

### Control Pins
| Pin | Function | Behavior |
|-----|----------|----------|
| DC | Data/Command | Low = Command, High = Data |
| RST | Reset | Active low, hardware reset |
| BUSY | Status | High = busy, Low = ready |

## ESP32 Pin Mapping

Current implementation uses the following GPIO assignments:

| Function | GPIO | Notes |
|----------|------|-------|
| SPI_SCLK | GPIO14 | SPI2 clock |
| SPI_MOSI | GPIO13 | SPI2 MOSI (DIN) |
| CS | GPIO15 | Chip select |
| DC | GPIO18 | Data/command |
| RST | GPIO4 | Reset |
| BUSY | GPIO5 | Busy status input |

## SPI Configuration
- **Baudrate**: 4 MHz
- **Mode**: MODE_0 (CPOL=0, CPHA=0)
- **Peripheral**: SPI2

## Driver Integration

This project uses the `epd-waveshare` crate (v0.6) which handles all low-level SPI communication and controller initialization. The driver:
- Manages SPI timing and command sequences automatically
- Provides embedded-graphics trait implementations
- Supports both full and partial refresh modes
- Handles display rotation via `DisplayRotation` enum

See `epaper-spec.md` for implementation details and usage patterns.
