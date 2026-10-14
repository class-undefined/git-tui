use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use git_tui::git::{DiffMode, FileStatus, Mutation, Repository};
use tempfile::TempDir;

struct Fixture {
    dir: TempDir,
    binary: OsString,
}

impl Fixture {
    fn new() -> Result<Self> {
        let fixture = Self {
            dir: tempfile::tempdir()?,
            binary: env::var_os("GIT_TUI_TEST_GIT").unwrap_or_else(|| "git".into()),
        };
        fixture.git(&["init", "--template="])?;
        fixture.git(&["symbolic-ref", "HEAD", "refs/heads/main"])?;
        for (key, value) in [
            ("user.name", "Git TUI Test"),
            ("user.email", "test@example.invalid"),
            ("commit.gpgsign", "false"),
            ("core.autocrlf", "false"),
            ("core.hooksPath", ".git/hooks"),
        ] {
            fixture.git(&["config", key, value])?;
        }
        Ok(fixture)
    }

    fn git(&self, args: &[&str]) -> Result<String> {
        let output = Command::new(&self.binary)
            .current_dir(self.dir.path())
            .args(args)
            .output()?;
        ensure!(
            output.status.success(),
            "Git fixture {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    fn repo(&self) -> Result<Repository> {
        Repository::discover(self.dir.path(), &self.binary)
    }

    fn write(&self, path: impl AsRef<Path>, contents: &[u8]) -> Result<()> {
        fs::write(self.dir.path().join(path), contents)?;
        Ok(())
    }

    fn file(&self, name: impl AsRef<Path>) -> Result<FileStatus> {
        self.repo()?
            .snapshot()?
            .files
            .into_iter()
            .find(|f| f.path == name.as_ref())
            .context("expected file in status")
    }

    fn initial_commit(&self) -> Result<()> {
        self.write("tracked.txt", b"original\n")?;
        let repo = self.repo()?;
        repo.apply(&Mutation::Stage(self.file("tracked.txt")?))?;
        repo.apply(&Mutation::Commit("initial".to_owned()))?;
        Ok(())
    }
}

#[test]
fn unborn_stage_unstage_commit_and_log() -> Result<()> {
    let f = Fixture::new()?;
    let repo = f.repo()?;
    assert!(repo.snapshot()?.head.contains("no commits"));
    assert!(repo.snapshot()?.commits.is_empty());
    f.write("first file.txt", b"hello\n")?;
    let file = f.file("first file.txt")?;
    assert!(repo.diff(&file, DiffMode::Worktree)?.contains("hello"));
    repo.apply(&Mutation::Stage(file))?;
    let staged = f.file("first file.txt")?;
    assert!(staged.is_staged());
    assert!(repo.diff(&staged, DiffMode::Staged)?.contains("+hello"));
    f.write("first file.txt", b"newer worktree\n")?;
    repo.apply(&Mutation::Unstage(staged))?;
    assert!(f.file("first file.txt")?.is_untracked());
    assert_eq!(
        fs::read(f.dir.path().join("first file.txt"))?,
        b"newer worktree\n"
    );
    repo.apply(&Mutation::Stage(f.file("first file.txt")?))?;
    assert!(repo.apply(&Mutation::Commit(" ".to_owned())).is_err());
    repo.apply(&Mutation::Commit("first commit".to_owned()))?;
    let snapshot = repo.snapshot()?;
    assert!(snapshot.files.is_empty());
    assert_eq!(snapshot.commits[0].summary, "first commit");
    assert!(
        repo.show(&snapshot.commits[0].id)?
            .contains("newer worktree")
    );
    Ok(())
}

#[test]
fn literal_paths_never_stage_neighbors() -> Result<()> {
    let f = Fixture::new()?;
    let repo = f.repo()?;
    for path in [
        "*.txt",
        "neighbor.txt",
        "-flag",
        ":(glob)*",
        "line\nbreak",
        "中文.txt",
    ] {
        f.write(path, b"contents\n")?;
    }
    for path in [
        OsStr::new("*.txt"),
        OsStr::new("-flag"),
        OsStr::new(":(glob)*"),
        OsStr::new("line\nbreak"),
        OsStr::new("中文.txt"),
    ] {
        repo.apply(&Mutation::Stage(f.file(path)?))?;
        assert!(f.file(path)?.is_staged());
        assert!(f.file("neighbor.txt")?.is_untracked());
        repo.apply(&Mutation::Unstage(f.file(path)?))?;
        assert!(f.file(path)?.is_untracked());
    }
    Ok(())
}

// macOS filesystems reject invalid UTF-8 names; Linux exercises the full path.
#[cfg(target_os = "linux")]
#[test]
fn non_utf8_paths_roundtrip() -> Result<()> {
    use std::os::unix::ffi::OsStringExt;
    let f = Fixture::new()?;
    let raw = OsString::from_vec(b"non-utf8-\xff".to_vec());
    f.write(&raw, b"raw\n")?;
    let repo = f.repo()?;
    repo.apply(&Mutation::Stage(f.file(&raw)?))?;
    assert!(f.file(&raw)?.is_staged());
    repo.apply(&Mutation::Unstage(f.file(&raw)?))?;
    assert!(f.file(&raw)?.is_untracked());
    Ok(())
}

#[test]
fn modified_and_deleted_files_preserve_worktree_on_unstage() -> Result<()> {
    let f = Fixture::new()?;
    f.initial_commit()?;
    let repo = f.repo()?;
    f.write("tracked.txt", b"changed\n")?;
    assert!(
        repo.diff(&f.file("tracked.txt")?, DiffMode::Worktree)?
            .contains("+changed")
    );
    repo.apply(&Mutation::Stage(f.file("tracked.txt")?))?;
    f.write("tracked.txt", b"unstaged addition\n")?;
    let file = f.file("tracked.txt")?;
    assert!(file.is_staged() && file.has_worktree_changes());
    assert!(repo.diff(&file, DiffMode::Staged)?.contains("+changed"));
    repo.apply(&Mutation::Unstage(file))?;
    assert_eq!(
        fs::read(f.dir.path().join("tracked.txt"))?,
        b"unstaged addition\n"
    );
    fs::remove_file(f.dir.path().join("tracked.txt"))?;
    repo.apply(&Mutation::Stage(f.file("tracked.txt")?))?;
    assert_eq!(f.file("tracked.txt")?.index, b'D');
    repo.apply(&Mutation::Unstage(f.file("tracked.txt")?))?;
    assert!(!f.dir.path().join("tracked.txt").exists());
    assert_eq!(f.file("tracked.txt")?.worktree, b'D');
    Ok(())
}

#[test]
fn rename_unstages_both_paths() -> Result<()> {
    let f = Fixture::new()?;
    f.initial_commit()?;
    f.git(&["mv", "tracked.txt", "new name.txt"])?;
    let repo = f.repo()?;
    let renamed = f.file("new name.txt")?;
    assert_eq!(
        renamed.original_path.as_deref(),
        Some(Path::new("tracked.txt"))
    );
    assert!(
        repo.diff(&renamed, DiffMode::Staged)?
            .contains("new name.txt")
    );
    repo.apply(&Mutation::Unstage(renamed))?;
    assert!(f.file("new name.txt")?.is_untracked());
    assert_eq!(f.file("tracked.txt")?.worktree, b'D');
    assert!(!f.dir.path().join("tracked.txt").exists());
    Ok(())
}

#[test]
fn branches_switch_safely_and_log_works_detached() -> Result<()> {
    let f = Fixture::new()?;
    f.initial_commit()?;
    let repo = f.repo()?;
    repo.apply(&Mutation::CreateBranch("feature/test".to_owned()))?;
    f.write("tracked.txt", b"feature\n")?;
    repo.apply(&Mutation::Stage(f.file("tracked.txt")?))?;
    repo.apply(&Mutation::Commit("feature".to_owned()))?;
    f.write("tracked.txt", b"do not overwrite\n")?;
    assert!(repo.apply(&Mutation::Checkout("main".into())).is_err());
    assert_eq!(
        fs::read(f.dir.path().join("tracked.txt"))?,
        b"do not overwrite\n"
    );
    f.write("tracked.txt", b"feature\n")?;
    repo.apply(&Mutation::Checkout("main".into()))?;
    assert_eq!(repo.snapshot()?.head, "main");
    assert_eq!(repo.snapshot()?.branches.len(), 2);
    assert!(
        repo.apply(&Mutation::CreateBranch("--orphan".to_owned()))
            .is_err()
    );
    assert!(
        repo.apply(&Mutation::CreateBranch("bad name".to_owned()))
            .is_err()
    );
    assert!(repo.apply(&Mutation::Checkout("--detach".into())).is_err());
    let commit = repo.snapshot()?.commits[0].id.clone();
    f.git(&["checkout", "--detach", &commit])?;
    assert!(repo.snapshot()?.head.starts_with("detached"));
    Ok(())
}

#[test]
fn discovers_from_subdirectory_and_rejects_non_repositories() -> Result<()> {
    let f = Fixture::new()?;
    let nested = f.dir.path().join("nested");
    fs::create_dir(&nested)?;
    assert_eq!(
        Repository::discover(&nested, &f.binary)?.root(),
        fs::canonicalize(f.dir.path())?
    );
    let empty = tempfile::tempdir()?;
    assert!(Repository::discover(empty.path(), &f.binary).is_err());
    f.git(&["init", "--bare", "bare.git"])?;
    assert!(Repository::discover(&f.dir.path().join("bare.git"), &f.binary).is_err());
    Ok(())
}

#[test]
fn merge_conflicts_are_visible_and_can_be_staged_after_resolution() -> Result<()> {
    let f = Fixture::new()?;
    f.initial_commit()?;
    f.git(&["checkout", "-b", "other"])?;
    f.write("tracked.txt", b"other\n")?;
    f.git(&["commit", "-am", "other"])?;
    f.git(&["checkout", "main"])?;
    f.write("tracked.txt", b"main\n")?;
    f.git(&["commit", "-am", "main"])?;
    assert!(f.git(&["merge", "other"]).is_err());
    assert!(f.file("tracked.txt")?.is_conflicted());
    let repo = f.repo()?;
    assert!(
        repo.diff(&f.file("tracked.txt")?, DiffMode::Worktree)?
            .contains("tracked.txt")
    );
    f.write("tracked.txt", b"resolved\n")?;
    repo.apply(&Mutation::Stage(f.file("tracked.txt")?))?;
    assert!(!f.file("tracked.txt")?.is_conflicted());
    Ok(())
}

#[test]
fn previews_binary_and_symlinks_without_following_targets() -> Result<()> {
    let f = Fixture::new()?;
    f.write("binary", b"a\0b")?;
    symlink("/does/not/exist", f.dir.path().join("link"))?;
    let repo = f.repo()?;
    assert!(
        repo.diff(&f.file("binary")?, DiffMode::Worktree)?
            .contains("binary")
    );
    assert!(
        repo.diff(&f.file("link")?, DiffMode::Worktree)?
            .contains("/does/not/exist")
    );
    Ok(())
}

#[test]
fn hooks_report_failures_and_deadlines_kill_descendants() -> Result<()> {
    let f = Fixture::new()?;
    f.write("file", b"contents")?;
    let repo = f.repo()?;
    repo.apply(&Mutation::Stage(f.file("file")?))?;
    let hook_dir = f.dir.path().join(".git/hooks");
    fs::create_dir_all(&hook_dir)?;
    let hook = hook_dir.join("pre-commit");
    fs::write(
        &hook,
        "#!/bin/sh\necho 'hook rejected commit' >&2\nexit 1\n",
    )?;
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755))?;
    assert!(
        repo.apply(&Mutation::Commit("rejected".to_owned()))
            .err()
            .context("hook should reject commit")?
            .to_string()
            .contains("hook rejected")
    );
    fs::write(hook, "#!/bin/sh\nsleep 20\n")?;
    let started = Instant::now();
    let error = repo
        .with_timeout(Duration::from_millis(100))
        .apply(&Mutation::Commit("timeout".to_owned()))
        .err()
        .context("hook should time out")?;
    assert!(error.to_string().contains("timed out"));
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(fs::read(f.dir.path().join("file"))?, b"contents");
    Ok(())
}
