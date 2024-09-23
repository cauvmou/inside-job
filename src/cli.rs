use std::process::Command;
use std::sync::{Arc, RwLock};
use clap::{command, Parser, Subcommand};
use log::info;
use crate::storage::SessionStore;


pub fn run(session_store: Arc<RwLock<SessionStore>>) {
    let mut line_editor = reedline::Reedline::create();
    let prompt = reedline::DefaultPrompt::default();
    
    info!("Starting cli...");
    loop {
        let sig = line_editor.read_line(&prompt);
        match sig {
            Ok(reedline::Signal::Success(buffer)) => {
                println!("We processed: {}", buffer);
            }
            Ok(reedline::Signal::CtrlD) | Ok(reedline::Signal::CtrlC) => {
                println!("\nAborted!");
                break;
            }
            x => {
                println!("Event: {:?}", x);
            }
        }
    }
}