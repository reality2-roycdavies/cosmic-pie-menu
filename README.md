# COSMIC Pie Menu

A radial/pie menu app launcher for the [COSMIC desktop environment](https://system76.com/cosmic) that mirrors your dock favorites.

![License](https://img.shields.io/badge/license-GPL--3.0-blue.svg)
![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)

## About This Project

This project was developed collaboratively between **Dr. Roy C. Davies** and **Claude** (Anthropic's AI assistant) using [Claude Code](https://claude.ai/claude-code). The entire application—from initial concept to working release—was built through natural language conversation.

This is the third project in a series exploring human-AI collaboration for COSMIC desktop development:
1. [cosmic-bing-wallpaper](https://github.com/reality2-roycdavies/cosmic-bing-wallpaper) - Daily Bing wallpaper integration
2. [cosmic-runkat](https://github.com/reality2-roycdavies/cosmic-runkat) - Animated CPU monitor tray icon
3. **cosmic-pie-menu** (this project) - Radial app launcher

All three projects serve as case studies in AI-assisted software development, with complete documentation of the process.

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
├── docs/
│   ├── README.md             # Documentation overview
│   ├── DEVELOPMENT.md        # Technical learnings
│   ├── THEMATIC_ANALYSIS.md  # AI collaboration patterns
│   └── transcripts/          # Full development conversation
├── Cargo.toml
├── cosmic-pie-menu.desktop
├── LICENSE
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

## Development Documentation

The [docs/](docs/) directory contains detailed documentation about the development process:

| Document | Description |
|----------|-------------|
| [DEVELOPMENT.md](docs/DEVELOPMENT.md) | Technical learnings and solutions discovered |
| [THEMATIC_ANALYSIS.md](docs/THEMATIC_ANALYSIS.md) | Analysis of patterns in AI-assisted development |
| [transcripts/](docs/transcripts/) | Complete conversation logs from the development session |

### Key Technical Insights

From developing this project, several notable patterns emerged:

- **Canvas over Widgets**: Standard row/column layouts couldn't achieve true circular positioning. Canvas-based rendering with trigonometry provided full control over radial geometry.

- **Wayland Security Model**: Unlike X11, Wayland doesn't expose global cursor position to applications. This is a security feature, not a limitation to work around. The menu opens centered instead.

- **Scaled Display Challenges**: HiDPI displays (e.g., 150% scaling) cause initial layout miscalculations. Solution: skip drawing until bounds correct + timer-based layout refresh.

- **Layer-Shell for Overlays**: COSMIC/Wayland's layer-shell protocol enables floating overlay windows without traditional window decorations.

- **Arc Drawing Quirks**: Standard canvas `arc()` functions behaved unexpectedly. Manual line-segment approximation gave predictable results.

- **Icon Discovery Complexity**: Finding the right icon involves multiple paths (system themes, Flatpak, alternate names) and format handling (SVG vs PNG).

### Development Approach

The iterative development process demonstrated effective human-AI collaboration:

1. **Visual feedback loops** - UI evolved through "looks wrong" → code change → "better" cycles
2. **Platform discovery** - AI suggests approaches, real-world testing reveals actual behavior
3. **Scope flexibility** - Features like cursor-position menus were descoped when complexity exceeded value
4. **Accumulated learning** - Solutions from previous projects (ksni tray, layer-shell, config paths) were directly reusable

## License

This project is licensed under the GPL-3.0 License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- [System76](https://system76.com/) for the COSMIC desktop environment
- [libcosmic](https://github.com/pop-os/libcosmic) for the UI framework
- [iced](https://github.com/iced-rs/iced) for the underlying GUI library
- [Kando](https://github.com/kando-menu/kando) for pie menu inspiration
- [Claude](https://claude.ai/) (Anthropic) for AI-assisted development collaboration
