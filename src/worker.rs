use std::{
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError},
    thread::{self, JoinHandle},
};

use anyhow::{Context, Result, bail};
use git_tui::git::{DiffMode, FileStatus, Mutation, Repository, Snapshot};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PreviewTarget {
    File(FileStatus, DiffMode),
    Commit(String),
}

pub(crate) enum Request {
    Refresh,
    Preview {
        generation: u64,
        target: PreviewTarget,
    },
    Mutate(Mutation),
}

pub(crate) enum Response {
    Refreshed(Result<Snapshot, String>),
    Preview {
        generation: u64,
        target: PreviewTarget,
        result: Result<String, String>,
    },
    Mutated {
        outcome: Result<String, String>,
        snapshot: Result<Snapshot, String>,
    },
}

pub(crate) struct Worker {
    sender: Option<SyncSender<Request>>,
    receiver: Option<Receiver<Response>>,
    handle: Option<JoinHandle<()>>,
}

impl Worker {
    pub(crate) fn start(repo: Repository) -> Result<Self> {
        let (sender, jobs) = mpsc::sync_channel(1);
        let (results, receiver) = mpsc::sync_channel(1);
        let handle = thread::Builder::new()
            .name("git-worker".to_owned())
            .spawn(move || {
                while let Ok(request) = jobs.recv() {
                    let response = match request {
                        Request::Refresh => {
                            Response::Refreshed(repo.snapshot().map_err(|e| format!("{e:#}")))
                        }
                        Request::Preview { generation, target } => {
                            let result = match &target {
                                PreviewTarget::File(file, mode) => repo.diff(file, *mode),
                                PreviewTarget::Commit(id) => repo.show(id),
                            }
                            .map_err(|e| format!("{e:#}"));
                            Response::Preview {
                                generation,
                                target,
                                result,
                            }
                        }
                        Request::Mutate(mutation) => {
                            let outcome = repo.apply(&mutation).map_err(|e| format!("{e:#}"));
                            // A hook or timeout can leave a changed index even on failure.
                            let snapshot = repo.snapshot().map_err(|e| format!("{e:#}"));
                            Response::Mutated { outcome, snapshot }
                        }
                    };
                    if results.send(response).is_err() {
                        break;
                    }
                }
            })
            .context("could not start Git worker")?;
        Ok(Self {
            sender: Some(sender),
            receiver: Some(receiver),
            handle: Some(handle),
        })
    }

    pub(crate) fn send(&self, request: Request) -> Result<()> {
        self.sender
            .as_ref()
            .context("Git worker is closed")?
            .try_send(request)
            .context("Git worker queue unavailable")
    }

    pub(crate) fn receive(&self) -> Result<Option<Response>> {
        match self
            .receiver
            .as_ref()
            .context("Git worker is closed")?
            .try_recv()
        {
            Ok(response) => Ok(Some(response)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => bail!("Git worker stopped unexpectedly"),
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.sender.take();
        self.receiver.take();
        // Never abandon an in-flight mutation or leave a child process behind.
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
