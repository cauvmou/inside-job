mod session;
mod storage;
mod web;
mod command;

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, LockResult, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::thread;
use std::time::Duration;
use actix_web::rt::time::sleep;
use clap::Parser as ClapParser;
use pest::Parser;
use log::{error, info, trace, warn, Level, Log, Metadata, Record};
use pest::error::{Error, InputLocation};
use uuid::Uuid;
use crate::parser::Rule;
use crate::storage::{LockState, SessionStore};
use colored::Colorize;

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
    // #[arg(short, long, default_value_t = log::LevelFilter::Info)]
    // loglevel: log::LevelFilter,
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

pub struct ExternalPrinterLogger(reedline::ExternalPrinter<String>);

impl Log for ExternalPrinterLogger {
    fn enabled(&self, _: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        self.0.print(format!("{}", record.args()).to_string()).unwrap();
    }

    fn flush(&self) {}
}

fn level_to_color(level: log::Level) -> colored::ColoredString {
    match level {
        Level::Error => level.to_string().red(),
        Level::Warn => level.to_string().yellow(),
        Level::Info => level.to_string().blue(),
        Level::Debug => level.to_string().green(),
        Level::Trace => level.to_string().purple(),
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();

    let printer = reedline::ExternalPrinter::new(u16::MAX as usize);
    let logger: Box<dyn Log> = Box::new(ExternalPrinterLogger(printer.clone()));
    let mut line_editor = reedline::Reedline::create().with_ansi_colors(true).with_external_printer(printer);

    // init logger
    println!("{LOGO}");
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{}] {}",
                level_to_color(record.level()),
                message,
            ))
        })
        .chain(fern::Dispatch::new().filter(|metadata| metadata.target().starts_with("insidejob")).level(log::LevelFilter::Info).chain(logger))
        .apply().expect("logger initialization failed");
    
    // create store
    let session_store = Arc::new(RwLock::new(SessionStore::default()));
    let mut alias_store: HashMap<String, u128> = HashMap::new();
    let mut active_session: Option<u128> = None;

    // start web
    let server_handle = web::run(session_store.clone()).await?;

    // cli
    info!("Starting cli...");
    loop {
        let prompt: Box<dyn reedline::Prompt> = match active_session {
            Some(uuid) => Box::new(SessionPrompt(Uuid::from_u128(uuid))),
            None => Box::new(ShellPrompt)
        };

        let sig = line_editor.read_line(prompt.as_ref());

        match sig {
            Ok(reedline::Signal::Success(buffer)) => {
                // TODO: REFACTOR
                if buffer.is_empty() {
                    continue;
                }
                if let Some(uuid) = active_session {
                    if buffer.as_str() == "quit" {
                        active_session = None;
                        continue;
                    }
                    let result = if let Ok(mut session_store) = session_store.write() {
                        session_store.start_command(uuid, buffer).ok()
                    } else { None };
                    if let Some(()) = result {
                        loop {
                            sleep(Duration::from_millis(10)).await;
                            if let Ok(session_store) = session_store.read() {
                                if let Some(Some(LockState::ToReceive(output))) = session_store.session_lock.get(&uuid) {
                                    if !output.ends_with("\n") {
                                        println!("{output}");
                                    } else {
                                        print!("{output}");
                                    }
                                    break;
                                }
                            }
                        }
                        if let Ok(mut session_store) = session_store.write() {
                            session_store.session_lock.insert(uuid, None);
                        }
                    }
                } else {
                    match parser::Parser::parse(Rule::command, buffer.as_str()) {
                        Ok(res) => {
                            let command = if let Ok(session_store) = session_store.read() {
                                match command::Command::from_pairs(res, &session_store, &alias_store) {
                                    Ok(command) => command,
                                    Err(err) => {
                                        println!("{err}");
                                        continue;
                                    }
                                }
                            } else {
                                continue
                            };

                            if let Ok(mut session_store) = session_store.write() {
                                if let Err(err) = command.execute(&mut session_store, &mut alias_store, &mut active_session) {
                                    error!("{err}");
                                }
                            }
                        }
                        Err(err) => {
                            println!("Unknown command or error at:");
                            println!("   $> {buffer}");
                            match err.location {
                                InputLocation::Pos(start) => {
                                    println!("      {:>amount$}^", "", amount = start)
                                }
                                InputLocation::Span((start, end)) => {
                                    println!("      {:>amount$}{:>count$}", "", "^", amount = start, count = (end - start))
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
            other => {
                info!("Unhandled {:?}", other);
            }
        }
    }
    Ok(())
}

#[derive(Default)]
struct ShellPrompt;

impl reedline::Prompt for ShellPrompt {
    fn render_prompt_left(&self) -> Cow<str> {
        "inside-job".to_owned().into()
    }

    fn render_prompt_right(&self) -> Cow<str> {
        "".to_owned().into()
    }

    fn render_prompt_indicator(&self, prompt_mode: reedline::PromptEditMode) -> Cow<str> {
        match prompt_mode {
            reedline::PromptEditMode::Default | reedline::PromptEditMode::Emacs => "> ".into(),
            reedline::PromptEditMode::Vi(vi_mode) => match vi_mode {
                reedline::PromptViMode::Normal => "> ".into(),
                reedline::PromptViMode::Insert => ": ".into(),
            },
            reedline::PromptEditMode::Custom(str) => format!("({})", str).into(),
        }
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<str> {
        ">>> ".to_owned().into()
    }

    fn render_prompt_history_search_indicator(&self, history_search: reedline::PromptHistorySearch) -> Cow<str> {
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

impl reedline::Prompt for SessionPrompt {
    fn render_prompt_left(&self) -> Cow<str> {
        self.0.to_string().into()
    }

    fn render_prompt_right(&self) -> Cow<str> {
        "".to_owned().into()
    }

    fn render_prompt_indicator(&self, prompt_mode: reedline::PromptEditMode) -> Cow<str> {
        match prompt_mode {
            reedline::PromptEditMode::Default | reedline::PromptEditMode::Emacs => "> ".into(),
            reedline::PromptEditMode::Vi(vi_mode) => match vi_mode {
                reedline::PromptViMode::Normal => "> ".into(),
                reedline::PromptViMode::Insert => ": ".into(),
            },
            reedline::PromptEditMode::Custom(str) => format!("({})", str).into(),
        }
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<str> {
        ">>> ".to_owned().into()
    }

    fn render_prompt_history_search_indicator(&self, history_search: reedline::PromptHistorySearch) -> Cow<str> {
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