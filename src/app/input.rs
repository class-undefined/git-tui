use ratatui::{
    crossterm::event::{
        Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
        MouseEventKind,
    },
    layout::Position,
};

use super::{Action, App, Focus, HitRegions, Modal, View};

impl App {
    pub(crate) fn handle_event(&mut self, event: Event) {
        match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => self.key(key),
            Event::Mouse(mouse) => self.mouse(mouse),
            Event::Paste(text) => {
                if let Some(Modal::Input { text: value, .. }) = &mut self.modal {
                    for character in text
                        .chars()
                        .take(4096_usize.saturating_sub(value.chars().count()))
                    {
                        if character == '\n' || character == '\r' || character == '\t' {
                            value.push(' ');
                        } else if !character.is_control() {
                            value.push(character);
                        }
                    }
                    self.dirty = true;
                }
            }
            Event::Resize(_, _) => {
                self.hits = HitRegions::default();
                self.dirty = true;
            }
            Event::FocusGained => self.request_refresh(),
            _ => {}
        }
    }

    fn key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.act(Action::Quit);
            return;
        }
        if self.modal.is_some() {
            match key.code {
                KeyCode::Esc => self.act(Action::Cancel),
                KeyCode::Enter => self.act(Action::Confirm),
                KeyCode::Backspace => {
                    if let Some(Modal::Input { text, .. }) = &mut self.modal {
                        text.pop();
                        self.dirty = true;
                    }
                }
                KeyCode::Char(c)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    if let Some(Modal::Input { text, .. }) = &mut self.modal {
                        if !c.is_control() && text.chars().count() < 4096 {
                            text.push(c);
                            self.dirty = true;
                        }
                    } else if c == 'q' || c == '?' {
                        self.act(Action::Cancel);
                    }
                }
                _ => {}
            }
            return;
        }
        let action = match key.code {
            KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
            KeyCode::Char('1') => Action::View(View::Files),
            KeyCode::Char('2') => Action::View(View::Branches),
            KeyCode::Char('3') => Action::View(View::Log),
            KeyCode::Tab | KeyCode::BackTab => Action::Focus(match self.focus {
                Focus::List => Focus::Preview,
                Focus::Preview => Focus::List,
            }),
            KeyCode::Char('h') | KeyCode::Left => Action::Focus(Focus::List),
            KeyCode::Char('H') => Action::ScrollHorizontal(-8),
            KeyCode::Char('L') => Action::ScrollHorizontal(8),
            KeyCode::Char('l') | KeyCode::Right => Action::Focus(Focus::Preview),
            KeyCode::Char('j') | KeyCode::Down => Action::Move(1),
            KeyCode::Char('k') | KeyCode::Up => Action::Move(-1),
            KeyCode::PageDown => Action::Move(10),
            KeyCode::PageUp => Action::Move(-10),
            KeyCode::Home => Action::Move(i32::MIN),
            KeyCode::End => Action::Move(i32::MAX),
            KeyCode::Char('s') | KeyCode::Char(' ') => Action::Stage,
            KeyCode::Char('u') => Action::Unstage,
            KeyCode::Char('c') => Action::Commit,
            KeyCode::Char('n') => Action::NewBranch,
            KeyCode::Enter if self.view == View::Branches => Action::Checkout,
            KeyCode::Enter => Action::Focus(Focus::Preview),
            KeyCode::Char('d') => Action::ToggleDiff,
            KeyCode::Char('r') => Action::Refresh,
            KeyCode::Char('?') => Action::Help,
            _ => return,
        };
        self.act(action);
    }

    fn mouse(&mut self, mouse: MouseEvent) {
        let position = Position::new(mouse.column, mouse.row);
        if self.modal.is_some() {
            if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                && let Some((_, action)) = self
                    .hits
                    .modal_buttons
                    .iter()
                    .find(|(rect, _)| rect.contains(position))
            {
                self.act(*action);
            }
            return;
        }
        match mouse.kind {
            MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight => {
                if self.hits.preview.contains(position) {
                    self.act(Action::ScrollHorizontal(
                        if mouse.kind == MouseEventKind::ScrollLeft {
                            -8
                        } else {
                            8
                        },
                    ));
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some((_, action)) = self
                    .hits
                    .buttons
                    .iter()
                    .find(|(rect, _)| rect.contains(position))
                {
                    self.act(*action);
                } else if self.hits.list.contains(position) {
                    let row = usize::from(mouse.row - self.hits.list.y);
                    let index = self.lists[self.view.index()].offset() + row;
                    self.act(Action::Select(index));
                } else if self.hits.preview.contains(position) {
                    self.act(Action::Focus(Focus::Preview));
                }
            }
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                let delta = if mouse.kind == MouseEventKind::ScrollDown {
                    3
                } else {
                    -3
                };
                if self.hits.list.contains(position) {
                    self.focus = Focus::List;
                    self.act(Action::Move(delta));
                } else if self.hits.preview.contains(position) {
                    self.focus = Focus::Preview;
                    self.act(Action::Scroll(delta));
                }
            }
            _ => {}
        }
    }
}
