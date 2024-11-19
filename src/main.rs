mod command;
mod payload;
mod session;
mod storage;
mod web;

use crate::parser::Rule;
use crate::storage::{LockState, SessionStore};
use clap::Parser as ClapParser;
use colored::Colorize;
use log::{error, info, Level, Log, Metadata, Record};
use pest::error::InputLocation;
use pest::Parser;
use pollster::FutureExt;
use reedline::Color;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::thread;
use std::thread::spawn;
use std::time::Duration;
use uuid::Uuid;

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

pub struct ExternalPrinterLogger(reedline::ExternalPrinter<String>);

impl Log for ExternalPrinterLogger {
    fn enabled(&self, _: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        self.0
            .print(format!("{}", record.args()).to_string())
            .unwrap();
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
        .chain(
            fern::Dispatch::new()
                .filter(|metadata| metadata.target().starts_with("insidejob"))
                .level(args.loglevel)
                .chain(logger),
        )
        .apply()
        .expect("logger initialization failed");

    // create store
    let session_store = Arc::new(RwLock::new(SessionStore::default()));
    let mut alias_store: HashMap<String, u128> = HashMap::new();
    let mut active_session: Option<u128> = None;

    // start web

    // cli
    let server_handle = web::run(session_store.clone()).await?;
    let mut line_editor = reedline::Reedline::create()
        .with_ansi_colors(true)
        .with_external_printer(printer);
    loop {
        let prompt: Box<dyn reedline::Prompt> = match active_session {
            Some(uuid) => Box::new(SessionPrompt(Uuid::from_u128(uuid))),
            None => Box::new(ShellPrompt),
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
                    } else {
                        None
                    };
                    if let Some(()) = result {
                        let session_store = session_store.clone();
                        spawn(move || {
                            loop {
                                thread::sleep(Duration::from_millis(10));
                                if let Ok(session_store) = session_store.read() {
                                    if let Some(Some(LockState::ToReceive(output))) =
                                        session_store.session_lock.get(&uuid)
                                    {
                                        info!("{}: {output}", Uuid::from_u128(uuid));
                                        break;
                                    }
                                }
                            }
                            if let Ok(mut session_store) = session_store.write() {
                                session_store.session_lock.insert(uuid, None);
                            }
                        });
                    } else {
                        error!("Still waiting on previous command!")
                    }
                } else {
                    match parser::Parser::parse(Rule::command, buffer.as_str()) {
                        Ok(res) => {
                            let command = if let Ok(session_store) = session_store.read() {
                                match command::Command::from_pairs(
                                    res,
                                    &session_store,
                                    &alias_store,
                                ) {
                                    Ok(command) => command,
                                    Err(err) => {
                                        error!("{err}");
                                        continue;
                                    }
                                }
                            } else {
                                continue;
                            };

                            if let Ok(mut session_store) = session_store.write() {
                                match command
                                    .execute(
                                        &mut session_store,
                                        &mut alias_store,
                                        &mut active_session,
                                    )
                                    .await
                                {
                                    Ok(should_quit) if should_quit => break,
                                    Err(err) => {
                                        error!("{err}");
                                    }
                                    _ => {}
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
                                    println!(
                                        "      {:>amount$}{:>count$}",
                                        "",
                                        "^",
                                        amount = start,
                                        count = (end - start)
                                    )
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
        "[ inside-job ]".to_owned().into()
    }

    fn render_prompt_right(&self) -> Cow<str> {
        "".to_owned().into()
    }

    fn render_prompt_indicator(&self, prompt_mode: reedline::PromptEditMode) -> Cow<str> {
        match prompt_mode {
            reedline::PromptEditMode::Default | reedline::PromptEditMode::Emacs => ": ".into(),
            _ => "> ".into(),
        }
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<str> {
        ">>> ".to_owned().into()
    }

    fn render_prompt_history_search_indicator(
        &self,
        history_search: reedline::PromptHistorySearch,
    ) -> Cow<str> {
        let prefix = match history_search.status {
            reedline::PromptHistorySearchStatus::Passing => "",
            reedline::PromptHistorySearchStatus::Failing => "failing ",
        };
        format!("({}reverse-search: {}) ", prefix, history_search.term).into()
    }

    fn get_prompt_color(&self) -> Color {
        Color::Green
    }

    fn get_indicator_color(&self) -> Color {
        Color::Green
    }
}

struct SessionPrompt(uuid::Uuid);

impl reedline::Prompt for SessionPrompt {
    fn render_prompt_left(&self) -> Cow<str> {
        format!("[ {} ]", self.0).into()
    }

    fn render_prompt_right(&self) -> Cow<str> {
        "".to_owned().into()
    }

    fn render_prompt_indicator(&self, prompt_mode: reedline::PromptEditMode) -> Cow<str> {
        match prompt_mode {
            reedline::PromptEditMode::Default | reedline::PromptEditMode::Emacs => "$ ".into(),
            _ => "> ".into(),
        }
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<str> {
        ">>> ".to_owned().into()
    }

    fn render_prompt_history_search_indicator(
        &self,
        history_search: reedline::PromptHistorySearch,
    ) -> Cow<str> {
        let prefix = match history_search.status {
            reedline::PromptHistorySearchStatus::Passing => "",
            reedline::PromptHistorySearchStatus::Failing => "failing ",
        };
        format!("({}reverse-search: {}) ", prefix, history_search.term).into()
    }

    fn get_prompt_color(&self) -> Color {
        Color::Cyan
    }

    fn get_indicator_color(&self) -> Color {
        Color::Cyan
    }
}
