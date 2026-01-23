# COSMIC Pie Menu

A radial/pie menu app launcher for the [COSMIC desktop environment](https://system76.com/cosmic) that mirrors your dock favorites and applets.

![License](https://img.shields.io/badge/license-GPL--3.0-blue.svg)
![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)

![COSMIC Pie Menu Screenshot](docs/images/screenshot.png)

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
- **Dock Applets**: Includes App Library, Launcher, and Workspaces buttons from your dock
- **Running App Detection**: Shows which apps are currently running with arc indicators
- **Non-Favorite Running Apps**: Displays running apps that aren't dock favorites
- **Dynamic Sizing**: Menu radius scales based on number of apps
- **Dynamic Icon Positioning**: Icons positioned optimally based on pie size
- **Icon Support**: Displays app icons (SVG and PNG) with fallback to initials
- **Hover Highlighting**: Subtle segment highlighting as you move the mouse
- **Center Display**: Shows app name in the center when hovering
- **Transparent Background**: Only the circular menu is visible
- **Keyboard Support**: Press Escape to close, or click the center
- **System Tray**: Theme-aware tray icon for click-to-open access
- **Theme Support**: Tray icon adapts to light/dark mode changes
- **Autostart**: Automatically creates autostart entry on first run
- **Scaled Display Support**: Works correctly on HiDPI/scaled displays
- **Suspend/Resume Safe**: Uses full-screen layer surface for reliable display

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

The tray daemon automatically creates an autostart entry on first run at `~/.config/autostart/cosmic-pie-menu.desktop`. After running `cosmic-pie-menu` once, it will start automatically on login.

To manually set up autostart:

```bash
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

1. Reads dock applets from `~/.config/cosmic/com.system76.CosmicPanel.Dock/v1/plugins_center`
2. Reads dock favorites from `~/.config/cosmic/com.system76.CosmicAppList/v1/favorites`
3. Detects running applications via Wayland's `ext_foreign_toplevel_list_v1` protocol
4. Parses `.desktop` files to get app names, icons, and launch commands
5. Displays apps in a radial layout using libcosmic's layer-shell support
6. Click an app segment to launch it, or click the center/press Escape to close

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
│   ├── tray.rs       # System tray icon
│   └── windows.rs    # Running app detection via Wayland protocol
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

## Why No Mouse Activation?

Traditional pie menus (like [Kando](https://github.com/kando-menu/kando)) open at the cursor position when triggered by a mouse gesture or hotkey. This project currently opens the menu **centered on screen** instead. Here's why:

### The Wayland Security Model

Unlike X11, Wayland was designed with security in mind. One key restriction: **applications cannot query the global cursor position**. An app only knows where the cursor is when it's over that app's own window.

This is intentional—it prevents malicious apps from tracking your mouse movements across the desktop, monitoring which windows you're using, or capturing input intended for other applications.

### What This Means for Pie Menus

When you press a keyboard shortcut to open the pie menu:
1. The pie menu app starts with **no window yet**
2. It cannot ask "where is the cursor right now?"
3. It can only create a window and wait for cursor events *after* the window exists
4. By then, the window is already positioned

### How Other Apps Solve This

- **Kando** uses shell extensions (GNOME Shell, KDE KWin) that have privileged access to cursor position and expose it via D-Bus
- **Some apps** use a brief full-screen transparent overlay to "catch" the cursor position, then reposition—this adds latency and visual artifacts
- **COSMIC-native apps** could potentially use compositor-specific protocols, but these don't exist yet for this purpose

### Current Approach

This project uses **centered positioning**, which:
- Works reliably without compositor extensions
- Is predictable—you always know where the menu will appear
- Works well with keyboard shortcuts (the recommended activation method)

The `--track` mode attempts cursor tracking via a transparent overlay but falls back to centered after 500ms if it can't capture the position quickly enough.

### Future Possibilities

If COSMIC adds a protocol for trusted apps to query cursor position (similar to how it provides `ext_foreign_toplevel_list_v1` for window detection), this project could support cursor-positioned menus. Contributions implementing compositor-specific solutions are welcome.

## Known Issues

- **First Launch on Scaled Displays**: May briefly show incorrect size before correcting (within 500ms).
- **Web App Detection**: Some PWAs/web apps may not be detected if their app_id doesn't match a desktop file pattern.

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

- **Running App Detection**: COSMIC supports `ext_foreign_toplevel_list_v1` Wayland protocol for detecting running applications. This required subprocess isolation to avoid Wayland connection conflicts with libcosmic.

- **Full-Screen Layer Surface**: Using anchored full-screen surfaces (`Anchor::TOP | BOTTOM | LEFT | RIGHT`) is more reliable than fixed-size centered windows, especially after suspend/resume cycles.

- **Scaled Display Challenges**: HiDPI displays (e.g., 150% scaling) cause initial layout miscalculations. Solution: skip drawing until bounds correct + timer-based layout refresh.

- **Layer-Shell for Overlays**: COSMIC/Wayland's layer-shell protocol enables floating overlay windows without traditional window decorations.

- **Arc Drawing Quirks**: Standard canvas `arc()` functions behaved unexpectedly. Manual line-segment approximation gave predictable results.

- **Icon Discovery Complexity**: Finding the right icon involves multiple paths (system themes, Flatpak, alternate names) and format handling (SVG vs PNG). App IDs like "Slack" need fuzzy matching to find "com.slack.Slack.desktop".

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
