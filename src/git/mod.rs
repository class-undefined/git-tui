mod process;
mod status;

use std::{
    ffi::{OsStr, OsString},
    fs,
    io::Read,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
pub use status::FileStatus;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DiffMode {
    #[default]
    Worktree,
    Staged,
}

#[derive(Clone, Debug)]
pub struct Branch {
    pub name: OsString,
    pub current: bool,
}

#[derive(Clone, Debug)]
pub struct Commit {
    pub id: String,
    pub summary: String,
    pub author: String,
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub head: String,
    pub files: Vec<FileStatus>,
    pub branches: Vec<Branch>,
    pub commits: Vec<Commit>,
}

#[derive(Clone, Debug)]
pub enum Mutation {
    Stage(FileStatus),
    Unstage(FileStatus),
    Commit(String),
    Checkout(OsString),
    CreateBranch(String),
}

#[derive(Clone, Debug)]
pub struct Repository {
    root: PathBuf,
    binary: OsString,
    version: String,
    timeout: Duration,
}

impl Repository {
    pub fn discover(directory: &Path, binary: &OsStr) -> Result<Self> {
        // Resolve a relative executable before switching the child's directory.
        let binary =
            if Path::new(binary).components().count() > 1 && Path::new(binary).is_relative() {
                fs::canonicalize(binary)
                    .context("could not resolve Git executable")?
                    .into_os_string()
            } else {
                binary.to_owned()
            };
        let mut repo = Self {
            root: fs::canonicalize(directory).context("could not open repository directory")?,
            binary,
            version: String::new(),
            timeout: Duration::from_secs(30),
        };
        let version = repo.bytes(&[OsStr::new("--version")])?;
        repo.version = String::from_utf8_lossy(&version).trim().to_owned();
        ensure!(
            supported_version(&repo.version),
            "Git >= 1.8.3.1 is required; found {}",
            repo.version
        );
        let inside = repo.bytes(&["rev-parse".as_ref(), "--is-inside-work-tree".as_ref()])?;
        ensure!(
            inside == b"true\n",
            "a non-bare Git working tree is required"
        );
        let mut root = repo.bytes(&["rev-parse".as_ref(), "--show-toplevel".as_ref()])?;
        if root.last() == Some(&b'\n') {
            root.pop();
        }
        repo.root = PathBuf::from(OsString::from_vec(root));
        Ok(repo)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Set a deadline per subprocess, including local commit hooks.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn output(&self, args: &[&OsStr]) -> Result<process::Output> {
        process::run(&self.binary, &self.root, args, self.timeout)
    }

    fn bytes(&self, args: &[&OsStr]) -> Result<Vec<u8>> {
        let output = self.output(args)?;
        ensure!(
            output.status.success(),
            "git {}: {}",
            args[0].to_string_lossy(),
            display_bytes(&output.stderr)
        );
        ensure!(
            !output.truncated,
            "Git output exceeded 2 MiB; narrow the repository or operation"
        );
        Ok(output.stdout)
    }

    fn has_head(&self) -> Result<bool> {
        let output = self.output(&[
            "rev-parse".as_ref(),
            "--verify".as_ref(),
            "--quiet".as_ref(),
            "HEAD".as_ref(),
        ])?;
        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => bail!("could not read HEAD: {}", display_bytes(&output.stderr)),
        }
    }

    pub fn snapshot(&self) -> Result<Snapshot> {
        let files = status::parse(&self.bytes(&[
            "status".as_ref(),
            "--porcelain".as_ref(),
            "-z".as_ref(),
            "--untracked-files=all".as_ref(),
        ])?)?;
        let symbolic =
            self.output(&["symbolic-ref".as_ref(), "--quiet".as_ref(), "HEAD".as_ref()])?;
        let head_ref = if symbolic.status.success() {
            Some(
                symbolic
                    .stdout
                    .strip_suffix(b"\n")
                    .unwrap_or(&symbolic.stdout)
                    .to_vec(),
            )
        } else {
            ensure!(
                symbolic.status.code() == Some(1),
                "could not read current branch: {}",
                display_bytes(&symbolic.stderr)
            );
            None
        };
        let has_head = self.has_head()?;
        let head = match head_ref.as_deref() {
            Some(name) => display_bytes(name.strip_prefix(b"refs/heads/").unwrap_or(name)),
            None => format!(
                "detached {}",
                display_bytes(&self.bytes(&[
                    "rev-parse".as_ref(),
                    "--short".as_ref(),
                    "HEAD".as_ref()
                ])?)
            ),
        };
        let branches = self
            .bytes(&[
                "for-each-ref".as_ref(),
                "--format=%(refname)".as_ref(),
                "refs/heads/".as_ref(),
            ])?
            .split(|b| *b == b'\n')
            .filter(|line| !line.is_empty())
            .map(|name| Branch {
                name: OsString::from_vec(
                    name.strip_prefix(b"refs/heads/").unwrap_or(name).to_vec(),
                ),
                current: head_ref.as_deref() == Some(name),
            })
            .collect();
        let commits = if has_head {
            let log = self.bytes(&[
                "log".as_ref(),
                "-100".as_ref(),
                "--no-decorate".as_ref(),
                "--format=%H%x00%s%x00%an%x00".as_ref(),
            ])?;
            let mut parts = log.split(|b| *b == 0);
            let mut commits = Vec::new();
            while let Some(id) = parts.next() {
                let id = id.strip_prefix(b"\n").unwrap_or(id);
                if id.is_empty() {
                    continue;
                }
                let summary = parts.next().context("invalid Git log summary")?;
                let author = parts.next().context("invalid Git log author")?;
                commits.push(Commit {
                    id: String::from_utf8(id.to_vec())?,
                    summary: display_bytes(summary),
                    author: display_bytes(author),
                });
            }
            commits
        } else {
            Vec::new()
        };
        Ok(Snapshot {
            head: if has_head {
                head
            } else {
                format!("{head} (no commits)")
            },
            files,
            branches,
            commits,
        })
    }

    pub fn diff(&self, file: &FileStatus, mode: DiffMode) -> Result<String> {
        if file.is_untracked() && mode == DiffMode::Worktree {
            let path = self.root.join(&file.path);
            let metadata =
                fs::symlink_metadata(&path).context("could not inspect untracked path")?;
            if metadata.file_type().is_symlink() {
                return Ok(format!(
                    "Untracked symlink -> {}",
                    display_path(&fs::read_link(path)?)
                ));
            }
            ensure!(
                metadata.is_file(),
                "preview is only available for regular files"
            );
            let mut bytes = Vec::new();
            fs::File::open(path)?
                .take(256 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            let truncated = bytes.len() > 256 * 1024;
            bytes.truncate(256 * 1024);
            if bytes.contains(&0) {
                return Ok("Untracked binary file (preview omitted)".to_owned());
            }
            return Ok(format!(
                "Untracked file\n{}{}",
                display_multiline(&bytes),
                if truncated {
                    "\n[preview truncated]"
                } else {
                    ""
                }
            ));
        }
        let path = Path::new(".").join(&file.path);
        let original_path = file.original_path.as_ref().map(|p| Path::new(".").join(p));
        let mut args = vec![
            OsStr::new("diff"),
            OsStr::new("--no-ext-diff"),
            OsStr::new("--no-textconv"),
            OsStr::new("--no-color"),
        ];
        if mode == DiffMode::Staged {
            args.push(OsStr::new("--cached"));
        }
        args.extend([OsStr::new("--"), path.as_os_str()]);
        if let Some(original) = &original_path {
            args.push(original.as_os_str());
        }
        self.preview(&args)
    }

    pub fn show(&self, id: &str) -> Result<String> {
        ensure!(
            (id.len() == 40 || id.len() == 64) && id.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid commit ID"
        );
        self.preview(&[
            "show".as_ref(),
            "--no-ext-diff".as_ref(),
            "--no-textconv".as_ref(),
            "--no-color".as_ref(),
            "--format=fuller".as_ref(),
            "--stat".as_ref(),
            "--patch".as_ref(),
            id.as_ref(),
            "--".as_ref(),
        ])
    }

    fn preview(&self, args: &[&OsStr]) -> Result<String> {
        let output = self.output(args)?;
        ensure!(
            output.status.success(),
            "Git preview failed: {}",
            display_bytes(&output.stderr)
        );
        let mut text = display_multiline(&output.stdout);
        if output.truncated {
            text.push_str("\n[preview truncated at 2 MiB]");
        }
        if text.is_empty() {
            text.push_str("No changes in this view. Press d to switch worktree / staged diff.");
        }
        Ok(text)
    }

    pub fn apply(&self, mutation: &Mutation) -> Result<String> {
        match mutation {
            Mutation::Stage(file) => {
                // Git 1.8.3.1 still parses a leading colon with --literal-pathspecs.
                let path = Path::new(".").join(&file.path);
                let original_path = file.original_path.as_ref().map(|p| Path::new(".").join(p));
                let mut args = vec![
                    OsStr::new("add"),
                    OsStr::new("-A"),
                    OsStr::new("--"),
                    path.as_os_str(),
                ];
                // A staged rename's old path no longer exists in the index.
                if file.index != b'R'
                    && let Some(original) = &original_path
                {
                    args.push(original.as_os_str());
                }
                self.bytes(&args)?;
                Ok(format!("Staged {}", display_path(&file.path)))
            }
            Mutation::Unstage(file) => {
                let path = Path::new(".").join(&file.path);
                let original_path = file.original_path.as_ref().map(|p| Path::new(".").join(p));
                let mut args: Vec<&OsStr> = if self.has_head()? {
                    vec![
                        "reset".as_ref(),
                        "-q".as_ref(),
                        "HEAD".as_ref(),
                        "--".as_ref(),
                    ]
                } else {
                    vec![
                        "rm".as_ref(),
                        "--cached".as_ref(),
                        "-q".as_ref(),
                        "-f".as_ref(),
                        "--".as_ref(),
                    ]
                };
                args.push(path.as_os_str());
                if let Some(original) = &original_path {
                    args.push(original.as_os_str());
                }
                self.bytes(&args)?;
                Ok(format!(
                    "Unstaged {} (worktree kept)",
                    display_path(&file.path)
                ))
            }
            Mutation::Commit(message) => {
                ensure!(
                    !message.trim().is_empty(),
                    "commit message must not be empty"
                );
                self.bytes(&["commit".as_ref(), "-m".as_ref(), message.as_ref()])?;
                Ok("Commit created".to_owned())
            }
            Mutation::Checkout(name) => {
                self.bytes(&["check-ref-format".as_ref(), "--branch".as_ref(), name])?;
                let mut full = OsString::from("refs/heads/");
                full.push(name);
                self.bytes(&["show-ref".as_ref(), "--verify".as_ref(), full.as_os_str()])?;
                self.bytes(&["checkout".as_ref(), name, "--".as_ref()])?;
                Ok(format!("Switched to {}", display_bytes(name.as_bytes())))
            }
            Mutation::CreateBranch(name) => {
                ensure!(
                    !name.starts_with('-') && !name.is_empty(),
                    "invalid branch name"
                );
                self.bytes(&[
                    "check-ref-format".as_ref(),
                    "--branch".as_ref(),
                    name.as_ref(),
                ])?;
                self.bytes(&["checkout".as_ref(), "-b".as_ref(), name.as_ref()])?;
                Ok(format!("Created branch {name}"))
            }
        }
    }
}

fn supported_version(version: &str) -> bool {
    let Some(number) = version
        .strip_prefix("git version ")
        .and_then(|v| v.split_whitespace().next())
    else {
        return false;
    };
    let mut parts = [0_u32; 4];
    let mut count = 0;
    for (index, part) in number.split('.').take(4).enumerate() {
        let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
        let Ok(value) = digits.parse() else {
            break;
        };
        parts[index] = value;
        count += 1;
    }
    count >= 3 && parts >= [1, 8, 3, 1]
}

/// Escape controls so repository content cannot inject terminal commands or rows.
pub fn display_bytes(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .flat_map(|c| {
            if c.is_control() {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

pub fn display_path(path: &Path) -> String {
    display_bytes(path.as_os_str().as_bytes())
}

fn display_multiline(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .split('\n')
        .map(|line| display_bytes(line.as_bytes()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enforces_centos_baseline() {
        for version in [
            "git version 1.8.3.1",
            "git version 2.46.0",
            "git version 2.39.5 (Apple Git-154)",
            "git version 2.40.0.windows.1",
        ] {
            assert!(supported_version(version), "{version}");
        }
        for version in [
            "git version 1.8.3",
            "git version 1.7.1",
            "git version 1.8.2.9",
            "unparseable",
        ] {
            assert!(!supported_version(version), "{version}");
        }
    }

    #[test]
    fn display_escapes_terminal_control_sequences() {
        assert_eq!(display_bytes(b"a\n\x1b[31m\tb"), "a\\n\\u{1b}[31m\\tb");
    }
}
