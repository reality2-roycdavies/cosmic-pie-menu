name := 'cosmic-pie-menu'
appid := 'io.github.reality2_roycdavies.cosmic-pie-menu'

# Default recipe: build release
default: build-release

# Build in debug mode
build-debug:
    cargo build

# Build in release mode
build-release:
    cargo build --release

# Run in debug mode
run:
    cargo run

# Run in release mode
run-release:
    cargo run --release

# Check code with clippy
check:
    cargo clippy --all-features

# Format code
fmt:
    cargo fmt

# Clean build artifacts
clean:
    cargo clean

# Install to local user
install-local:
    #!/bin/bash
    set -e

    echo "Stopping any running instances..."
    pkill -x "cosmic-pie-menu" 2>/dev/null || true
    sleep 1

    # Install binary
    mkdir -p ~/.local/bin
    rm -f ~/.local/bin/{{name}}
    cp target/release/{{name}} ~/.local/bin/

    # Install desktop entry
    mkdir -p ~/.local/share/applications
    cp resources/{{appid}}.desktop ~/.local/share/applications/

    # Install icons
    mkdir -p ~/.local/share/icons/hicolor/symbolic/apps
    cp resources/{{appid}}-symbolic.svg ~/.local/share/icons/hicolor/symbolic/apps/

    echo "Installation complete!"
    echo "Add the applet to your COSMIC panel to use it."

# Uninstall from local user
uninstall-local:
    rm -f ~/.local/bin/{{name}}
    rm -f ~/.local/share/applications/{{appid}}.desktop
    rm -f ~/.local/share/icons/hicolor/symbolic/apps/{{appid}}-symbolic.svg

# Build and run
br: build-debug run
