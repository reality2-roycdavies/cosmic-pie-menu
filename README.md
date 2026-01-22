# COSMIC Pie Menu

A radial/pie menu app launcher for the [COSMIC desktop environment](https://system76.com/cosmic) that mirrors your dock favorites.

![License](https://img.shields.io/badge/license-GPL--3.0-blue.svg)
![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)

## Features

- **Radial Layout**: Apps arranged in a circular pie menu for quick access
- **Dock Integration**: Automatically reads favorites from COSMIC dock configuration
- **Icon Support**: Displays app icons (SVG and PNG) with fallback to initials
- **Hover Highlighting**: Subtle segment highlighting as you move the mouse
- **Center Display**: Shows app name in the center when hovering
- **Transparent Background**: Only the circular menu is visible
- **Keyboard Support**: Press Escape to close, or click the center
- **System Tray**: Optional tray icon for click-to-open access
- **Scaled Display Support**: Works correctly on HiDPI/scaled displays

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/reality2-roycdavies/cosmic-pie-menu.git
cd cosmic-pie-menu

# Build in release mode
cargo build --release

# Install the binary
sudo cp target/release/cosmic-pie-menu /usr/local/bin/
```

### Dependencies

- Rust 1.75 or later
- COSMIC desktop environment (or libcosmic)
- D-Bus (for system tray)

## Usage

### Direct Launch

Show the pie menu directly:

```bash
cosmic-pie-menu --pie
```

### With System Tray

Run the daemon with tray icon:

```bash
cosmic-pie-menu
```

Then click the tray icon to show the pie menu.

### Keyboard Shortcut (Recommended)

1. Open **COSMIC Settings**
2. Navigate to **Keyboard** → **Keyboard Shortcuts**
3. Add a custom shortcut:
   - **Command**: `cosmic-pie-menu --pie`
   - **Shortcut**: Your preferred key combo (e.g., `Super+Space` or `Ctrl+Alt+P`)

### Autostart

To start the tray daemon automatically:

```bash
# Copy desktop file to autostart
cp cosmic-pie-menu.desktop ~/.config/autostart/

# Or create manually
mkdir -p ~/.config/autostart
cat > ~/.config/autostart/cosmic-pie-menu.desktop << EOF
[Desktop Entry]
Name=COSMIC Pie Menu
Exec=cosmic-pie-menu
Type=Application
X-GNOME-Autostart-enabled=true
EOF
```

## How It Works

1. Reads your dock favorites from `~/.config/cosmic/com.system76.CosmicAppList/v1/favorites`
2. Parses `.desktop` files to get app names, icons, and launch commands
3. Displays apps in a radial layout using libcosmic's layer-shell support
4. Click an app segment to launch it, or click the center/press Escape to close

## Configuration

Currently, the pie menu reads directly from the COSMIC dock favorites. To change which apps appear:

1. Open COSMIC Settings → Dock
2. Add or remove apps from your dock favorites
3. The pie menu will reflect these changes on next launch

Future versions may include a dedicated settings interface.

## Building

### Requirements

- Rust toolchain (rustup recommended)
- System dependencies for libcosmic:
  ```bash
  # Debian/Ubuntu
  sudo apt install libwayland-dev libxkbcommon-dev libssl-dev pkg-config

  # Fedora
  sudo dnf install wayland-devel libxkbcommon-devel openssl-devel

  # Arch
  sudo pacman -S wayland libxkbcommon openssl
  ```

### Build Commands

```bash
# Debug build
cargo build

# Release build (recommended)
cargo build --release

# Run directly
cargo run -- --pie
```

## Project Structure

```
cosmic-pie-menu/
├── src/
│   ├── main.rs       # Entry point and tray event loop
│   ├── apps.rs       # Desktop file parsing and icon lookup
│   ├── config.rs     # COSMIC dock config reader
│   ├── pie_menu.rs   # Radial menu UI (canvas-based)
│   └── tray.rs       # System tray icon
├── Cargo.toml
├── cosmic-pie-menu.desktop
└── README.md
```

## Known Issues

- **Cursor Position**: Currently opens centered on screen. Wayland security model restricts access to global cursor position.
- **First Launch on Scaled Displays**: May briefly show incorrect size before correcting (within 500ms).

## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

This project is licensed under the GPL-3.0 License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- [System76](https://system76.com/) for the COSMIC desktop environment
- [libcosmic](https://github.com/pop-os/libcosmic) for the UI framework
- [iced](https://github.com/iced-rs/iced) for the underlying GUI library
- [Kando](https://github.com/kando-menu/kando) for pie menu inspiration
