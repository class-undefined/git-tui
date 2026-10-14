mod input;
#[cfg(test)]
mod tests;

use std::ffi::OsString;

use git_tui::git::{DiffMode, Mutation, Snapshot, display_bytes};
use ratatui::{layout::Rect, widgets::ListState};
use std::os::unix::ffi::OsStrExt;
use unicode_width::UnicodeWidthStr;

use crate::worker::{PreviewTarget, Request, Response};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum View {
    #[default]
    Files,
    Branches,
    Log,
}

impl View {
    pub(crate) fn index(self) -> usize {
        match self {
            Self::Files => 0,
            Self::Branches => 1,
            Self::Log => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Focus {
    #[default]
    List,
    Preview,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InputKind {
    Commit,
    Branch,
}

#[derive(Debug)]
pub(crate) enum Modal {
    Input { kind: InputKind, text: String },
    Checkout(OsString),
    Help,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Action {
    View(View),
    Select(usize),
    Move(i32),
    Scroll(i32),
    ScrollHorizontal(i32),
    Focus(Focus),
    Stage,
    Unstage,
    Commit,
    NewBranch,
    Checkout,
    ToggleDiff,
    Refresh,
    Help,
    Confirm,
    Cancel,
    Quit,
}

#[derive(Default)]
pub(crate) struct HitRegions {
    pub list: Rect,
    pub preview: Rect,
    pub buttons: Vec<(Rect, Action)>,
    pub modal_buttons: Vec<(Rect, Action)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Busy {
    Read,
    Mutation,
}

pub(crate) struct App {
    pub snapshot: Snapshot,
    pub view: View,
    pub focus: Focus,
    pub lists: [ListState; 3],
    pub diff_mode: DiffMode,
    pub preview: String,
    pub preview_scroll: u16,
    pub preview_column: u16,
    pub message: String,
    pub is_error: bool,
    pub modal: Option<Modal>,
    pub hits: HitRegions,
    pub quit: bool,
    pub dirty: bool,
    pub repository_label: String,
    pub version: String,
    busy: Option<Busy>,
    pending_mutation: Option<Mutation>,
    submitted_input: Option<(InputKind, String)>,
    refresh_pending: bool,
    preview_pending: bool,
    generation: u64,
}

impl App {
    pub(crate) fn new(repository_label: String, version: String) -> Self {
        Self {
            snapshot: Snapshot::default(),
            view: View::Files,
            focus: Focus::List,
            lists: std::array::from_fn(|_| ListState::default()),
            diff_mode: DiffMode::Worktree,
            preview: "Loading repository…".to_owned(),
            preview_scroll: 0,
            preview_column: 0,
            message: "Click a file or use ↑/↓. Press ? for help.".to_owned(),
            is_error: false,
            modal: None,
            hits: HitRegions::default(),
            quit: false,
            dirty: true,
            repository_label,
            version,
            busy: None,
            pending_mutation: None,
            submitted_input: None,
            refresh_pending: true,
            preview_pending: false,
            generation: 0,
        }
    }

    pub(crate) fn is_busy(&self) -> bool {
        self.busy.is_some() || self.pending_mutation.is_some()
    }

    pub(crate) fn selected(&self) -> Option<usize> {
        self.lists[self.view.index()].selected()
    }

    pub(crate) fn item_count(&self) -> usize {
        match self.view {
            View::Files => self.snapshot.files.len(),
            View::Branches => self.snapshot.branches.len(),
            View::Log => self.snapshot.commits.len(),
        }
    }

    pub(crate) fn request_refresh(&mut self) {
        self.refresh_pending = true;
        self.dirty = true;
    }

    fn target(&self) -> Option<PreviewTarget> {
        let index = self.selected()?;
        match self.view {
            View::Files => self
                .snapshot
                .files
                .get(index)
                .map(|f| PreviewTarget::File(f.clone(), self.diff_mode)),
            View::Log => self
                .snapshot
                .commits
                .get(index)
                .map(|c| PreviewTarget::Commit(c.id.clone())),
            View::Branches => None,
        }
    }

    fn invalidate_preview(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.preview_scroll = 0;
        self.preview_column = 0;
        self.preview_pending = self.target().is_some();
        self.preview = if self.preview_pending { "Loading preview…" } else {
            match self.view {
                View::Files => "Working tree clean.\n\nChanges appear here automatically; press r to refresh.",
                View::Branches => "Local branches\n\nSelect a branch and choose Switch (Enter).\nChoose New (n) to create and switch to a branch.\n\nGit protects uncommitted changes during checkout.",
                View::Log => "No commits yet. Stage a file and create your first commit.",
            }
        }.to_owned();
        self.dirty = true;
    }

    pub(crate) fn next_request(&mut self) -> Option<Request> {
        if self.busy.is_some() {
            return None;
        }
        let request = if let Some(mutation) = self.pending_mutation.take() {
            self.busy = Some(Busy::Mutation);
            self.refresh_pending = false;
            Request::Mutate(mutation)
        } else if self.refresh_pending {
            self.refresh_pending = false;
            self.busy = Some(Busy::Read);
            Request::Refresh
        } else if self.preview_pending {
            self.preview_pending = false;
            let target = self.target()?;
            self.busy = Some(Busy::Read);
            Request::Preview {
                generation: self.generation,
                target,
            }
        } else {
            return None;
        };
        self.dirty = true;
        Some(request)
    }

    pub(crate) fn receive(&mut self, response: Response) {
        self.busy = None;
        self.dirty = true;
        match response {
            Response::Refreshed(result) => match result {
                Ok(snapshot) => self.replace_snapshot(snapshot),
                Err(error) => {
                    self.error(error);
                    self.preview =
                        "Repository could not be refreshed. Press r to retry.".to_owned();
                }
            },
            Response::Preview {
                generation,
                target,
                result,
            } => {
                if generation == self.generation && self.target().as_ref() == Some(&target) {
                    self.preview = match result {
                        Ok(body) => body,
                        Err(error) => format!("Preview failed:\n{error}"),
                    };
                }
            }
            Response::Mutated { outcome, snapshot } => {
                let refresh_error = match snapshot {
                    Ok(snapshot) => {
                        self.replace_snapshot(snapshot);
                        None
                    }
                    Err(error) => Some(error),
                };
                match outcome {
                    Ok(message) => {
                        self.submitted_input = None;
                        self.notice(message);
                    }
                    Err(error) => {
                        if let Some((kind, text)) = self.submitted_input.take() {
                            self.modal = Some(Modal::Input { kind, text });
                        }
                        self.error(error);
                    }
                }
                if let Some(error) = refresh_error {
                    self.error(format!("{}; refresh failed: {error}", self.message));
                }
            }
        }
    }

    fn replace_snapshot(&mut self, snapshot: Snapshot) {
        let old_target = self.target();
        let file = self.lists[0]
            .selected()
            .and_then(|i| self.snapshot.files.get(i))
            .map(|f| f.path.clone());
        let branch = self.lists[1]
            .selected()
            .and_then(|i| self.snapshot.branches.get(i))
            .map(|b| b.name.clone());
        let commit = self.lists[2]
            .selected()
            .and_then(|i| self.snapshot.commits.get(i))
            .map(|c| c.id.clone());
        let identities = [
            snapshot
                .files
                .iter()
                .position(|f| Some(&f.path) == file.as_ref()),
            snapshot
                .branches
                .iter()
                .position(|b| Some(&b.name) == branch.as_ref()),
            snapshot
                .commits
                .iter()
                .position(|c| Some(&c.id) == commit.as_ref()),
        ];
        let counts = [
            snapshot.files.len(),
            snapshot.branches.len(),
            snapshot.commits.len(),
        ];
        for (index, state) in self.lists.iter_mut().enumerate() {
            state.select(if counts[index] == 0 {
                None
            } else {
                Some(
                    identities[index]
                        .unwrap_or_else(|| state.selected().unwrap_or(0).min(counts[index] - 1)),
                )
            });
        }
        self.snapshot = snapshot;
        if old_target.is_some() && old_target == self.target() {
            // External edits can keep the same XY status. Reload the diff without
            // blanking it or losing the reader's scroll position every three seconds.
            self.generation = self.generation.wrapping_add(1);
            self.preview_pending = true;
        } else {
            self.invalidate_preview();
        }
    }

    pub(crate) fn notice(&mut self, message: String) {
        self.message = message;
        self.is_error = false;
        self.dirty = true;
    }
    fn error(&mut self, message: String) {
        self.message = message;
        self.is_error = true;
        self.dirty = true;
    }

    fn queue(&mut self, mutation: Mutation) {
        if self.busy == Some(Busy::Mutation) || self.pending_mutation.is_some() {
            self.notice("A Git operation is already running.".to_owned());
            return;
        }
        self.pending_mutation = Some(mutation);
        self.notice("Running Git operation…".to_owned());
    }

    pub(crate) fn act(&mut self, action: Action) {
        self.dirty = true;
        if (self.busy == Some(Busy::Mutation) || self.pending_mutation.is_some())
            && matches!(
                action,
                Action::Stage
                    | Action::Unstage
                    | Action::Commit
                    | Action::NewBranch
                    | Action::Checkout
                    | Action::Confirm
            )
        {
            self.notice("A Git operation is already running.".to_owned());
            return;
        }
        if self.modal.is_some()
            && !matches!(action, Action::Confirm | Action::Cancel | Action::Quit)
        {
            return;
        }
        match action {
            Action::View(view) => {
                self.view = view;
                self.focus = Focus::List;
                self.invalidate_preview();
            }
            Action::Select(index) => {
                if index < self.item_count() {
                    self.lists[self.view.index()].select(Some(index));
                    self.focus = Focus::List;
                    self.invalidate_preview();
                }
            }
            Action::Move(delta) => {
                if self.focus == Focus::Preview {
                    self.act(Action::Scroll(delta));
                } else if self.item_count() != 0 {
                    let index = self
                        .selected()
                        .unwrap_or(0)
                        .saturating_add_signed(delta as isize)
                        .min(self.item_count() - 1);
                    self.act(Action::Select(index));
                }
            }
            Action::Scroll(delta) => {
                let max = self
                    .preview
                    .lines()
                    .count()
                    .saturating_sub(usize::from(self.hits.preview.height))
                    .min(usize::from(u16::MAX));
                self.preview_scroll = usize::from(self.preview_scroll)
                    .saturating_add_signed(delta as isize)
                    .min(max) as u16;
            }
            Action::Focus(focus) => self.focus = focus,
            Action::ScrollHorizontal(delta) => {
                let max = self
                    .preview
                    .lines()
                    .map(UnicodeWidthStr::width)
                    .max()
                    .unwrap_or(0)
                    .saturating_sub(usize::from(self.hits.preview.width))
                    .min(usize::from(u16::MAX));
                self.preview_column = usize::from(self.preview_column)
                    .saturating_add_signed(delta as isize)
                    .min(max) as u16;
            }
            Action::Stage | Action::Unstage => {
                if self.view == View::Files
                    && let Some(file) = self
                        .selected()
                        .and_then(|i| self.snapshot.files.get(i))
                        .cloned()
                {
                    if action == Action::Stage {
                        self.queue(Mutation::Stage(file));
                    } else if file.is_staged() || file.is_conflicted() {
                        self.queue(Mutation::Unstage(file));
                    } else {
                        self.notice("This file has no staged changes.".to_owned());
                    }
                }
            }
            Action::Commit => {
                self.modal = Some(Modal::Input {
                    kind: InputKind::Commit,
                    text: String::new(),
                })
            }
            Action::NewBranch => {
                self.modal = Some(Modal::Input {
                    kind: InputKind::Branch,
                    text: String::new(),
                })
            }
            Action::Checkout => {
                if self.view == View::Branches
                    && let Some(branch) =
                        self.selected().and_then(|i| self.snapshot.branches.get(i))
                {
                    if branch.current {
                        self.notice("Already on this branch.".to_owned());
                    } else {
                        self.modal = Some(Modal::Checkout(branch.name.clone()));
                    }
                }
            }
            Action::ToggleDiff => {
                self.diff_mode = match self.diff_mode {
                    DiffMode::Worktree => DiffMode::Staged,
                    DiffMode::Staged => DiffMode::Worktree,
                };
                self.invalidate_preview();
            }
            Action::Refresh => self.request_refresh(),
            Action::Help => self.modal = Some(Modal::Help),
            Action::Confirm => match self.modal.take() {
                Some(Modal::Input { kind, text }) if text.trim().is_empty() => {
                    self.modal = Some(Modal::Input { kind, text });
                    self.error("Please enter a value.".to_owned());
                }
                Some(Modal::Input { kind, text }) => {
                    self.submitted_input = Some((kind, text.clone()));
                    self.queue(match kind {
                        InputKind::Commit => Mutation::Commit(text),
                        InputKind::Branch => Mutation::CreateBranch(text),
                    });
                }
                Some(Modal::Checkout(name)) => self.queue(Mutation::Checkout(name)),
                Some(Modal::Help) | None => {}
            },
            Action::Cancel => self.modal = None,
            Action::Quit => {
                if self.busy == Some(Busy::Mutation) || self.pending_mutation.is_some() {
                    self.notice(
                        "Waiting for the Git operation to finish; then press q.".to_owned(),
                    );
                } else {
                    self.quit = true;
                }
            }
        }
    }

    pub(crate) fn branch_label(name: &OsString) -> String {
        display_bytes(name.as_bytes())
    }
}
