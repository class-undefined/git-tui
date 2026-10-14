use anyhow::{Context, Result};
use git_tui::git::{Branch, FileStatus, HeadState, Preview, PreviewKind};
use ratatui::{
    Terminal,
    backend::TestBackend,
    crossterm::event::{
        Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    },
};

use super::*;

fn snapshot(count: usize) -> Snapshot {
    Snapshot {
        head: "main".to_owned(),
        head_state: HeadState::Branch,
        files: (0..count)
            .map(|i| FileStatus {
                index: b'?',
                worktree: b'?',
                path: format!("file-{i:03}.txt").into(),
                original_path: None,
            })
            .collect(),
        branches: vec![
            Branch {
                name: "main".into(),
                current: true,
            },
            Branch {
                name: "topic".into(),
                current: false,
            },
        ],
        commits: vec![],
    }
}

fn ready(count: usize) -> App {
    let mut app = App::new(
        "/tmp/example".to_owned(),
        "git version 1.8.3.1".to_owned(),
        Language::En,
    );
    assert!(matches!(app.next_request(), Some(Request::Refresh)));
    app.receive(Response::Refreshed(Ok(snapshot(count))));
    app
}

fn draw(app: &mut App, width: u16, height: u16) -> Result<Terminal<TestBackend>> {
    let mut terminal = Terminal::new(TestBackend::new(width, height))?;
    terminal.draw(|frame| crate::ui::draw(frame, app))?;
    Ok(terminal)
}

fn click(app: &mut App, x: u16, y: u16) {
    app.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }));
}

fn button(app: &mut App, action: Action) -> Result<()> {
    let rect = app
        .hits
        .buttons
        .iter()
        .chain(&app.hits.modal_buttons)
        .find(|(_, a)| *a == action)
        .context("button missing")?
        .0;
    click(app, rect.x + 1, rect.y);
    Ok(())
}

fn key(app: &mut App, code: KeyCode) {
    app.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}

#[test]
fn mouse_rows_respect_the_rendered_scroll_offset() -> Result<()> {
    let mut app = ready(80);
    app.act(Action::Select(50));
    draw(&mut app, 120, 30)?;
    let offset = app.lists[0].offset();
    assert!(offset > 0);
    let rect = app.hits.list;
    click(&mut app, rect.x + 4, rect.y + 2);
    assert_eq!(app.selected(), Some(offset + 2));
    button(&mut app, Action::View(View::Branches))?;
    assert_eq!(app.view, View::Branches);
    Ok(())
}

#[test]
fn keyboard_and_mouse_stage_the_same_file() -> Result<()> {
    for use_mouse in [false, true] {
        let mut app = ready(2);
        app.act(Action::Select(1));
        draw(&mut app, 120, 30)?;
        if use_mouse {
            button(&mut app, Action::Stage)?;
        } else {
            key(&mut app, KeyCode::Char('s'));
        }
        match app.next_request() {
            Some(Request::Mutate(Mutation::Stage(file))) => {
                assert_eq!(file.path.to_string_lossy(), "file-001.txt")
            }
            _ => panic!("expected a stage request"),
        }
        app.act(Action::Quit);
        assert!(!app.quit, "must not abandon a mutation");
        assert!(app.next_request().is_none());
    }
    Ok(())
}

#[test]
fn modal_blocks_background_clicks_and_keeps_failed_input() -> Result<()> {
    let mut app = ready(2);
    app.act(Action::Commit);
    draw(&mut app, 120, 30)?;
    button(&mut app, Action::Stage)?;
    assert!(app.pending_mutation.is_none());
    app.handle_event(Event::Paste("修复\nissue\x1b".to_owned()));
    button(&mut app, Action::Confirm)?;
    match app.next_request() {
        Some(Request::Mutate(Mutation::Commit(text))) => assert_eq!(text, "修复 issue"),
        _ => panic!("expected a commit request"),
    }
    app.receive(Response::Mutated {
        mutation: Mutation::Commit("修复 issue".to_owned()),
        outcome: Err("hook failed".to_owned()),
        snapshot: Ok(snapshot(2)),
    });
    assert!(app.is_error);
    match app.modal {
        Some(Modal::Input { text, .. }) => assert_eq!(text, "修复 issue"),
        _ => panic!("input should survive a failed commit"),
    }
    Ok(())
}

#[test]
fn branch_checkout_requires_confirm_and_cancel_is_inert() -> Result<()> {
    let mut app = ready(0);
    app.act(Action::View(View::Branches));
    app.act(Action::Select(1));
    app.act(Action::Checkout);
    draw(&mut app, 120, 30)?;
    assert!(app.next_request().is_none());
    button(&mut app, Action::Cancel)?;
    assert!(app.pending_mutation.is_none());
    app.act(Action::Checkout);
    draw(&mut app, 120, 30)?;
    button(&mut app, Action::Confirm)?;
    assert!(
        matches!(app.next_request(), Some(Request::Mutate(Mutation::Checkout(name))) if name == "topic")
    );
    Ok(())
}

#[test]
fn delayed_preview_cannot_replace_a_new_selection() -> Result<()> {
    let mut app = ready(2);
    let Some(Request::Preview { generation, target }) = app.next_request() else {
        panic!("preview expected");
    };
    app.act(Action::Select(1));
    app.receive(Response::Preview {
        generation,
        target,
        result: Ok(preview("stale file zero")),
    });
    assert!(!app.preview.contains("stale"));
    let Some(Request::Preview { generation, target }) = app.next_request() else {
        panic!("new preview expected");
    };
    app.receive(Response::Preview {
        generation,
        target,
        result: Ok(preview("file one")),
    });
    assert_eq!(app.preview, "file one");
    Ok(())
}

#[test]
fn refresh_preserves_selected_path_and_preview_scroll() -> Result<()> {
    let mut app = ready(4);
    app.act(Action::Select(2));
    app.preview = (0..100).map(|i| format!("line {i}\n")).collect();
    app.preview_scroll = 20;
    let mut refreshed = snapshot(4);
    refreshed.files.remove(0);
    app.receive(Response::Refreshed(Ok(refreshed)));
    assert_eq!(app.selected(), Some(1));
    assert_eq!(app.preview_scroll, 20);
    assert!(app.preview.starts_with("line 0"));
    draw(&mut app, 120, 30)?;
    assert_eq!(app.preview_scroll, 20);
    Ok(())
}

#[test]
fn mouse_wheel_scrolls_the_hovered_pane_and_clamps() -> Result<()> {
    let mut app = ready(10);
    app.preview = (0..100).map(|i| format!("{i}\n")).collect();
    draw(&mut app, 120, 30)?;
    let rect = app.hits.preview;
    app.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: rect.x + 1,
        row: rect.y,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(app.preview_scroll, 3);
    assert_eq!(app.selected(), Some(0));
    app.act(Action::Scroll(i32::MAX));
    assert_eq!(
        usize::from(app.preview_scroll),
        100 - usize::from(rect.height)
    );
    app.preview = "long line ".repeat(100);
    key(&mut app, KeyCode::Char('L'));
    assert_eq!(app.preview_column, 8);
    key(&mut app, KeyCode::Char('H'));
    assert_eq!(app.preview_column, 0);
    app.preview_column = 100;
    app.preview = "content became shorter after refresh".to_owned();
    draw(&mut app, 120, 30)?;
    assert_eq!(
        app.preview_column, 0,
        "shorter diffs must not leave the pane blank"
    );
    Ok(())
}

#[test]
fn resize_discards_stale_hit_regions_and_small_layouts_render() -> Result<()> {
    let mut app = ready(0);
    draw(&mut app, 120, 30)?;
    app.handle_event(Event::Resize(20, 8));
    assert!(app.hits.buttons.is_empty());
    for (width, height) in [(0, 0), (20, 8), (60, 16), (80, 24), (160, 48)] {
        draw(&mut app, width, height)?;
    }
    app.modal = Some(Modal::Input {
        kind: InputKind::Commit,
        text: "测试终端宽度🙂".repeat(100),
    });
    for (width, height) in [(60, 16), (80, 24), (160, 48)] {
        let mut terminal = draw(&mut app, width, height)?;
        let cursor = terminal.get_cursor_position()?;
        assert!(cursor.x < width && cursor.y < height);
    }
    Ok(())
}

fn preview(text: &str) -> Preview {
    Preview {
        text: text.to_owned(),
        kind: PreviewKind::Diff,
        truncated: false,
    }
}

fn screen(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        let mut x = 0;
        while x < buffer.area.width {
            let symbol = buffer[(x, y)].symbol();
            text.push_str(symbol);
            x += symbol.width().max(1) as u16;
        }
        text.push('\n');
    }
    text
}

#[test]
fn language_settings_work_with_mouse_and_keyboard_at_supported_sizes() -> Result<()> {
    for (width, height) in [(60, 16), (80, 24), (120, 34)] {
        for mouse in [false, true] {
            let mut app = ready(0);
            draw(&mut app, width, height)?;
            if mouse {
                button(&mut app, Action::Language)?;
            } else {
                key(&mut app, KeyCode::F(2));
            }
            draw(&mut app, width, height)?;
            if mouse {
                button(&mut app, Action::ChooseLanguage(Language::Zh))?;
            } else {
                key(&mut app, KeyCode::Right);
            }
            draw(&mut app, width, height)?;
            if mouse {
                button(&mut app, Action::Confirm)?;
            } else {
                key(&mut app, KeyCode::Enter);
            }
            assert_eq!(app.language, Language::Zh);
            assert!(matches!(
                app.next_request(),
                Some(Request::SaveLanguage(Language::Zh))
            ));
            app.act(Action::Quit);
            assert!(!app.quit, "must finish saving settings before exiting");
            app.receive(Response::LanguageSaved {
                language: Language::Zh,
                result: Ok(()),
            });
            assert!(app.message.render(app.language).contains("语言设置已保存"));
            let rendered = screen(&draw(&mut app, width, height)?);
            assert!(rendered.contains("1 文件"), "{rendered}");
            assert!(rendered.contains("工作区干净"), "{rendered}");
            assert!(!rendered.contains("Working tree clean"));
            button(&mut app, Action::Help)?;
            assert!(screen(&draw(&mut app, width, height)?).contains("键盘与鼠标"));
        }
    }
    Ok(())
}

#[test]
fn language_cancel_preserves_input_and_settings_and_failed_save_is_reported() -> Result<()> {
    let mut app = ready(0);
    app.act(Action::Language);
    key(&mut app, KeyCode::Char('2'));
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.language, Language::En);
    assert!(app.pending_language.is_none());
    app.act(Action::Commit);
    app.handle_event(Event::Paste("中文 subject".to_owned()));
    key(&mut app, KeyCode::F(2));
    assert!(matches!(&app.modal, Some(Modal::Input {text, ..}) if text == "中文 subject"));
    app.act(Action::Cancel);
    app.act(Action::Language);
    app.act(Action::ChooseLanguage(Language::Zh));
    app.act(Action::Confirm);
    assert!(matches!(
        app.next_request(),
        Some(Request::SaveLanguage(Language::Zh))
    ));
    app.receive(Response::LanguageSaved {
        language: Language::Zh,
        result: Err("read-only filesystem".to_owned()),
    });
    assert_eq!(app.language, Language::Zh);
    assert!(app.is_error);
    assert!(app.message.render(app.language).contains("无法保存"));
    assert!(
        app.message
            .render(app.language)
            .contains("read-only filesystem")
    );
    Ok(())
}

#[test]
fn language_switch_preserves_selection_scroll_and_in_flight_preview() -> Result<()> {
    let mut app = ready(4);
    app.act(Action::Select(2));
    let body: String = (0..100).map(|i| format!("line {i} 原文\n")).collect();
    app.set_preview(PreviewContent::Repository(preview(&body)));
    app.preview_scroll = 20;
    let Some(Request::Preview { generation, target }) = app.next_request() else {
        panic!("preview expected")
    };
    app.act(Action::Language);
    app.act(Action::ChooseLanguage(Language::Zh));
    app.act(Action::Confirm);
    assert_eq!(app.selected(), Some(2));
    assert_eq!(app.preview_scroll, 20);
    assert_eq!(app.preview, body);
    assert!(
        app.next_request().is_none(),
        "keep one worker request in flight"
    );
    app.receive(Response::Preview {
        generation,
        target,
        result: Ok(Preview {
            text: body.clone(),
            kind: PreviewKind::UntrackedText,
            truncated: true,
        }),
    });
    assert!(app.preview.starts_with("未跟踪文件\n"));
    assert!(app.preview.contains(&body));
    assert!(app.preview.ends_with("[预览已截断至 256 KiB]"));
    assert!(matches!(
        app.next_request(),
        Some(Request::SaveLanguage(Language::Zh))
    ));
    app.receive(Response::LanguageSaved {
        language: Language::Zh,
        result: Ok(()),
    });
    app.act(Action::View(View::Branches));
    assert!(app.preview.contains("本地分支"));
    app.act(Action::View(View::Log));
    assert!(app.preview.contains("还没有提交"));
    Ok(())
}

#[test]
fn delayed_settings_and_git_results_use_the_current_language() {
    let mut app = ready(0);
    app.set_language(Language::Zh);
    assert!(matches!(
        app.next_request(),
        Some(Request::SaveLanguage(Language::Zh))
    ));
    app.set_language(Language::En);
    app.receive(Response::LanguageSaved {
        language: Language::Zh,
        result: Ok(()),
    });
    assert_eq!(
        app.message.render(app.language),
        "Saving language preference…"
    );
    assert!(matches!(
        app.next_request(),
        Some(Request::SaveLanguage(Language::En))
    ));
    app.receive(Response::LanguageSaved {
        language: Language::En,
        result: Ok(()),
    });
    app.receive(Response::Mutated {
        mutation: Mutation::CreateBranch("中文-topic".to_owned()),
        outcome: Ok(()),
        snapshot: Ok(snapshot(0)),
    });
    assert_eq!(app.message.render(Language::Zh), "已创建分支 中文-topic");
    assert_eq!(
        app.message.render(Language::En),
        "Created branch 中文-topic"
    );
}
