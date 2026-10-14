#!/usr/bin/env python3
"""Capture the actual TUI in a PTY and render its terminal cells as PNGs.

Requires pyte and Pillow. No UI text is synthesized by the renderer.
"""

import argparse
import math
import os
from pathlib import Path
import runpy
import subprocess
import tempfile

from PIL import Image, ImageDraw, ImageFont
from pyte.graphics import FG_BG_256

Terminal = runpy.run_path(str(Path(__file__).with_name("pty-smoke.py")))["Terminal"]

# An explicit terminal palette keeps screenshots consistent across machines.
PALETTE = {
    "black": "#1e1e2e", "red": "#f38ba8", "green": "#a6e3a1",
    "brown": "#f9e2af", "blue": "#89b4fa", "magenta": "#cba6f7",
    "cyan": "#89dceb", "white": "#cdd6f4", "brightblack": "#7f849c",
    "brightred": "#f38ba8", "brightgreen": "#a6e3a1", "brightbrown": "#f9e2af",
    "brightblue": "#89b4fa", "brightmagenta": "#cba6f7",
    "brightcyan": "#89dceb", "brightwhite": "#ffffff",
}
# pyte resolves indexed ANSI colors to hex; terminal themes customize these too.
PALETTE.update(zip(FG_BG_256[:16], tuple(PALETTE.values())))


def color(value, default):
    if value == "default":
        return default
    return PALETTE.get(value, f"#{value}")


def render(screen, path, font_path):
    font = ImageFont.truetype(str(font_path), 18)
    bold_path = font_path.with_name(font_path.name.replace("Regular", "Bold"))
    bold = ImageFont.truetype(str(bold_path if bold_path.exists() else font_path), 18)
    cell_width, cell_height, padding = math.ceil(font.getlength("M")), 26, 20
    background, foreground = "#1e1e2e", "#cdd6f4"
    canvas = Image.new("RGB", (screen.columns * cell_width + padding * 2,
                               screen.lines * cell_height + padding * 2), background)
    draw = ImageDraw.Draw(canvas)
    ascent, descent = font.getmetrics()
    baseline = (cell_height - ascent - descent) // 2 + ascent
    # Paint backgrounds first so a double-width glyph is never painted over.
    for y in range(screen.lines):
        for x in range(screen.columns):
            cell = screen.buffer[y][x]
            fg, bg = color(cell.fg, foreground), color(cell.bg, background)
            if cell.reverse:
                fg, bg = bg, fg
            left, top = padding + x * cell_width, padding + y * cell_height
            draw.rectangle((left, top, left + cell_width - 1, top + cell_height - 1), fill=bg)
    for y in range(screen.lines):
        for x in range(screen.columns):
            cell = screen.buffer[y][x]
            fg = color(cell.bg, background) if cell.reverse else color(cell.fg, foreground)
            left, top = padding + x * cell_width, padding + y * cell_height
            draw.text((left, top + baseline), cell.data, font=bold if cell.bold else font,
                      fill=fg, anchor="ls")
            if cell.underscore:
                draw.line((left, top + baseline + 2, left + cell_width - 1, top + baseline + 2), fill=fg)
    canvas.save(path, optimize=True)
    print(f"Captured {path} ({canvas.width}x{canvas.height})")


def fixture(directory):
    def git(*args):
        subprocess.run(["git", *args], cwd=directory, check=True, capture_output=True)

    def write(name, content):
        path = directory / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    git("init", "--template=")
    git("symbolic-ref", "HEAD", "refs/heads/main")
    git("config", "user.name", "Demo")
    git("config", "user.email", "demo@example.invalid")
    git("config", "commit.gpgsign", "false")
    write("Cargo.toml", '[package]\nname = "terminal-demo"\nversion = "0.1.0"\nedition = "2024"\n')
    write("README.md", "# Terminal demo\n\nA small Rust project for exploring local Git workflows.\n")
    original = '''use std::path::Path;

fn main() {
    let repository = Path::new(".");
    println!("Repository: {}", repository.display());
    println!("Ready to explore your changes.");
}
'''
    write("src/main.rs", original)
    git("add", "--all")
    git("commit", "-m", "chore: start terminal client")
    write("src/keys.rs", '''pub enum Action {
    Stage,
    Unstage,
    Commit,
    Refresh,
}
''')
    git("add", "--all")
    git("commit", "-m", "feat: add keyboard actions")
    write("src/status.rs", '''pub fn status_label(staged: usize, modified: usize) -> String {
    match (staged, modified) {
        (0, 0) => "Working tree clean".into(),
        (s, m) => format!("{s} staged, {m} modified"),
    }
}
''')
    git("add", "--all")
    git("commit", "-m", "feat: show repository status")
    git("branch", "feature/diff-preview")
    git("checkout", "-b", "feature/mouse")
    write("src/main.rs", '''use std::path::Path;

mod status;

fn main() {
    let repository = Path::new(".");
    println!("Repository: {}", repository.display());
    println!("{}", status::status_label(2, 1));
    println!("Click a file to inspect its diff.");
}
''')
    write("README.md", "# Terminal demo\n\nExplore changes with your keyboard or mouse.\n")
    write("src/mouse.rs", "pub fn supports_mouse() -> bool {\n    true\n}\n")
    git("add", "README.md", "src/mouse.rs")
    write("docs/keybindings.md", "# Keyboard shortcuts\n\ns: stage   u: unstage   c: commit\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--font", required=True, type=Path, help="Monospaced TrueType font")
    parser.add_argument("--output", type=Path, default=Path("docs/images"))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    # Capture a normal color terminal even when the invoking shell disables color.
    os.environ.pop("NO_COLOR", None)
    # Isolate the demo from personal Git config and make commit identities stable.
    os.environ.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull,
                      GIT_AUTHOR_DATE="2026-01-01T12:00:00+00:00",
                      GIT_COMMITTER_DATE="2026-01-01T12:00:00+00:00")
    with tempfile.TemporaryDirectory(prefix="git-tui-demo-", dir="/tmp") as name:
        directory = Path(name)
        fixture(directory)
        terminal = Terminal(args.binary.resolve(), directory, "git")
        try:
            terminal.wait("src/main.rs")
            terminal.click("src/main.rs")
            terminal.wait('+    println!("Click a file')
            terminal.read(timeout=0.2)
            render(terminal.screen, args.output / "files.png", args.font)
            terminal.click("3 Log")
            terminal.wait("Commit details")
            terminal.wait("+pub fn status_label")
            terminal.read(timeout=0.2)
            render(terminal.screen, args.output / "history.png", args.font)
        finally:
            terminal.close()


if __name__ == "__main__":
    main()
