# Development Documentation

This directory contains documentation about the development process of cosmic-pie-menu, created as an educational resource for understanding AI-assisted software development.

## About This Project

cosmic-pie-menu was developed collaboratively between **Dr. Roy C. Davies** and **Claude** (Anthropic's AI assistant) using [Claude Code](https://claude.ai/claude-code). The entire application—from initial concept to working, documented release—was built through natural language conversation.

This is the third project developed using this approach, following:
1. [cosmic-bing-wallpaper](https://github.com/reality2-roycdavies/cosmic-bing-wallpaper)
2. [cosmic-runkat](https://github.com/reality2-roycdavies/cosmic-runkat)

All three projects serve as case studies in human-AI collaboration for software development on the COSMIC desktop environment.

## Contents

| Document | Description |
|----------|-------------|
| [DEVELOPMENT.md](DEVELOPMENT.md) | Technical learnings and solutions discovered during development |
| [THEMATIC_ANALYSIS.md](THEMATIC_ANALYSIS.md) | Analysis of patterns in AI-assisted development |
| [transcripts/](transcripts/) | Complete conversation logs from the development session |

## Why Document This?

1. **Transparency** - Show exactly how AI-assisted development works, including the iterative process, mistakes, and corrections

2. **Education** - Help others understand the workflow of collaborating with AI on real software projects

3. **Research** - Provide data for studying human-AI collaboration patterns

4. **Reproducibility** - Allow others to learn from and build upon these techniques

## Key Insights

From developing this project, some notable observations:

- **Canvas-based UI for complex layouts** - When standard row/column layouts can't achieve the desired visual result (true circular positioning), canvas-based rendering provides full control
- **Wayland security model** - Cursor position isn't globally accessible in Wayland for security reasons, requiring different approaches than X11
- **Scaled display challenges** - HiDPI displays can cause initial layout miscalculations that require workarounds
- **Layer-shell for overlays** - COSMIC/Wayland's layer-shell protocol enables floating overlay windows without traditional window decorations
- **Icon discovery complexity** - Finding the right icon for an app involves multiple paths, alternate names, and format handling (SVG vs PNG)
- **Platform-specific icons** - COSMIC provides its own icons that match dock styling at `/usr/share/icons/hicolor/scalable/apps/`
- **Dynamic formulas over fixed ratios** - Layout calculations should adapt to context (number of items, available space)
- **Project patterns transfer** - Solutions from sibling projects (autostart, theme detection) apply directly
- **Theme color integration** - Use `cosmic::theme::system_preference()` and `background.component` colors to match dock
- **Gradient simulation** - Non-overlapping stroked rings create clean gradients without native gradient support
- **Transparency compositing** - Understanding the full rendering stack is essential for transparency effects

## Unique Challenges in This Project

Unlike the previous projects, cosmic-pie-menu required:

1. **True circular geometry** - Trigonometric calculations for positioning elements in a radial pattern
2. **Custom canvas rendering** - Drawing annular (ring) segments with proper arc calculations
3. **Mouse hit detection** - Determining which pie segment the cursor is over based on angle calculations
4. **Cross-display compatibility** - Handling both 100% and scaled displays with different initial bounds
5. **Dock applet integration** - Reading COSMIC panel plugin configuration and mapping applets to actions
6. **Dynamic icon positioning** - Formula-based positioning that adapts to pie size and number of items
7. **Multi-theme icon discovery** - Searching Pop, Adwaita, hicolor themes with COSMIC-specific priority

## Related Resources

- [cosmic-pie-menu main repository](https://github.com/reality2-roycdavies/cosmic-pie-menu)
- [cosmic-bing-wallpaper](https://github.com/reality2-roycdavies/cosmic-bing-wallpaper) - First project in this series
- [cosmic-runkat](https://github.com/reality2-roycdavies/cosmic-runkat) - Second project in this series
- [Claude Code](https://claude.ai/claude-code) - The AI coding assistant used
- [COSMIC Desktop](https://github.com/pop-os/cosmic-epoch) - The Linux desktop environment
- [Kando](https://github.com/kando-menu/kando) - Inspiration for pie menu concept

## License

This documentation is provided under the same GPL-3.0 license as the main project.
