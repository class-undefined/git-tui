#!/usr/bin/env python3
"""Exercise the real terminal, mouse protocol, Git mutations, and mode restoration.

Run: uv run --with pyte==0.8.2 python scripts/pty-smoke.py target/debug/git-tui
"""

import fcntl
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time

import pyte


class Terminal:
    def __init__(self, binary, directory, git, mouse=True):
        self.master, self.slave = pty.openpty()
        os.set_blocking(self.master, False)
        self.before = termios.tcgetattr(self.slave)
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 34, 120, 0, 0))
        self.screen = pyte.Screen(120, 34)
        self.stream = pyte.ByteStream(self.screen)
        self.raw = bytearray()
        args = [str(binary), str(directory), "--git", git]
        if not mouse:
            args.append("--no-mouse")
        self.process = subprocess.Popen(
            args, stdin=self.slave, stdout=self.slave, stderr=self.slave,
            env={**os.environ, "TERM": "xterm-256color"}, start_new_session=True,
        )

    @property
    def rows(self):
        # pyte 0.8's display property can fail on a wide-character continuation
        # cell after incremental redraws; raw cells retain the correct text.
        return ["".join(self.screen.buffer[y][x].data for x in range(self.screen.columns))
                for y in range(self.screen.lines)]

    @property
    def text(self):
        return "\n".join(self.rows)

    def read(self, timeout=0.05):
        if select.select([self.master], [], [], timeout)[0]:
            try:
                chunk = os.read(self.master, 65536)
            except (BlockingIOError, OSError):
                return
            self.raw.extend(chunk)
            self.stream.feed(chunk)

    def wait(self, fragment, timeout=10):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            self.read()
            if fragment in self.text:
                return
        raise AssertionError(f"Screen did not contain {fragment!r}:\n{self.text}")

    def send(self, data):
        os.write(self.master, data.encode() if isinstance(data, str) else data)

    def click(self, label):
        for row, line in enumerate(self.rows):
            column = line.find(label)
            if column >= 0:
                self.send(f"\x1b[<0;{column + 2};{row + 1}M\x1b[<0;{column + 2};{row + 1}m")
                return
        raise AssertionError(f"No button {label!r}:\n{self.text}")

    def close(self, external_signal=False):
        if self.process.poll() is None:
            if external_signal:
                self.process.send_signal(signal.SIGTERM)
            else:
                self.send("\x03")
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                self.read()
                if self.process.poll() is not None:
                    break
            if self.process.poll() is None:
                self.process.kill()
                self.process.wait(timeout=5)
                raise AssertionError("TUI failed to exit")
        assert self.process.returncode == 0, self.text
        self.read()
        after = termios.tcgetattr(self.slave)
        assert after == self.before, "Terminal attributes were not restored"
        assert b"\x1b[?1000l" in self.raw, "Mouse capture was not disabled"
        assert b"\x1b[?1049l" in self.raw, "Alternate screen was not left"
        os.close(self.master)
        os.close(self.slave)


def main():
    binary = Path(sys.argv[1]).resolve()
    git = os.environ.get("GIT_TUI_TEST_GIT", "git")
    with tempfile.TemporaryDirectory(prefix="git-tui-pty-") as name:
        directory = Path(name)

        def command(*args):
            return subprocess.check_output([git, *args], cwd=directory, stderr=subprocess.STDOUT).decode().strip()

        command("init", "--template=")
        command("symbolic-ref", "HEAD", "refs/heads/main")
        command("config", "user.name", "PTY Test")
        command("config", "user.email", "pty@example.invalid")
        command("config", "commit.gpgsign", "false")
        (directory / "hello.txt").write_text("hello from the terminal\n")
        terminal = Terminal(binary, directory, git)
        try:
            terminal.wait("Untracked file")
            terminal.click("[s Stage]")
            terminal.wait("Staged hello.txt")
            terminal.click("[d Diff]")
            terminal.wait("+hello from the terminal")
            terminal.click("[c Commit]")
            terminal.wait("Commit summary")
            terminal.send("\x1b[200~鼠标提交 smoke\x1b[201~")
            terminal.wait("鼠标提交 smoke")
            terminal.click("[Confirm]")
            terminal.wait("Commit created")
            assert command("log", "-1", "--format=%s") == "鼠标提交 smoke"
            assert command("status", "--porcelain") == ""
            terminal.click("2 Branches")
            terminal.wait("Local branches")
            terminal.click("[n New]")
            terminal.wait("New branch name")
            terminal.send("feature/pty")
            terminal.click("[Confirm]")
            terminal.wait("Created branch feature/pty")
            assert command("symbolic-ref", "HEAD") == "refs/heads/feature/pty"
            terminal.click("  main")
            terminal.click("[Enter Switch]")
            terminal.wait("Switch to main?")
            terminal.click("[Confirm]")
            terminal.wait("Switched to main")
            assert command("symbolic-ref", "HEAD") == "refs/heads/main"
            terminal.click("3 Log")
            terminal.wait("Commit details")
            terminal.wait("+hello from the terminal")
            print(terminal.text)
        finally:
            terminal.close(external_signal=True)
        terminal = Terminal(binary, directory, git, mouse=False)
        try:
            terminal.wait("Working tree clean")
            assert b"\x1b[?1000h" not in terminal.raw
        finally:
            terminal.close()
    print("PTY smoke passed: mouse stage/diff/commit/branch/log, paste, no-mouse, terminal restoration")


if __name__ == "__main__":
    main()
