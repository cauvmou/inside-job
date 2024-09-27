mod session;
mod storage;
mod web;
mod command;

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, LockResult, RwLock, RwLockReadGuard, RwLockWriteGuard};
use clap::Parser as ClapParser;
use pest::Parser;
use log::{error, info, trace, warn};
use pest::error::{Error, InputLocation};
use reedline::{Prompt, PromptEditMode, PromptHistorySearch};
use crate::parser::Rule;
use crate::storage::SessionStore;

static LOGO: &'static str = "
            ┌───┐
──┬── ┬┐  ┬ │   │ ──┬── ┬──┐  ┬───┐           ┬ ┌───┐ ┬──┐
  │   │└┐ │ │   ┴   │   │  └┐ │               │ │   │ │  │
  │   │ │ │ └───┐   │   │   │ ┼──┼   ───      │ │   │ ┼──┴┐
  │   │ └┐│ ┬   │   │   │  ┌┘ │           ┬   │ │   │ │   │
──┴── ┴  └┴ │   │ ──┴── ┴──┘  ┴───┘       └───┘ └───┘ ┴───┘
            └───┘                           made by cauvmou
                                         based on hoaxshell

";

#[derive(clap::Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "0.0.0.0")]
    address: std::net::Ipv4Addr,
    #[arg(short, long, default_value_t = 4132)]
    port: u16,
    #[arg(short, long, default_value_t = log::LevelFilter::Info)]
    loglevel: log::LevelFilter,
    #[arg(
        short,
        long,
        value_name = "FILE",
        requires = "cert",
        help = "TLS key for HTTPS, must be PEM format."
    )]
    key: Option<std::path::PathBuf>,
    #[arg(
        short,
        long,
        value_name = "FILE",
        requires = "key",
        help = "TLS cert for HTTPS, must be PEM format."
    )]
    cert: Option<std::path::PathBuf>,
}

mod parser {
    use pest_derive::Parser;

    #[derive(Parser)]
    #[grammar = "grammar.pest"]
    pub struct Parser;
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();

    // init logger
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Warn)
        .with_module_level("insidejob", args.loglevel)
        .init().expect("Failed to start logger!");

    // create store
    let session_store = Arc::new(RwLock::new(SessionStore::default()));
    let mut alias_store: HashMap<String, u128> = HashMap::new();
    let mut active_session: Option<u128> = None;

    // start web
    let server_handle = web::run(session_store.clone()).await?;
    
    // cli
    let mut line_editor = reedline::Reedline::create().with_ansi_colors(false);
    let prompt = ShellPrompt::default();

    info!("Starting cli...");
    println!("{LOGO}");
    loop {
        let sig = line_editor.read_line(&prompt);

        match sig {
            Ok(reedline::Signal::Success(buffer)) => {
                // TODO: REFACTOR
                if let Some(uuid) = active_session {
                    if buffer.as_str() == "quit" {
                        active_session = None;
                        continue
                    }
                    match session_store.write() {
                        Ok(session_store) => {
                            match session_store.start_command(uuid, buffer) {
                                Ok(_) => {
                                    
                                }
                                Err(err) => {
                                    error!("{err}");
                                }
                            }
                        }
                        Err(err) => {
                            error!("{err}");
                        }
                    }
                } else {
                    match parser::Parser::parse(Rule::command, buffer.as_str()) {
                        Ok(res) => {
                            let command = match session_store.read() {
                                Ok(session_store) => {
                                    match command::Command::from_pairs(res, &session_store, &alias_store) {
                                        Ok(command) => command,
                                        Err(err) => {
                                            println!("{err}");
                                            continue
                                        }
                                    }
                                }
                                Err(err) => {
                                    error!("{err}");
                                    continue
                                }
                            };
                            match session_store.write() {
                                Ok(mut session_store) => {
                                    if let Err(err) = command.execute(&mut session_store, &mut alias_store, &mut active_session) {
                                        error!("{err}");
                                        continue
                                    }
                                }
                                Err(err) => {
                                    error!("{err}");
                                    continue
                                }
                            }
                        }
                        Err(err) => {
                            println!("Unknown command or error at:");
                            println!("   $> {buffer}");
                            match err.location {
                                InputLocation::Pos(start) => {
                                    println!("      {:>amount$}^", " ", amount = start)
                                }
                                InputLocation::Span((start, end)) => {
                                    println!("      {:>amount$}{:>count$}", " ", "^", amount=start, count=(end-start))
                                }
                            }
                        }
                    }
                }
            }
            Ok(reedline::Signal::CtrlD) | Ok(reedline::Signal::CtrlC) => {
                info!("Stopping...");
                server_handle.stop(false).await;
                break;
            }
            _ => {}
        }
    }
    Ok(())
}

#[derive(Default)]
struct ShellPrompt;

impl Prompt for ShellPrompt {
    fn render_prompt_left(&self) -> Cow<str> {
        "inside-job".to_owned().into()
    }

    fn render_prompt_right(&self) -> Cow<str> {
        "".to_owned().into()
    }

    fn render_prompt_indicator(&self, prompt_mode: PromptEditMode) -> Cow<str> {
        match prompt_mode {
            PromptEditMode::Default | PromptEditMode::Emacs => "> ".into(),
            PromptEditMode::Vi(vi_mode) => match vi_mode {
                reedline::PromptViMode::Normal => "> ".into(),
                reedline::PromptViMode::Insert => ": ".into(),
            },
            PromptEditMode::Custom(str) => format!("({})", str).into(),
        }
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<str> {
        ">>> ".to_owned().into()
    }

    fn render_prompt_history_search_indicator(&self, history_search: PromptHistorySearch) -> Cow<str> {
        let prefix = match history_search.status {
            reedline::PromptHistorySearchStatus::Passing => "",
            reedline::PromptHistorySearchStatus::Failing => "failing ",
        };
        format!(
            "({}reverse-search: {}) ",
            prefix, history_search.term
        ).into()
    }
}

struct SessionPrompt(uuid::Uuid);

impl Prompt for SessionPrompt {
    fn render_prompt_left(&self) -> Cow<str> {
        self.0.to_string().into()
    }

    fn render_prompt_right(&self) -> Cow<str> {
        "".to_owned().into()
    }

    fn render_prompt_indicator(&self, prompt_mode: PromptEditMode) -> Cow<str> {
        match prompt_mode {
            PromptEditMode::Default | PromptEditMode::Emacs => "> ".into(),
            PromptEditMode::Vi(vi_mode) => match vi_mode {
                reedline::PromptViMode::Normal => "> ".into(),
                reedline::PromptViMode::Insert => ": ".into(),
            },
            PromptEditMode::Custom(str) => format!("({})", str).into(),
        }
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<str> {
        ">>> ".to_owned().into()
    }

    fn render_prompt_history_search_indicator(&self, history_search: PromptHistorySearch) -> Cow<str> {
        let prefix = match history_search.status {
            reedline::PromptHistorySearchStatus::Passing => "",
            reedline::PromptHistorySearchStatus::Failing => "failing ",
        };
        format!(
            "({}reverse-search: {}) ",
            prefix, history_search.term
        ).into()
    }
}