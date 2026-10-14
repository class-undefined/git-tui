use git_tui::git::{DiffMode, display_path};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{Action, App, Focus, HitRegions, InputKind, Modal, View};
use crate::i18n::{self, Language, Text};

const ACCENT: Color = Color::Cyan;
const MUTED: Color = Color::DarkGray;

pub(crate) fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let language = app.language;
    app.hits = HitRegions::default();
    if area.width < 60 || area.height < 16 {
        frame.render_widget(Paragraph::new(language.text(Text::SmallTerminal)), area);
        return;
    }
    let [header, tabs, actions, body, status, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Min(5),
        Constraint::Length(2),
        Constraint::Length(1),
    ])
    .areas(area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    " git-tui ",
                    Style::default()
                        .fg(Color::Black)
                        .bg(ACCENT)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("  {}", app.repository_label)),
            ]),
            Line::from(vec![
                Span::styled(
                    format!(" {}", i18n::head(&app.snapshot, language)),
                    Style::default().fg(Color::Green),
                ),
                Span::styled(
                    format!(
                        "  •  {}{}",
                        app.version,
                        if app.is_busy() {
                            language.text(Text::Working)
                        } else {
                            ""
                        }
                    ),
                    Style::default().fg(MUTED),
                ),
            ]),
        ]),
        header,
    );
    let mut x = tabs.x;
    for (label, view) in [
        (language.text(Text::FilesTab), View::Files),
        (language.text(Text::BranchesTab), View::Branches),
        (language.text(Text::LogTab), View::Log),
    ] {
        let rect = Rect::new(x, tabs.y, 15.min(tabs.right().saturating_sub(x)), 1);
        let style = if app.view == view {
            Style::default()
                .fg(Color::Black)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(MUTED)
        };
        frame.render_widget(Paragraph::new(format!(" {label} ")).style(style), rect);
        app.hits.buttons.push((rect, Action::View(view)));
        x += rect.width;
    }
    let mut buttons = match app.view {
        View::Files => vec![
            (language.text(Text::StageButton), Action::Stage),
            (language.text(Text::UnstageButton), Action::Unstage),
            (language.text(Text::CommitButton), Action::Commit),
            (language.text(Text::DiffButton), Action::ToggleDiff),
        ],
        View::Branches => vec![
            (language.text(Text::SwitchButton), Action::Checkout),
            (language.text(Text::NewButton), Action::NewBranch),
        ],
        View::Log => vec![],
    };
    buttons.extend([
        (language.text(Text::RefreshButton), Action::Refresh),
        (language.text(Text::LanguageButton), Action::Language),
        (language.text(Text::HelpButton), Action::Help),
        (language.text(Text::QuitButton), Action::Quit),
    ]);
    app.hits
        .buttons
        .extend(button_row(frame, actions, &buttons));
    let [list_area, preview_area] = if area.width >= 90 {
        Layout::horizontal([Constraint::Percentage(38), Constraint::Percentage(62)]).areas(body)
    } else {
        Layout::vertical([Constraint::Percentage(45), Constraint::Percentage(55)]).areas(body)
    };
    let title = match app.view {
        View::Files => format!(
            " {} ({})  {} ",
            language.text(Text::Files),
            app.snapshot.files.len(),
            language.text(Text::IndexLegend)
        ),
        View::Branches => format!(
            " {} ({}) ",
            language.text(Text::Branches),
            app.snapshot.branches.len()
        ),
        View::Log => format!(
            " {} ({}/100) ",
            language.text(Text::History),
            app.snapshot.commits.len()
        ),
    };
    let list_block = pane(title, app.focus == Focus::List);
    app.hits.list = list_block.inner(list_area);
    let items: Vec<ListItem<'static>> = match app.view {
        View::Files => app
            .snapshot
            .files
            .iter()
            .map(|file| {
                let color = if file.is_conflicted() {
                    Color::Red
                } else if file.is_staged() {
                    Color::Green
                } else if file.is_untracked() {
                    Color::Yellow
                } else {
                    Color::White
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{}{} ", char::from(file.index), char::from(file.worktree)),
                        Style::default().fg(color),
                    ),
                    Span::raw(display_path(&file.path)),
                ]))
            })
            .collect(),
        View::Branches => app
            .snapshot
            .branches
            .iter()
            .map(|branch| {
                ListItem::new(format!(
                    "{} {}",
                    if branch.current { "*" } else { " " },
                    App::branch_label(&branch.name)
                ))
            })
            .collect(),
        View::Log => app
            .snapshot
            .commits
            .iter()
            .map(|commit| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{} ", &commit.id[..commit.id.len().min(8)]),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::raw(format!("{} · {}", commit.summary, commit.author)),
                ]))
            })
            .collect(),
    };
    frame.render_stateful_widget(
        List::new(items)
            .block(list_block)
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("› "),
        list_area,
        &mut app.lists[app.view.index()],
    );
    let title = match app.view {
        View::Files => match app.diff_mode {
            DiffMode::Worktree => language.text(Text::WorktreeDiff),
            DiffMode::Staged => language.text(Text::StagedDiff),
        },
        View::Branches => language.text(Text::BranchActions),
        View::Log => language.text(Text::CommitDetails),
    };
    let preview_block = pane(title.to_owned(), app.focus == Focus::Preview);
    app.hits.preview = preview_block.inner(preview_area);
    let max_scroll = app
        .preview
        .lines()
        .count()
        .saturating_sub(usize::from(app.hits.preview.height))
        .min(usize::from(u16::MAX)) as u16;
    app.preview_scroll = app.preview_scroll.min(max_scroll);
    let max_column = app
        .preview
        .lines()
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0)
        .saturating_sub(usize::from(app.hits.preview.width))
        .min(usize::from(u16::MAX)) as u16;
    app.preview_column = app.preview_column.min(max_column);
    // Only style visible lines; a large diff must not stall the draw loop.
    let lines: Vec<Line<'_>> = app
        .preview
        .lines()
        .skip(usize::from(app.preview_scroll))
        .take(usize::from(app.hits.preview.height))
        .map(|line| {
            let color = if line.starts_with('+') {
                Color::Green
            } else if line.starts_with('-') {
                Color::Red
            } else if line.starts_with("@@") {
                ACCENT
            } else {
                Color::Reset
            };
            Line::styled(line, Style::default().fg(color))
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines)
            .block(preview_block)
            .scroll((0, app.preview_column)),
        preview_area,
    );
    frame.render_widget(
        Paragraph::new(app.message.render(language))
            .style(Style::default().fg(if app.is_error {
                Color::Red
            } else {
                Color::Yellow
            }))
            .wrap(Wrap { trim: false }),
        status,
    );
    frame.render_widget(
        Paragraph::new(language.text(Text::Footer)).style(Style::default().fg(MUTED)),
        footer,
    );
    if app.modal.is_some() {
        modal(frame, app);
    }
}

fn pane(title: String, focused: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(if focused { ACCENT } else { MUTED }))
}

fn button_row(frame: &mut Frame, area: Rect, buttons: &[(&str, Action)]) -> Vec<(Rect, Action)> {
    let mut hits = Vec::new();
    let mut x = area.x;
    let mut y = area.y;
    for (label, action) in buttons {
        let width = (label.width() as u16 + 4).min(area.width);
        if x + width > area.right() {
            x = area.x;
            y += 1;
        }
        if y >= area.bottom() {
            break;
        }
        let rect = Rect::new(x, y, width, 1);
        frame.render_widget(
            Paragraph::new(format!(" [{label}] ")).style(Style::default().fg(ACCENT)),
            rect,
        );
        hits.push((rect, *action));
        x += width;
    }
    hits
}

fn modal(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let language = app.language;
    let width = area.width.saturating_sub(6).min(76);
    let height = if matches!(app.modal, Some(Modal::Help)) {
        22
    } else {
        9
    }
    .min(area.height.saturating_sub(2));
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, rect);
    let Some(modal) = &app.modal else {
        return;
    };
    let title = match modal {
        Modal::Input {
            kind: InputKind::Commit,
            ..
        } => language.text(Text::CommitTitle),
        Modal::Input {
            kind: InputKind::Branch,
            ..
        } => language.text(Text::BranchTitle),
        Modal::Checkout(_) => language.text(Text::CheckoutTitle),
        Modal::Help => language.text(Text::HelpTitle),
        Modal::Language { .. } => language.text(Text::LanguageTitle),
    };
    let block = pane(title.to_owned(), true);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let content = Rect::new(
        inner.x + 1,
        inner.y,
        inner.width.saturating_sub(2),
        inner.height.saturating_sub(2),
    );
    match modal {
        Modal::Input { kind, text } => {
            let label = match kind {
                InputKind::Commit => language.text(Text::CommitPrompt),
                InputKind::Branch => language.text(Text::BranchPrompt),
            };
            let visible = input_tail(text, content.width.saturating_sub(2) as usize);
            frame.render_widget(
                Paragraph::new(vec![
                    Line::raw(label),
                    Line::raw(""),
                    Line::styled(format!("> {visible}"), Style::default().fg(ACCENT)),
                    Line::raw(""),
                    Line::styled(language.text(Text::InputHint), Style::default().fg(MUTED)),
                ]),
                content,
            );
            frame.set_cursor_position((content.x + 2 + visible.width() as u16, content.y + 2));
        }
        Modal::Checkout(name) => {
            let prompt = match language {
                Language::En => format!("Switch to {}?", App::branch_label(name)),
                Language::Zh => format!("切换到 {}？", App::branch_label(name)),
            };
            frame.render_widget(
                Paragraph::new(format!("{prompt}\n\n{}", language.text(Text::CheckoutHint)))
                    .wrap(Wrap { trim: false }),
                content,
            );
        }
        Modal::Help => {
            frame.render_widget(Paragraph::new(language.text(Text::Help)), content);
        }
        Modal::Language { selected } => {
            frame.render_widget(
                Paragraph::new(format!(
                    "{} {}",
                    language.text(Text::SelectedLanguage),
                    selected.name()
                )),
                content,
            );
            app.hits.modal_buttons.extend(button_row(
                frame,
                Rect::new(content.x, content.y + 1, content.width, 1),
                &[
                    ("1 English", Action::ChooseLanguage(Language::En)),
                    ("2 中文", Action::ChooseLanguage(Language::Zh)),
                ],
            ));
            frame.render_widget(
                Paragraph::new(language.text(Text::LanguageHint)).wrap(Wrap { trim: false }),
                Rect::new(content.x, content.y + 3, content.width, 2),
            );
        }
    }
    let button_area = Rect::new(
        inner.x + 1,
        inner.bottom().saturating_sub(1),
        inner.width.saturating_sub(2),
        1,
    );
    let buttons = if matches!(modal, Modal::Help) {
        vec![(language.text(Text::Close), Action::Cancel)]
    } else {
        vec![
            (language.text(Text::Confirm), Action::Confirm),
            (language.text(Text::Cancel), Action::Cancel),
        ]
    };
    app.hits
        .modal_buttons
        .extend(button_row(frame, button_area, &buttons));
}

fn input_tail(text: &str, max_width: usize) -> &str {
    let mut width = 0;
    let mut start = text.len();
    for (index, ch) in text.char_indices().rev() {
        width += ch.width().unwrap_or(0);
        if width > max_width {
            break;
        }
        start = index;
    }
    &text[start..]
}
