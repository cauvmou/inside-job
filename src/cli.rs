use std::{time::SystemTime, io::Write};

use uuid::Uuid;

use crate::{APPLICATION};

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
    let stdin = std::io::stdin();
    let mut active_session: Option<Uuid> = None;
    loop {
        let mut buffer = "".to_string();
        if let Some(uuid) = active_session {
            if let Ok(application) = APPLICATION.lock() {
                if let Ok(mut sessions) = application.sessions.lock() {
                    if let Some(session) = sessions.get_mut(&uuid) {
                        print!("PS {}>  ", session.pwd);
                        std::io::stdout().flush().expect("Failed to flush stdout buffer.");
                        let _ = stdin.read_line(&mut buffer);
                        buffer.remove(buffer.len()-1);
                        match buffer.as_str() {
                            "jexit" => {
                                active_session = None
                            },
                            _ => {
                                session.current_input = (buffer, true);
                            }
                        }
                        
                    }
                    drop(sessions)
                }
                drop(application);
                std::thread::sleep(std::time::Duration::from_secs_f64(0.5));
            }
            
            continue;
        }
        print!("▶  ");
        std::io::stdout().flush().expect("Failed to flush stdout buffer.");
        let _ = stdin.read_line(&mut buffer);
        buffer = buffer.trim().to_string();
        match *buffer.split(" ").collect::<Vec<&str>>().get(0).unwrap() {
            "show" => show_sessions(),
            "connect" => {
                if let Some(options) = buffer.split_once(" ") {
                    if let Ok(uuid) = Uuid::parse_str(options.1) {
                        if let Ok(application) = APPLICATION.lock() {
                            if let Ok(sessions) = application.sessions.lock() {
                                if let Some(_) = sessions.get(&uuid) {
                                    active_session = Some(uuid)
                                }
                            }
                        }
                        else { println!("No session with UUID found.") }
                    }
                    else { println!("Invalid UUID.") }
                }
                else { println!("No UUID specified.") }
            },
            "" => {},
            _ => println!("Unknown command: {:?}", buffer)
        }
    }
}

fn show_sessions() {
    if let Ok(application) = APPLICATION.lock() {
        if let Ok(sessions) = application.sessions.lock() {
            for (k, session) in sessions.iter() {
                println!("{:20} | {:40} | {:10}", k.to_string(), session.user, format!("{:?}", session.state))
            }
        }
    }
}