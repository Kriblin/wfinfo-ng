# WFinfo-ng

A Linux-compatible version of the [WFinfo](https://github.com/WFCD/WFinfo/) tool for Warframe, written in Rust. This project is a fork of [knoellle/wfinfo-ng](https://github.com/knoellle/wfinfo-ng).

WFinfo-ng helps you determine the value of relic rewards in real-time by capturing your screen, performing OCR (Optical Character Recognition) to detect item names, and looking up their current platinum and ducat values.

## Features

- **Relic Reward Detection**: Automatically detects when you are on a reward screen by monitoring `EE.log`.
- **OCR Integration**: Uses Tesseract to read item names from screenshots.
- **Value Lookup**: Displays platinum and ducat values for each detected item.
- **Transparent Overlay**: Real-time overlay showing reward values directly over the game window.
- **Multi-Server Support**: Compatible with both X11 and Wayland (via `xcap`).
- **Manual Trigger**: Trigger detection manually with a customizable hotkey (default: `F12`).
- **UI Theme Support**: Supports various Warframe UI themes (Vitruvian, Stalker, Baruuk, Corpus, etc.).

## Prerequisites

To build and run WFinfo-ng, you need the following system dependencies:

- **Rust**: Edition 2024 (stable recommended). Install via [rustup.rs](https://rustup.rs).
- **Tesseract OCR**: Required for OCR processing. Ensure English language data (`tessdata/eng.traineddata`) is installed.
- **libxrandr**: Required for X11 screenshot capturing.
- **curl & jq**: Required for the `update.sh` script to fetch data.

## Installation

1.  **Clone the repository**:
    ```bash
    git clone https://github.com/yourusername/wfinfo-ng.git
    cd wfinfo-ng
    ```

2.  **Setup the database**:
    The application requires item price and metadata to function. Run the update script:
    ```bash
    sh update.sh
    ```
    *Note: The application will also attempt to download this data on startup if it's missing.*

3.  **Build the project**:
    ```bash
    cargo build --release
    ```

4.  **Install the binary**:
    ```bash
    cargo install --path .
    ```

## Usage

### Running the Application

WFinfo-ng needs access to Warframe's `EE.log` file to detect reward screens automatically. On most Linux systems (via Steam/Proton), it is located at:
`~/.local/share/Steam/steamapps/compatdata/230410/pfx/drive_c/users/steamuser/AppData/Local/Warframe/EE.log`

Run `wfinfo` (optionally specifying the log path):
```bash
wfinfo --game-log-file-path /path/to/EE.log
```

If the log file is at the default location, you can simply run:
```bash
wfinfo
```

### Controls
- **F12**: Manually trigger screen capture and reward detection.

### Configuration

WFinfo-ng uses a YAML configuration file located at `~/.config/wfinfo-ng/config.yaml`.

| Option | CLI Argument | Description | Default |
| :--- | :--- | :--- | :--- |
| `game_log_file_path` | `-g`, `--game-log-file-path` | Path to Warframe's `EE.log` | Auto-detected |
| `window_name` | `-w`, `--window-name` | Title of the Warframe window | `Warframe` |
| `hotkey` | `--hotkey` | Hotkey for manual detection | `F12` |
| `capture_delay_ms` | `--capture-delay-ms` | Delay (ms) after event before capturing | `1500` |
| `log_level` | `--log-level` | Logging level (error, warn, info, debug, trace) | `info` |
| `log_timestamps` | `-l`, `--log-timestamps` | Enable timestamps in logs | `false` |

### Environment Variables
- `WFINFO_LOG`: Overrides the logging level (e.g., `WFINFO_LOG=debug`).
- `WFINFO_STYLE`: Controls terminal output styling (e.g., `WFINFO_STYLE=always`).

## Project Structure

- `src/lib.rs`: Core library defining modules for database, OCR, and models.
- `src/bin/main.rs`: Main entry point for the `wfinfo` binary (CLI + Overlay).
- `src/ocr/`: Tesseract OCR integration and image preprocessing.
- `src/database/`: Database loading and fuzzy item searching logic.
- `src/overlay/`: Egui-based implementation of the transparent HUD.
- `src/theme/`: Detection logic for different Warframe UI color themes.
- `update.sh`: Fetches latest price data from WarframeStat.us.

## Utility Binaries

The project includes several specialized tools:
- `theme_tune`: Debugging tool for tuning UI theme detection and color ranges.
- `ability-timer`: A small overlay tool to track ESO ability timeouts.
- `relics`: CLI tool to calculate and display potential relic values.
- `image`: Tool to batch process images for OCR verification and labeling.

## Testing

Run the test suite using Cargo:
```bash
cargo test
```
- **Unit Tests**: Found within individual module files.
- **Integration Tests**: OCR verification using images in `test-images/` and `WFI test images/`.

## License

This project is licensed under the **GNU General Public License v3.0** - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- This project is a fork of [knoellle/wfinfo-ng](https://github.com/knoellle/wfinfo-ng).
- Inspired by the [WFinfo](https://github.com/WFCD/WFinfo/) project by the WFCD team.
- Item and price data provided by [WarframeStat.us](https://warframestat.us/).
