# 2.13" e-Paper HAT (B) – SPI & Timing

> This file should be filled out from the official Waveshare wiki timing diagrams and the controller datasheet.

## SPI Bus Parameters (To Confirm)
- **Mode**: CPOL/CPHA (often Mode 0 for these controllers).
- **Max clock**: typically a few MHz (e.g. 4 MHz), check docs.
- **Bit order**: MSB-first.
- **Word size**: 8 bits per transfer.

## Data/Command Protocol
- `CS` low selects the device.
- `DC` low: the following byte is a *command*.
- `DC` high: the following bytes are *data*.
- `BUSY` pin indicates when the controller is busy processing operations.

## Reset Sequence (Typical Pattern)
1. Pull `RST` low for at least X ms.
2. Pull `RST` high.
3. Wait for `BUSY` to indicate ready (high or low depending on controller).
4. Send initialization command sequence.

## Busy-Wait Pattern
- After certain commands (e.g. `DISPLAY_REFRESH`), the panel will set `BUSY` until internal driving is complete.
- Pseudocode:

```rust
while busy_pin_is_busy() {
    delay_ms(10);
}
```

We’ll implement a helper `wait_until_idle(&mut self)` that polls `BUSY` with a timeout.

## TODO / To-Confirm from Docs
- Exact SPI mode.
- Maximum safe SPI clock frequency.
- Exact reset timing requirements (ms values).
- Which commands require waiting on `BUSY`.
- Whether partial refresh has special timing constraints.
