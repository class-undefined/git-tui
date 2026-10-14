# 运行截图

`files.png`、`history.png`、`files-zh.png` 和 `language.png` 来自真实 `git-tui` 二进制在 PTY 中的输出。脚本创建隔离的演示 Git 仓库，通过 SGR 鼠标事件点击文件、日志标签和语言设置，再用 pyte 解析终端输出、Pillow 渲染终端单元格；没有手工拼接界面文字。

画面尺寸为 120 列 × 34 行。使用固定深色终端配色和 JetBrains Mono 字体；中文截图另使用系统 CJK 字体。演示仓库路径由系统生成，Git 版本取决于本机；实际截图会随这些信息及程序变化。

在 macOS 或 Linux 上复现：

```sh
cargo build --locked --release
uv run --no-project --with pyte==0.8.2 --with pillow==11.3.0 python \
  scripts/capture-screenshots.py target/release/git-tui \
  --font /path/to/JetBrainsMono-Regular.ttf \
  --cjk-font /path/to/CJK-font.ttc
```

`--font` 也可以指定其他等宽 TrueType 字体；脚本优先使用同目录的 `Bold` 字体渲染粗体。默认输出到 `docs/images`，可以通过 `--output` 改变位置。截图结束时检查应用正常退出，并恢复终端模式。

`--cjk-font` 可使用 macOS 的 `/System/Library/Fonts/STHeiti Light.ttc` 或 Linux 的 Noto Sans CJK 字体。省略此参数只生成英文截图。截图脚本使用临时配置目录，不修改个人语言设置。
