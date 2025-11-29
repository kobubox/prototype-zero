# 2.13" e-Paper HAT (B) – Controller Commands

> This is a skeleton to be populated with the actual command set from the controller datasheet referenced on Waveshare’s wiki.

## Controller Identification
- **Controller**: (e.g.) `SSD1680`, `IL3895`, or related.
- Check the wiki/datasheet and fill this section with the exact model.

## Core Registers & Commands (Examples / Placeholders)

- `PANEL_SETTING` (0x??): configure resolution, LUT, scan direction.
- `POWER_SETTING` (0x??): power parameters.
- `POWER_ON` (0x??).
- `POWER_OFF` (0x??).
- `BOOSTER_SOFT_START` (0x??).
- `DISPLAY_REFRESH` (0x??).
- `DATA_START_TRANSMISSION_1` (0x??) – black/white RAM.
- `DATA_START_TRANSMISSION_2` (0x??) – color RAM (for tri-color panels).
- `PARTIAL_IN` / `PARTIAL_OUT` (0x??) if supported.

Each command needs:
- Command code (hex).
- Parameters (if any).
- Whether to follow with data bytes and how many.
- Whether we must wait for `BUSY` after the command.

## Init Sequence Skeleton

Document the ordered list of commands and params required at startup. For example (pseudo):

1. `POWER_SETTING(...)`
2. `POWER_ON`
3. Wait `BUSY` idle.
4. `PANEL_SETTING(...)`
5. `BOOSTER_SOFT_START(...)`
6. Set RAM X/Y ranges.
7. Clear RAM if needed.

## RAM Addressing & Framebuffer Layout
- How to map (x,y) pixel coordinates into the controller’s RAM.
- How bytes are arranged (e.g. each byte holds 8 horizontal pixels).
- Whether we must write line by line or can stream a full frame.

## TODO / To-Confirm
- Fill this document with concrete hex values and sequences from the datasheet.
- Decide which subset of commands we need for:
  - Full refresh.
  - Optional partial refresh.
  - Deep sleep / wake.
