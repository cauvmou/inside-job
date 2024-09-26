mod session;
mod storage;
mod web;

use std::borrow::Cow;
use std::collections::HashMap;
use std::string::FromUtf8Error;
use std::sync::{Arc, LockResult, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::thread;
use std::thread::spawn;
use clap::Parser as ClapParser;
use pest::Parser;
use log::{error, info, trace, warn};
use pest::error::Error;
use pest::iterators::Pairs;
use crate::parser::Rule;
use crate::session::{Command, Session, SessionData};
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

    // start web
    let server_handle = web::run(session_store.clone()).await?;

    let mut line_editor = reedline::Reedline::create();
    let prompt = reedline::DefaultPrompt::default();

    info!("Starting cli...");
    loop {
        let sig = line_editor.read_line(&prompt);

        match sig {
            Ok(reedline::Signal::Success(buffer)) => {
                match parser::Parser::parse(Rule::command, buffer.as_str()) {
                    Ok(res) => {
                        println!("Parsed uuid: {res:?}");
                    }
                    Err(err) => {

                        println!("Failed to parse uuid: {err:?}");
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