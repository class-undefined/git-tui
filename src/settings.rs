use std::{
    ffi::OsString,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use tempfile::NamedTempFile;

use crate::i18n::Language;

pub(crate) struct Settings {
    path: Option<PathBuf>,
}

impl Settings {
    pub(crate) fn from_environment() -> Self {
        Self::at(config_path(
            std::env::var_os("XDG_CONFIG_HOME"),
            std::env::var_os("HOME"),
        ))
    }

    pub(crate) fn at(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub(crate) fn load(&self) -> Result<Option<Language>> {
        let Some(path) = &self.path else {
            return Ok(None);
        };
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).context("cannot read language setting"),
        };
        let mut bytes = Vec::new();
        file.take(65).read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 64, "language setting exceeds 64 bytes");
        let value = std::str::from_utf8(&bytes).context("language setting is not UTF-8")?;
        Language::parse(value.trim())
            .map(Some)
            .context("language setting must be en or zh")
    }

    /// Write beside the destination, then atomically rename. A failed write
    /// never truncates the last saved preference. Called on the worker thread.
    pub(crate) fn save(&self, language: Language) -> Result<()> {
        let path = self
            .path
            .as_ref()
            .context("no absolute XDG_CONFIG_HOME or HOME is available")?;
        let parent = path
            .parent()
            .context("language setting has no parent directory")?;
        fs::create_dir_all(parent).context("cannot create settings directory")?;
        let mut file = NamedTempFile::new_in(parent).context("cannot create temporary setting")?;
        writeln!(file, "{}", language.code()).context("cannot write language setting")?;
        file.as_file()
            .sync_all()
            .context("cannot flush language setting")?;
        file.persist(path)
            .context("cannot replace language setting")?;
        Ok(())
    }
}

fn config_path(xdg: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    xdg.filter(|value| Path::new(value).is_absolute())
        .map(PathBuf::from)
        .or_else(|| {
            home.filter(|value| Path::new(value).is_absolute())
                .map(|value| PathBuf::from(value).join(".config"))
        })
        .map(|base| base.join("git-tui/language"))
}

pub(crate) fn system_language() -> Language {
    locale_language(["LC_ALL", "LC_MESSAGES", "LANG"].map(std::env::var_os))
}

fn locale_language(locales: [Option<OsString>; 3]) -> Language {
    locales
        .into_iter()
        .flatten()
        .find(|value| !value.is_empty())
        .map(|value| Language::from_locale(&value.to_string_lossy()))
        .unwrap_or_default()
}

pub(crate) fn resolve(
    cli: Option<Language>,
    saved: Option<Language>,
    system: Language,
) -> Language {
    cli.or(saved).unwrap_or(system)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolution_respects_overrides_and_posix_locale_precedence() {
        assert_eq!(
            resolve(Some(Language::En), Some(Language::Zh), Language::Zh),
            Language::En
        );
        assert_eq!(
            resolve(None, Some(Language::En), Language::Zh),
            Language::En
        );
        assert_eq!(resolve(None, None, Language::Zh), Language::Zh);
        for locale in ["zh_CN.UTF-8", "zh-Hans", "ZH_TW", "zh"] {
            assert_eq!(Language::from_locale(locale), Language::Zh);
        }
        for locale in ["C", "POSIX", "en_US.UTF-8", "de_DE", ""] {
            assert_eq!(Language::from_locale(locale), Language::En);
        }
        assert_eq!(
            locale_language([Some("C".into()), Some("zh_CN".into()), None]),
            Language::En
        );
        assert_eq!(
            locale_language([Some("".into()), Some("zh_CN".into()), Some("en_US".into())]),
            Language::Zh
        );
    }

    #[test]
    fn config_paths_ignore_empty_or_relative_environment_values() {
        assert_eq!(
            config_path(Some("/tmp/config".into()), Some("/tmp/home".into())),
            Some("/tmp/config/git-tui/language".into())
        );
        for xdg in [None, Some("".into()), Some("relative".into())] {
            assert_eq!(
                config_path(xdg, Some("/tmp/home".into())),
                Some("/tmp/home/.config/git-tui/language".into())
            );
        }
        assert!(config_path(None, None).is_none());
    }

    #[test]
    fn saves_atomically_and_rejects_corrupt_settings() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("git-tui/language");
        let settings = Settings::at(Some(path.clone()));
        assert_eq!(settings.load()?, None);
        settings.save(Language::Zh)?;
        assert_eq!(settings.load()?, Some(Language::Zh));
        settings.save(Language::En)?;
        assert_eq!(fs::read_to_string(&path)?, "en\n");
        for value in [b"invalid".as_slice(), b"\xff", &[b'x'; 65]] {
            fs::write(&path, value)?;
            assert!(settings.load().is_err());
        }
        settings.save(Language::Zh)?;
        assert_eq!(settings.load()?, Some(Language::Zh));
        assert_eq!(fs::read_dir(path.parent().context("parent")?)?.count(), 1);
        Ok(())
    }

    #[test]
    fn failed_save_does_not_replace_an_existing_destination() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("language");
        fs::create_dir(&path)?;
        fs::write(path.join("keep"), "unchanged")?;
        assert!(Settings::at(Some(path.clone())).save(Language::Zh).is_err());
        assert_eq!(fs::read_to_string(path.join("keep"))?, "unchanged");
        assert_eq!(fs::read_dir(dir.path())?.count(), 1);
        assert!(Settings::at(None).save(Language::Zh).is_err());
        Ok(())
    }
}
