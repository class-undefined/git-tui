mod app;
mod i18n;
mod settings;
mod terminal;
mod ui;
mod worker;

use std::{
    ffi::OsString,
    io::{self, IsTerminal},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use clap::Parser;
use git_tui::git::{Repository, display_bytes, display_path};
use ratatui::crossterm::event;

use app::App;
use i18n::{Language, Message, Text};
use settings::Settings;
use worker::Worker;

#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    /// Repository directory (defaults to the current directory).
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Git executable; also configurable with GIT_TUI_GIT.
    #[arg(long)]
    git: Option<OsString>,

    /// Disable mouse capture to use terminal text selection.
    #[arg(long)]
    no_mouse: bool,

    /// Print repository diagnostics without opening the TUI.
    #[arg(long)]
    check: bool,

    /// UI language for this session / 本次会话语言 (en: English, zh: 中文).
    #[arg(long, value_enum)]
    lang: Option<Language>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let settings = Settings::from_environment();
    let saved = settings.load();
    let language = settings::resolve(
        cli.lang,
        saved.as_ref().ok().copied().flatten(),
        settings::system_language(),
    );
    let warning = saved
        .err()
        .map(|error| display_bytes(format!("{error:#}").as_bytes()));
    run(cli, settings, language, warning).context(language.text(Text::StartupFailed))
}

fn run(cli: Cli, settings: Settings, language: Language, warning: Option<String>) -> Result<()> {
    let binary = cli
        .git
        .or_else(|| std::env::var_os("GIT_TUI_GIT"))
        .unwrap_or_else(|| "git".into());
    let repo = Repository::discover(&cli.path, &binary)?;
    if cli.check {
        let snapshot = repo.snapshot()?;
        println!(
            "{}\n{} {}\nHEAD: {}\n{} {}\n{}: {}\n{} {}",
            repo.version(),
            language.text(Text::Repository),
            display_path(repo.root()),
            i18n::head(&snapshot, language),
            language.text(Text::ChangedFiles),
            snapshot.files.len(),
            language.text(Text::Branches),
            snapshot.branches.len(),
            language.text(Text::RecentCommits),
            snapshot.commits.len()
        );
        if let Some(warning) = warning {
            eprintln!("{} {warning}", language.text(Text::ConfigLoadFailed));
        }
        return Ok(());
    }
    ensure!(
        io::stdin().is_terminal() && io::stdout().is_terminal(),
        language.text(Text::NeedTerminal)
    );
    let mut app = App::new(
        display_path(repo.root()),
        repo.version().to_owned(),
        language,
    );
    if let Some(warning) = warning {
        app.error(Message::Detail(Text::ConfigLoadFailed, warning));
    }
    let interrupted = Arc::new(AtomicBool::new(false));
    for signal in [
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGHUP,
        signal_hook::consts::SIGINT,
    ] {
        signal_hook::flag::register(signal, Arc::clone(&interrupted))?;
    }
    // Worker outlives terminal session: restore the display before joining it.
    let worker = Worker::start(repo, settings)?;
    let mut session = terminal::Session::enter(!cli.no_mouse)?;
    let mut refresh_at = Instant::now();
    while !app.quit && !interrupted.load(Ordering::Relaxed) {
        if let Some(response) = worker.receive()? {
            app.receive(response);
        }
        if refresh_at.elapsed() >= Duration::from_secs(3) && !app.is_busy() && app.modal.is_none() {
            app.request_refresh();
            refresh_at = Instant::now();
        }
        if let Some(request) = app.next_request() {
            worker.send(request)?;
        }
        if app.dirty {
            session.terminal.draw(|frame| ui::draw(frame, &mut app))?;
            app.dirty = false;
        }
        if event::poll(Duration::from_millis(50))? {
            app.handle_event(event::read()?);
        }
    }
    Ok(())
}
