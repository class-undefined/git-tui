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
from wcwidth import wcwidth, wcswidth


class TerminalScreen(pyte.Screen):
    def draw(self, data):
        # pyte 0.8.2 leaves orphan continuation cells when a wide glyph is
        # overwritten. Real terminals erase both halves of that glyph.
        for character in data:
            line = self.buffer[self.cursor.y]
            for column in range(self.cursor.x, min(self.columns, self.cursor.x + max(0, wcwidth(character)))):
                cell = line[column]
                if cell.data == "" and column > 0 and wcswidth(line[column - 1].data) == 2:
                    line[column - 1] = line[column - 1]._replace(data=" ")
                if wcswidth(cell.data) == 2 and column + 1 < self.columns:
                    line[column + 1] = line[column + 1]._replace(data=" ")
            super().draw(character)


class Terminal:
    def __init__(self, binary, directory, git, mouse=True, lang="en", config=None, env=None):
        self.temporary_config = tempfile.TemporaryDirectory(prefix="git-tui-config-") if config is None else None
        config = config if config is not None else self.temporary_config.name
        self.master, self.slave = pty.openpty()
        os.set_blocking(self.master, False)
        self.before = termios.tcgetattr(self.slave)
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 34, 120, 0, 0))
        self.screen = TerminalScreen(120, 34)
        self.stream = pyte.ByteStream(self.screen)
        self.raw = bytearray()
        args = [str(binary), str(directory), "--git", git]
        if not mouse:
            args.append("--no-mouse")
        if lang is not None:
            args.extend(["--lang", lang])
        self.process = subprocess.Popen(
            args, stdin=self.slave, stdout=self.slave, stderr=self.slave,
            env={**os.environ, "TERM": "xterm-256color", "LC_ALL": "C", "XDG_CONFIG_HOME": str(config), **(env or {})},
            start_new_session=True,
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
        # A title can arrive before the rest of a frame in another PTY chunk.
        self.wait(label)
        for row, line in enumerate(self.rows):
            offset = line.find(label)
            if offset >= 0:
                # A Chinese glyph occupies two terminal columns but one Python
                # character. Map the string offset back to the actual cells.
                consumed = 0
                for column in range(self.screen.columns):
                    consumed += len(self.screen.buffer[row][column].data)
                    if consumed > offset:
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
        if self.temporary_config is not None:
            self.temporary_config.cleanup()


def language_smoke(binary, directory, git, command):
    config = directory / ".git" / "tui-test-config"
    saved = config / "git-tui" / "language"
    (directory / "中文.txt").write_text("Untracked file is repository text, not a translation key.\n", encoding="utf-8")
    terminal = Terminal(binary, directory, git, config=config)
    try:
        terminal.wait("Untracked file")
        terminal.click("[F2 Language]")
        terminal.wait("Selected language:")
        terminal.click("[2 中文]")
        terminal.click("[Confirm]")
        terminal.wait("语言设置已保存： 中文")
        assert saved.read_text().strip() == "zh"
        terminal.wait("未跟踪文件")
        assert "Untracked file is repository text" in terminal.text
        terminal.click("[s 暂存]")
        terminal.wait("已暂存 中文.txt")
        terminal.click("[d 差异]")
        terminal.wait("+Untracked file is repository text")
        terminal.click("[c 提交]")
        terminal.wait("提交摘要")
        terminal.send("\x1b[200~中文界面提交\x1b[201~")
        terminal.click("[确认]")
        terminal.wait("提交已创建")
        assert command("log", "-1", "--format=%s") == "中文界面提交"
        terminal.click("2 分支")
        terminal.wait("本地分支")
        terminal.click("[n 新建]")
        terminal.wait("新分支名称")
        terminal.send("feature/zh")
        terminal.click("[确认]")
        terminal.wait("已创建分支 feature/zh")
        terminal.click("  main")
        terminal.click("[Enter 切换]")
        terminal.wait("切换到 main？")
        terminal.click("[确认]")
        terminal.wait("已切换到 main")
        terminal.click("3 日志")
        terminal.wait("提交详情")
        terminal.send("\x1b[H")  # Home: select the newly created commit.
        terminal.wait("+Untracked file is repository text")
        terminal.click("[? 帮助]")
        terminal.wait("键盘与鼠标")
        terminal.click("[关闭]")
    finally:
        terminal.close()

    # Saved preferences override the C locale on restart; a CLI override is
    # temporary and must not overwrite the saved file.
    terminal = Terminal(binary, directory, git, lang="en", config=config)
    try:
        terminal.wait("Working tree clean")
        assert saved.read_text().strip() == "zh"
    finally:
        terminal.close()
    terminal = Terminal(binary, directory, git, lang=None, config=config)
    try:
        terminal.wait("工作区干净")
        terminal.send("\x1bOQ")  # F2
        terminal.wait("已选语言")
        terminal.send("1\r")
        terminal.wait("Language saved: English")
        assert saved.read_text().strip() == "en"
    finally:
        terminal.close()
    terminal = Terminal(binary, directory, git, lang=None, config=config, env={"LC_ALL": "zh_CN.UTF-8"})
    try:
        terminal.wait("Working tree clean")
    finally:
        terminal.close()
    saved.unlink()
    terminal = Terminal(binary, directory, git, lang=None, config=config, env={"LC_ALL": "zh_CN.UTF-8"})
    try:
        terminal.wait("工作区干净")
    finally:
        terminal.close()
    # Invalid config should show a diagnostic and still allow the app to run.
    saved.write_text("invalid\n")
    terminal = Terminal(binary, directory, git, lang="zh", config=config)
    try:
        terminal.wait("无法读取语言设置")
        terminal.wait("工作区干净")
    finally:
        terminal.close()
    # A non-directory config path fails saving without claiming persistence.
    blocked = config / "blocked"
    blocked.write_text("keep me")
    terminal = Terminal(binary, directory, git, lang="en", config=blocked)
    try:
        terminal.wait("Working tree clean")
        terminal.send("\x1bOQ")
        terminal.wait("Selected language")
        terminal.send("2\r")
        terminal.wait("无法保存设置")
        assert blocked.read_text() == "keep me"
    finally:
        terminal.close()
    print("Language smoke passed: Chinese mouse workflows, keyboard switching, restart persistence, CLI/locale precedence, config errors")


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
        language_smoke(binary, directory, git, command)
    print("PTY smoke passed: mouse stage/diff/commit/branch/log, paste, no-mouse, terminal restoration")


if __name__ == "__main__":
    main()
