# Prototype Zero

First prototype of "Kubobox" (just me learning a little embedded with an idea)

## Hardware

- **MCU**: ESP32 (Xtensa architecture)
- **Display**: Waveshare 2.13" e-Paper HAT (B) - 212×104 pixels, black/white
- **Scanner**: GM65 barcode scanner module (UART-based)

## Pin Configuration

### E-Paper Display (SPI2)
| ESP32 GPIO | Function | Waveshare HAT Pin |
| ---------- | -------- | ----------------- |
| 14         | SPI_SCLK | CLK               |
| 13         | SPI_MOSI | DIN               |
| 15         | CS       | CS                |
| 18         | DC       | DC                |
| 4          | RST      | RST               |
| 5          | BUSY     | BUSY              |
| 3.3V       | Power    | VCC               |
| GND        | Ground   | GND               |

### Barcode Scanner (UART1)
| ESP32 GPIO | Function               | GM65 Pin       |
| ---------- | ---------------------- | -------------- |
| 17         | UART TX (ESP32 → GM65) | Pin 3 (RXD)    |
| 16         | UART RX (GM65 → ESP32) | Pin 4 (TXD)    |
| 25         | Trigger control        | Pin 5 (TRIG)   |
| 26         | LED control            | Pin 7 (LED)    |
| 27         | BEEP control           | Pin 6 (BEEP)   |
| 5V         | Power (USB out)        | Pin 1 (VCC 5V) |
| GND        | Ground                 | Pin 2 (GND)    |

## Building & Flashing

### Prerequisites
- Rust with Xtensa toolchain
- ESP-IDF v5.3.3
- `espflash` for flashing firmware

### Setup
```bash
# The build system will automatically download ESP-IDF and Xtensa toolchain
# via embuild on first build
```

### Configure WiFi
Create a `.wifi_ssid` and `.wifi_password` file in the project root with your WiFi ssid and password:
```bash
echo "your_ssid_here" > .wifi_ssid
echo "your_password_here" > .wifi_password
```

### Build & Flash
```bash
# Build and run
cargo run

# Or just build
cargo build

# Flash with espflash
cargo espflash flash --monitor
```

## Documentation

- [E-Paper Hardware Specs](docs/epaper-hardware.md)
- [E-Paper Integration](docs/epaper-spec.md)
- [Barcode Scanner](docs/barcode-scanner.md)
- [GM65 Datasheet](docs/767.pdf)

## Dependencies
- `esp-idf-svc`: ESP-IDF services wrapper
- `esp-idf-hal`: Hardware abstraction layer
- `epd-waveshare`: E-paper display driver
- `embedded-graphics`: Graphics primitives

## License

See [LICENSE](LICENSE) file in repository.
