## 1. Overview

* **Module type:** GM65 1D/2D barcode reader module (commonly used inside Sunrom 767 scanner boards) 
* **Role:** Reads 1D + 2D barcodes, including QR, DataMatrix, PDF417, etc.
* **Interface options:**

  * **TTL-232 UART** (3.3V signalling) — default interface 
  * USB (keyboard mode)
  * USB Virtual COM
* **Default UART configuration:**

  * **9600 baud**, **8 data bits**, **no parity**, **1 stop bit**, **no flow control** (“9600 8N1”) 
* **Default scan mode:** **Manual mode** (scan only when trigger pressed) 
* **Output formatting:** ASCII + optional prefix, suffix, Code ID, and tail (CR/LF/CRLF/TAB).
  Tail types configured via zone bit `0x0060`. 
* **Supported symbologies:** All major 1D & 2D types — QR, Code128, Code39, EAN13, DataMatrix, PDF417, etc. Individual enable/disable supported. 

---

## 2. Electrical Integration

### 2.1 Power

From the GM65 electrical specification:

* **Operating Voltage:** **4.2–6.0V DC** (typically 5V) 
* **Standby current:** 30 mA
* **Operating current:** 160 mA
* **Sleep current:** 3 mA

The GM65 should be powered from **5V**, not the ESP32’s 3.3V rail.

> UART pins are **TTL-232 (3.3V logic)** — safe for direct ESP32 connection. 

### 2.2 UART Signals

* GM65 **TX** → ESP32 **RX** (e.g., GPIO16 / U1RXD)
* GM65 **RX** → ESP32 **TX** (e.g., GPIO17 / U1TXD)
* GM65 **GND** ↔ ESP32 **GND**
* GM65 **VCC** → ESP32 **5V pin**

Default UART settings: **9600 8N1** 

### 2.3 Trigger Pin (Optional)

GM65 supports:

* Manual (hardware button)
* **Command-triggered mode** (serial command + zone bit) 

Sunrom boards may expose a **TRIG** pin; tie it to an ESP32 GPIO (e.g., GPIO25).

---

## 3. Protocol & Data Framing

GM65 sends **plain ASCII text** when decoding succeeds:

* Optional prefix, suffix, Code ID
* Optional tail: CR, CRLF, TAB, or none (zone bit 0x0060) 

Defaults:

* **No prefix**
* **No suffix**
* **Tail disabled (none)**
* **Code ID disabled**

(all from zone bits 0x0060–0x0080) 

So default output is simply:

`<barcode><optional CR depending on Sunrom board>`

### 3.1 Parsing Strategy

1. Read bytes from UART.
2. Accumulate until CR/LF/CRLF.
3. Trim whitespace.
4. Emit a `BarcodeEvent::Scanned(String)`.

---

## 4. Rust Crate / Library Options

Use:

* `esp-idf-hal` for UART access
* Your existing event-thread architecture (similar to blinker and e-paper)

GM65 requires no special protocol beyond ASCII reading.

---

## 5. Proposed Rust Module Design

### 5.1 Public API

```rust
#[derive(Debug, Clone)]
pub enum BarcodeEvent {
    Scanned(String),
    Error(String),
}

#[derive(Clone)]
pub struct BarcodeHandle {}

pub struct BarcodeScanner {
    handle: BarcodeHandle,
}

impl BarcodeScanner {
    pub fn start<F>(
        uart: esp_idf_hal::uart::UartDriver,
        on_event: F,
    ) -> anyhow::Result<Self>
    where
        F: 'static + Send + FnMut(BarcodeEvent),
    {
        // spawn worker thread
    }

    pub fn handle(&self) -> BarcodeHandle {
        self.handle.clone()
    }
}
```

### 5.2 Worker Thread Logic (Simplified)

```rust
const MAX_CODE_LEN: usize = 128;

loop {
    match uart.read(&mut buf, timeout_ms) {
        Ok(1) => {
            let b = buf[0];
            if b == b'\r' || b == b'\n' {
                let s = String::from_utf8_lossy(&line).trim().to_string();
                line.clear();
                if !s.is_empty() {
                    on_event(BarcodeEvent::Scanned(s));
                }
            } else if line.len() < MAX_CODE_LEN {
                line.push(b);
            } else {
                line.clear();
                on_event(BarcodeEvent::Error("Barcode too long".into()));
            }
        }
        Ok(0) => continue,
        Err(e) => on_event(BarcodeEvent::Error(format!("UART read error: {e:?}"))),
    }
}
```

---

## 6. Integration with Existing Architecture

### 6.1 `main.rs` Wiring

```rust
let uart_config = UartConfig::new()
    .baudrate(9600)
    .data_bits(DataBits::DataBits8)
    .parity_none()
    .stop_bits(STOP1);

let uart_pins = UartPins {
    tx: periph.pins.gpio17,
    rx: periph.pins.gpio16,
    cts: None,
    rts: None,
};

let uart = UartDriver::new(Uart::U1, &uart_pins, &uart_config)?;

let barcode_scanner = BarcodeScanner::start(uart, move |event| match event {
    BarcodeEvent::Scanned(code) => {
        log::info!("Scanned: {}", code);
        let _ = display_handle.submit(DisplayJob::UpdateLine {
            line_number: 0,
            text: code,
        });
    }
    BarcodeEvent::Error(err) => log::warn!("Barcode error: {}", err),
})?;
```

### 6.2 Optional HTTP Integration

Store last scanned code in shared state and expose via HTTP.

---

## 7. Datasheet-Confirmed Parameters

### 7.1 Default UART Parameters

(from Form 2-1)

* **Baud:** **9600** 
* **Data bits:** 8
* **Parity:** None
* **Stop bits:** 1
* **Flow control:** None

### 7.2 Supported Baud Rates

**1200, 4800, 9600, 14400, 19200, 38400, 57600, 115200**
(configurable via zone bits 0x002B/0x002A) 

### 7.3 Scan Modes

(zone bit 0x0000 bits 1–0)

* `00` Manual (default) 
* `01` Command-triggered
* `10` Continuous
* `11` Sensor-mode

### 7.4 Timing

* **Default single-read timeout:** 5s 
* **Default inter-read interval:** 1s 
* Both configurable: 0.1–25.5s (0 = infinite)

### 7.5 Lighting / Aiming

(zone bit 0x0000)

* Bits 5–4: aim mode (no aim / standard / always on)
* Bits 3–2: illumination (no light / standard / always on)

Default: **standard lighting + standard aim** 

### 7.6 Decoding Beep

* Sound index: zone bit 0x000A
* Duration: zone bit 0x000B

Default duration: **60ms** 

### 7.7 Encoding Format

(zone bit 0x000D bits 3–2)

* `00` GBK (default)
* `01` UNICODE
* `10` BIG5 

### 7.8 Tail Options

(zone bit 0x0060 bits 6–5)

* `00` CR
* `01` CRLF
* `10` TAB
* `11` None (default) 

### 7.9 Symbology Defaults

Most common barcodes enabled by default:

* QR (zone bit 0x003F)
* Code128, Code39, Code93
* EAN13/EAN8
* UPC
* DataMatrix
* PDF417

All toggled via zone bits 0x002E–0x0055. 

### 7.10 Command-Triggered Mode

Trigger command (from section 3.4):

```text
7E 00 08 01 00 02 01 AB CD
```

This starts scanning after an acknowledgment reply. 
