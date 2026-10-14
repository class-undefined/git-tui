# Contributing

Keep the scope local: status, diff, staging, commits, branches, and history. Hosting-provider integrations are outside the project scope.

Use the pinned Rust toolchain and commit `Cargo.lock`. Follow [AGENTS.md](AGENTS.md) and the [architecture](docs/architecture.md).

Before submitting a change:

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

Changes to Git commands must also pass the real Git 1.8.3.1 suite described in [compatibility.md](docs/compatibility.md). Input/terminal changes should run the PTY smoke test; add focused regressions for behavior that can fail independently of the implementation.

Make one logical change per commit. Each commit should build and keep its relevant checks green. Separate refactoring, behavior changes, and unrelated fixes; use descriptive messages such as `fix(git): preserve both rename paths when unstaging`.

Avoid shell interpolation, lossy path arguments, blocking I/O on the UI thread, unbounded queues, and production `unwrap` / `expect`. Keep keyboard and mouse controls equivalent. Never introduce destructive worktree operations as a side effect of navigation or refresh.
