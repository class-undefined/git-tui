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

const ACCENT: Color = Color::Cyan;
const MUTED: Color = Color::DarkGray;

pub(crate) fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    app.hits = HitRegions::default();
    if area.width < 60 || area.height < 16 {
        frame.render_widget(Paragraph::new("git-tui\nTerminal too small: need 60 columns × 16 rows.\nResize, or Esc then q to quit."), area);
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
                    format!(" {}", app.snapshot.head),
                    Style::default().fg(Color::Green),
                ),
                Span::styled(
                    format!(
                        "  •  {}{}",
                        app.version,
                        if app.is_busy() {
                            "  •  working…"
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
        ("1 Files", View::Files),
        ("2 Branches", View::Branches),
        ("3 Log", View::Log),
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
            ("s Stage", Action::Stage),
            ("u Unstage", Action::Unstage),
            ("c Commit", Action::Commit),
            ("d Diff", Action::ToggleDiff),
        ],
        View::Branches => vec![
            ("Enter Switch", Action::Checkout),
            ("n New", Action::NewBranch),
        ],
        View::Log => vec![],
    };
    buttons.extend([
        ("r Refresh", Action::Refresh),
        ("? Help", Action::Help),
        ("q Quit", Action::Quit),
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
        View::Files => format!(" Files ({})  X=index Y=worktree ", app.snapshot.files.len()),
        View::Branches => format!(" Local branches ({}) ", app.snapshot.branches.len()),
        View::Log => format!(" History ({}/100) ", app.snapshot.commits.len()),
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
            DiffMode::Worktree => " Worktree diff · d: staged ",
            DiffMode::Staged => " Staged diff · d: worktree ",
        },
        View::Branches => " Branch actions ",
        View::Log => " Commit details ",
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
        Paragraph::new(app.message.as_str())
            .style(Style::default().fg(if app.is_error {
                Color::Red
            } else {
                Color::Yellow
            }))
            .wrap(Wrap { trim: false }),
        status,
    );
    frame.render_widget(
        Paragraph::new(" Click: select/action  Wheel: scroll  Tab: focus  1/2/3: views  ?: help")
            .style(Style::default().fg(MUTED)),
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
    let width = area.width.saturating_sub(6).min(76);
    let height = if matches!(app.modal, Some(Modal::Help)) {
        21
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
        } => " Commit staged changes ",
        Modal::Input {
            kind: InputKind::Branch,
            ..
        } => " Create and switch branch ",
        Modal::Checkout(_) => " Switch branch ",
        Modal::Help => " Keyboard & mouse ",
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
                InputKind::Commit => "Commit summary (staged changes only):",
                InputKind::Branch => "New branch name:",
            };
            let visible = input_tail(text, content.width.saturating_sub(2) as usize);
            frame.render_widget(
                Paragraph::new(vec![
                    Line::raw(label),
                    Line::raw(""),
                    Line::styled(format!("> {visible}"), Style::default().fg(ACCENT)),
                    Line::raw(""),
                    Line::styled(
                        "Enter: submit   Esc: cancel   Backspace: delete",
                        Style::default().fg(MUTED),
                    ),
                ]),
                content,
            );
            frame.set_cursor_position((content.x + 2 + visible.width() as u16, content.y + 2));
        }
        Modal::Checkout(name) => {
            frame.render_widget(Paragraph::new(format!("Switch to {}?\n\nUncommitted changes are kept when Git can carry them safely; conflicting changes block checkout.", App::branch_label(name))).wrap(Wrap { trim: false }), content);
        }
        Modal::Help => {
            frame.render_widget(Paragraph::new("1 / 2 / 3     Files / branches / history\nj / k or ↑/↓  Move selection / scroll preview\nTab or h / l  Focus list / preview\nH / L         Scroll preview horizontally\nPgUp / PgDn   Move ten rows\ns or Space    Stage selected file\nu             Unstage (keep worktree)\nd             Worktree / staged diff\nc             Commit staged changes\nn             Create and switch branch\nEnter         Switch branch / focus preview\nr             Refresh (also every 3 seconds)\nq / Ctrl-C    Quit when no mutation is running\n\nMouse: click tabs, rows and buttons; wheel over\na pane scrolls it. Esc closes dialogs."), content);
        }
    }
    let button_area = Rect::new(
        inner.x + 1,
        inner.bottom().saturating_sub(1),
        inner.width.saturating_sub(2),
        1,
    );
    let buttons = if matches!(modal, Modal::Help) {
        vec![("Close", Action::Cancel)]
    } else {
        vec![("Confirm", Action::Confirm), ("Cancel", Action::Cancel)]
    };
    app.hits.modal_buttons = button_row(frame, button_area, &buttons);
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
