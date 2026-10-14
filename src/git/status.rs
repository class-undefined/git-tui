use std::{ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf};

use anyhow::{Result, ensure};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileStatus {
    pub index: u8,
    pub worktree: u8,
    pub path: PathBuf,
    pub original_path: Option<PathBuf>,
}

impl FileStatus {
    pub fn is_conflicted(&self) -> bool {
        matches!(
            (self.index, self.worktree),
            (b'D', b'D')
                | (b'A', b'U')
                | (b'U', b'D')
                | (b'U', b'A')
                | (b'D', b'U')
                | (b'A', b'A')
                | (b'U', b'U')
        )
    }

    pub fn is_staged(&self) -> bool {
        !matches!(self.index, b' ' | b'?') && !self.is_conflicted()
    }

    pub fn is_untracked(&self) -> bool {
        self.index == b'?' && self.worktree == b'?'
    }

    pub fn has_worktree_changes(&self) -> bool {
        self.worktree != b' '
    }
}

pub(super) fn parse(bytes: &[u8]) -> Result<Vec<FileStatus>> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    ensure!(bytes.last() == Some(&0), "incomplete Git status output");
    let mut records = bytes[..bytes.len() - 1].split(|b| *b == 0);
    let mut files = Vec::new();
    while let Some(record) = records.next() {
        ensure!(
            record.len() >= 4 && record[2] == b' ',
            "invalid Git status record"
        );
        let original_path = if matches!(record[0], b'R' | b'C') || matches!(record[1], b'R' | b'C')
        {
            let original = records
                .next()
                .filter(|p| !p.is_empty())
                .ok_or_else(|| anyhow::anyhow!("rename is missing its original path"))?;
            Some(PathBuf::from(OsString::from_vec(original.to_vec())))
        } else {
            None
        };
        files.push(FileStatus {
            index: record[0],
            worktree: record[1],
            path: PathBuf::from(OsString::from_vec(record[3..].to_vec())),
            original_path,
        });
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn preserves_rename_order_and_raw_paths() -> Result<()> {
        let files = parse(b"R  new\nname\0old name\0?? bad\xff\0")?;
        assert_eq!(files[0].path, PathBuf::from("new\nname"));
        assert_eq!(files[0].original_path, Some(PathBuf::from("old name")));
        assert_eq!(files[1].path.as_os_str().as_bytes(), b"bad\xff");
        Ok(())
    }

    #[test]
    fn rejects_incomplete_records() {
        assert!(parse(b"M  file").is_err());
        assert!(parse(b"R  new\0").is_err());
        assert!(parse(b"??\0").is_err());
    }

    #[test]
    fn conflicts_are_not_regular_staged_changes() -> Result<()> {
        for code in ["DD", "AU", "UD", "UA", "DU", "AA", "UU"] {
            let file = parse(format!("{code} file\0").as_bytes())?.remove(0);
            assert!(file.is_conflicted());
            assert!(!file.is_staged());
        }
        Ok(())
    }
}
