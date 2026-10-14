# Git 与 CentOS 7 兼容性

最低 Git 版本为 **1.8.3.1**。CentOS 7 官方 base/updates 仓库提供该版本系列，包括最终更新 `git-1.8.3.1-25.el7_9`。来源：[CentOS 官方归档](https://vault.centos.org/centos/7/updates/x86_64/Packages/)、[CentOS 源码归档标签](https://gitlab.com/CentOS/archives/git.centos.org/rpms/git/-/tags/imports%2Fc7%2Fgit-1.8.3.1-25.el7_9)。

## 系统与架构

| 目标 | 构建策略 | 实际 CI 运行环境 |
| --- | --- | --- |
| macOS ARM64 | `aarch64-apple-darwin`，`MACOSX_DEPLOYMENT_TARGET=11.0` | macOS 15 Apple Silicon |
| Linux x86_64 | `x86_64-unknown-linux-musl`，静态链接 | Ubuntu 24.04、CentOS 7.9.2009 容器 |
| Linux ARM64 | `aarch64-unknown-linux-musl`，静态链接 | Ubuntu 24.04 ARM64 |

macOS 最低部署目标为 11.0，但 CI 只实际运行在 macOS 15；未声称每个 macOS 次版本都经过测试。macOS 产物没有 Developer ID 签名或 Apple 公证。Linux 两种架构均原生构建与测试，不依赖模拟器；CI 检查 ELF 没有动态解释器或共享库依赖。预编译产物无需 Rust，仍需要系统 Git 和支持 TUI 的终端。

GitHub runner 架构依据：[GitHub 官方 runner 列表](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)。打包、校验和自动发布见 [发布说明](releasing.md)。Windows 和 macOS Intel 暂无官方安装包或 CI 保证。

## 命令约束

| 功能 | 实际接口 |
| --- | --- |
| 工作目录 | Rust `Command::current_dir`，不用 `git -C` |
| 状态 | `status --porcelain -z --untracked-files=all`，不用 porcelain v2 |
| 当前分支 | `symbolic-ref --quiet HEAD`，detached 时 `rev-parse --short HEAD` |
| 本地分支 | `for-each-ref --format=%(refname) refs/heads/` |
| 差异 | `diff [--cached] --no-ext-diff --no-textconv --no-color -- <path>` |
| 暂存 | `add -A -- <path>` |
| 取消暂存 | 有 HEAD：`reset -q HEAD -- <path>`；首次提交前：`rm --cached -q -f -- <path>` |
| 提交 | `commit -m <summary>`，保留正常 hooks 行为 |
| 分支 | `check-ref-format --branch`、`checkout` / `checkout -b`，不用 `switch` |
| 日志 | `log -100 --no-decorate --format=...`，NUL 分隔字段 |
| 提交详情 | `show --format=fuller --stat --patch ... <object-id> --` |

所有路径使用 `--literal-pathspecs`，并增加 `./` 前缀。后者是 Git 1.8.3.1 的必要兼容处理：单独使用全局 literal 选项仍会解释某些以冒号开头的 pathspec magic。重命名解析遵循 `-z` 格式的“目标路径 NUL 原路径 NUL”，取消暂存包含两个路径。

解析依据：[Git v1.8.3.1 status 文档](https://github.com/git/git/blob/v1.8.3.1/Documentation/git-status.txt)。

## 两层验证

1. `scripts/build-legacy-git.sh` 下载 kernel.org 官方 Git 1.8.3.1 源码，核验固定 SHA-256，在隔离目录编译用于本地工作流的 Git 可执行文件，不安装或覆盖系统 Git。测试同时覆盖最新系统 Git 与这个真实旧版 Git。
2. CI 在现代 Ubuntu 构建 `x86_64-unknown-linux-musl` 的应用与集成测试程序，进入 CentOS 7.9.2009 容器，使用官方 vault yum 仓库安装 Git，并断言 `git --version` 为 1.8.3.1。随后用系统 Git 运行完整后端集成测试与 CLI 诊断。

Git 1.8.3.1 源码构建禁用网络相关组件，仅用于本项目的本地 Git 测试，不作为用户系统 Git 的替代品。源码校验和来源：[kernel.org](https://www.kernel.org/pub/software/scm/git/sha256sums.asc)。

## CentOS 7 二进制

现代构建主机（Ubuntu 示例）：

```sh
sudo apt-get install musl-tools
rustup target add x86_64-unknown-linux-musl
cargo build --locked --release --target x86_64-unknown-linux-musl
file target/x86_64-unknown-linux-musl/release/git-tui
```

把生成的 `git-tui` 复制到目标机即可，运行时继续调用目标机已有 Git。普通 `x86_64-unknown-linux-gnu` 构建可能依赖构建机较新的 glibc，不应当作 CentOS 7 可移植产物分发。

验证边界：容器检查 CentOS 7 用户空间、glibc 环境和 yum Git，使用宿主机内核；它不等同于在每一种真实 CentOS 7 / 3.10 内核主机上验证。CentOS 7 的系统包验证覆盖 x86_64；ARM64 静态产物在 Ubuntu ARM64 上验证，不代表验证过 CentOS 7 ARM64。

## 回归场景

空仓库与首次提交、部分暂存文件、重命名取消暂存、删除、冲突、中文与带换行的文件名、通配符与冒号路径、Linux 非 UTF-8 文件名、子目录发现、detached HEAD、脏工作区拒绝覆盖、hook 失败和超时、未跟踪二进制与符号链接预览。
