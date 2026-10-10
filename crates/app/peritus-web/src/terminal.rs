//! Durable CLI console observers over the existing owned process service.

use crate::{
    error::{Result, problem},
    processes::ManagedCommand,
    state::App,
};
use base64::Engine as _;
use peritus_process::OutputStream;
use serde_json::{Value, json};
use std::sync::Arc;

pub struct Terminal {
    command: Arc<ManagedCommand>,
}
impl Terminal {
    pub(crate) fn start(
        app: &App,
        console: crate::consoles::Console,
        args: Vec<String>,
    ) -> Result<()> {
        let project = app.project(&console.project)?;
        let title = crate::sessions::title(&console.title)?;
        let console = crate::consoles::Console { title: title.as_str().to_owned(), ..console };
        app.update(|state| {
            state.consoles.insert(
                console.id.clone(),
                crate::consoles::SavedConsole { console: console.clone(), closed: false },
            );
            Ok(())
        })?;
        let program = app
            .options
            .cli
            .to_str()
            .ok_or_else(|| problem("CLI executable path must be representable as Unicode"))?
            .to_owned();
        ManagedCommand::start(
            app,
            &format!("console:{}", console.id),
            &project.root,
            program,
            args,
            true,
        )?;
        Ok(())
    }
    pub(crate) fn get(app: &App, id: &str) -> Result<Self> {
        if !app.snapshot()?.consoles.contains_key(id) {
            return Err(problem("Console not found"));
        }
        Ok(Self { command: ManagedCommand::get(app, &format!("console:{id}"))? })
    }
    pub(crate) fn finished(&self) -> Result<bool> {
        Ok(crate::processes::settled(self.command.observe()?.state()))
    }
    pub(crate) fn read(&self, after: u64) -> Result<Value> {
        let observed = self.command.observe()?;
        let (total, bytes) = self.command.output(OutputStream::Terminal, after, 65536)?;
        let next = after
            .checked_add(u64::try_from(bytes.len()).map_err(problem)?)
            .ok_or_else(|| problem("Console offset overflow"))?;
        Ok(
            json!({"data":base64::engine::general_purpose::STANDARD.encode(&bytes),"next":next,"total":total,
            "lost":false,"ended":crate::processes::settled(observed.state()),"state":format!("{:?}", observed.state()),"inputAvailable":self.command.input_available(),"diagnostics":observed.progress()}),
        )
    }
    pub(crate) fn input(&self, text: &str) -> Result<()> {
        self.command.input(text.as_bytes())
    }
    pub(crate) fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.command.resize(rows, cols)
    }
    pub(crate) fn close(&self) -> Result<()> {
        self.command.cancel()
    }
}
