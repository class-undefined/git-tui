# Working on git-tui

- Scope: local Git workflows only; do not add Gitea or hosting-provider integration.
- Preserve Git 1.8.3.1 compatibility. Use `Command::current_dir`, porcelain v1 with NUL delimiters, and `checkout` / `reset`; no `git -C`, `switch`, `restore`, or porcelain v2.
- Keep raw path bytes through parsing and subprocess arguments. Escape control characters only when displaying text.
- Never interpolate a shell command or treat a user path as a pattern. Include regression tests for unusual paths and unborn repositories.
- Keep subprocesses off the UI thread; serialize mutations and bound queues/output. Restore terminal modes on errors and panics.
- Keep keyboard and mouse actions equivalent. Derive hit regions from the rendered layout and test scrolling, modals, and resizing.
- Do not add destructive worktree operations without an explicit product requirement.
- Make atomic commits. Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test --locked` for code changes. Run the legacy Git suite when changing Git commands.
- Prefer small domain modules, explicit enums, `Result` with context, and minimal visibility. No unsafe code or production panics for recoverable failures.
