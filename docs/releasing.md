# 自动发布

推送 `v*` 标签会触发 [Release workflow](../.github/workflows/release.yml)。标签必须与 `Cargo.toml` 的版本完全一致，例如 `version = "0.1.0"` 对应 `v0.1.0`。`v0.2.0-rc.1` 这类预发布版本会标记为 prerelease，不更新 Latest。

## 发布检查

Release 与日常 CI 共用 [verify.yml](../.github/workflows/verify.yml)：

| 安装包目标 | 原生构建与测试环境 |
| --- | --- |
| `aarch64-apple-darwin` | macOS 15 ARM64，部署目标 macOS 11.0 |
| `x86_64-unknown-linux-musl` | Ubuntu 24.04 x86_64，静态链接；另在 CentOS 7 验证 |
| `aarch64-unknown-linux-musl` | Ubuntu 24.04 ARM64，静态链接 |

每个平台执行格式检查、Clippy、单元/集成测试，并在最终 release 二进制上执行真实 PTY 鼠标测试。Linux x86_64 还覆盖真实 Git 1.8.3.1 和 CentOS 7 官方 yum Git。构建检查会拒绝动态链接的 Linux 二进制。

三个平台全部成功后，工作流检查安装包是否齐全、校验每个 SHA-256、验证压缩包文件列表及可执行权限，再生成统一 `SHA256SUMS`。发布 job 再次核验下载后的文件，使用 `gh release create --verify-tag --generate-notes` 发布三个 tar.gz 和校验文件。

第三方 Actions 固定到完整 commit SHA；只有最后的发布 job 拥有 `contents: write`。同一标签的工作流串行执行，已有 Release 不会被覆盖。安装包包含 `git-tui`、`README.md` 和 `LICENSE`；文件名包括版本与完整目标架构。

## 先演练

手动触发只执行完整验证和打包，产生 `release-bundle` artifact，不创建标签或 Release：

```sh
gh workflow run release.yml --repo class-undefined/git-tui --ref main
gh run list --repo class-undefined/git-tui --workflow release.yml
gh run watch <run-id> --repo class-undefined/git-tui --exit-status
gh run download <run-id> --repo class-undefined/git-tui \
  --name release-bundle --dir release-bundle
cd release-bundle
shasum -a 256 -c SHA256SUMS
```

CI 每次也会上传各平台的 `package-<target>` artifact，保留 14 天。Release 附件用于长期分发。

## 发布版本

1. 修改 `Cargo.toml` 的版本，运行 `cargo check` 更新 `Cargo.lock`，按原子提交约定提交并推送。先确认该提交的 CI 通过。
2. 在对应提交创建并推送版本标签。例如首版：

   ```sh
   git tag -a v0.1.0 -m 'Release v0.1.0'
   git push origin v0.1.0
   ```

3. 查看 Release workflow 和生成的 [GitHub Release](https://github.com/class-undefined/git-tui/releases)。构建或校验失败时不会发布。

如果尚未创建 Release，修复外部故障后可重跑同一次工作流；代码修复应使用新提交和新版本。若上传中断留下 draft，应先检查已有附件和失败日志，人工处理该 draft，再重跑。不要移动已发布的标签或覆盖已发布附件。

## 本地检查发布工具

需要 Python 3.11+；脚本仅使用标准库：

```sh
python3 -m unittest discover -s scripts -p 'test_release.py'
python3 scripts/release.py check-tag v0.1.0
python3 scripts/release.py package --target aarch64-apple-darwin \
  --binary target/aarch64-apple-darwin/release/git-tui --output dist
```

`package` 校验二进制报告的版本；`collect --output dist` 必须收到三个目标安装包及各自 `.sha256` 才会生成 `SHA256SUMS`。单元测试覆盖错误标签、二进制版本不匹配、缺少平台和附件损坏。
