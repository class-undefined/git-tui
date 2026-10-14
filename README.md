# git-tui

使用 Rust 编写、支持鼠标操作的本地 Git 终端客户端。最低 Git 版本为 **1.8.3.1**，覆盖 CentOS 7 官方 yum 仓库的版本。

首版范围：工作区状态、差异预览、文件级暂存与取消暂存、提交、分支及日志。项目专注本地 Git 工作流，不规划 Gitea 集成。

开发工具链固定为 Rust 1.94.1；Git 运行时版本与 Rust 构建工具链是两个独立要求。Linux 和 macOS 为支持平台。

```sh
cargo run -- --help
```

## 开发约定

- 按领域划分 Git 后端、应用状态、输入处理与界面渲染。
- Git 子进程在后台串行执行；界面只处理状态和事件。
- 错误通过 `Result` 传播，生产代码禁止 `unsafe`，避免 `unwrap` / `expect`。
- 文件路径按原始字节保留；命令使用独立参数，不拼接 shell。
- 所有核心工作流必须用真实 Git 1.8.3.1 验证。
- 每个提交只完成一个可审查的逻辑改动，保持可构建和可验证。

## 参考

交互参考 [gitu](https://github.com/altsem/gitu) 的本地 Git 工作流；需求调研参考 [gitea-idea-plugin](https://github.com/LeonDevLifeLog/gitea-idea-plugin)。本项目独立实现，不复制参考项目代码，也不包含 Gitea 功能。

Git 基线来源：[CentOS 7 官方归档](https://vault.centos.org/centos/7/updates/x86_64/Packages/)。

## License

MIT
