use std::{io, panic};

use anyhow::Result;
use ratatui::{
    DefaultTerminal,
    crossterm::{
        cursor::Show,
        event::{
            DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
            EnableFocusChange, EnableMouseCapture,
        },
        execute,
    },
};

pub(crate) struct Session {
    pub terminal: DefaultTerminal,
    _restore: Restore,
}

struct Restore;

fn restore_input() {
    let _ = execute!(
        io::stdout(),
        DisableMouseCapture,
        DisableBracketedPaste,
        DisableFocusChange,
        Show
    );
}

impl Drop for Restore {
    fn drop(&mut self) {
        restore_input();
        ratatui::restore();
    }
}

impl Session {
    pub(crate) fn enter(mouse: bool) -> Result<Self> {
        // Construct the guard first so partial initialization is also restored.
        let restore = Restore;
        let terminal = ratatui::try_init()?;
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            restore_input();
            previous(info);
        }));
        execute!(io::stdout(), EnableBracketedPaste, EnableFocusChange)?;
        if mouse {
            execute!(io::stdout(), EnableMouseCapture)?;
        }
        Ok(Self {
            terminal,
            _restore: restore,
        })
    }
}
