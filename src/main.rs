#[macro_use]
extern crate log;

mod tcp;

use actix_web::{
    delete, get, middleware::Logger, put, web, App, HttpResponse, HttpServer, Responder,
};
use bus::Bus;
use crossbeam_channel::{unbounded, Sender};
use env_logger::Env;
use std::{
    net::TcpListener,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
struct AppData {
    servers: Arc<Mutex<Vec<u16>>>,
    tx: Sender<tcp::Command>,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(Env::default().default_filter_or("debug"));
    let (tx, rx) = unbounded();
    // TODO: Change it, bus capacity must be a variable.
    let bus_close = Arc::new(Mutex::new(Bus::<u16>::new(100)));
    let servers = Arc::new(Mutex::new(Vec::new()));
    info!("Bootstrapping the application");

    tcp::bootstrap(rx, bus_close.clone(), servers.clone());

    let app_data = AppData {
        servers,
        tx: tx.clone(),
    };

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(app_data.clone()))
            .wrap(Logger::default())
            .wrap(Logger::new("%a %{User-Agent}i"))
            .service(status)
            .service(get_servers)
            .service(start)
            .service(close)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}

#[get("/status")]
async fn status() -> impl Responder {
    HttpResponse::Ok().body("OK")
}

#[get("/servers")]
async fn get_servers(data: web::Data<AppData>) -> impl Responder {
    // TODO: Handle it properly later.
    let servers = &data.servers.lock().unwrap();
    HttpResponse::Ok().body(format!("Servers: {:?}", servers))
}

#[put("/server/{port}")]
async fn start(data: web::Data<AppData>, path: web::Path<u16>) -> impl Responder {
    let tx = data.tx.clone();
    let port = path.into_inner();
    let addr = format!("127.0.0.1:{port}");
    info!("Starting TCP server at {addr}");
    let listener = TcpListener::bind(&addr);
    if let Err(e) = listener {
        let err_msg = format!("Cannot start a new TCP server at {addr}. Error: {e}");
        error!("{err_msg}");
        return HttpResponse::InternalServerError().body(err_msg);
    }

    let listener = listener.unwrap();
    tx.send(tcp::Command {
        port,
        action: tcp::Action::Start(listener),
    })
    .unwrap();

    HttpResponse::Ok().body("Server is running")
}

#[delete("/server/{port}")]
async fn close(data: web::Data<AppData>, path: web::Path<u16>) -> impl Responder {
    let tx = data.tx.clone();
    let port = path.into_inner();
    info!("Closing server at 127.0.0.1:{port}");

    tx.send(tcp::Command {
        port,
        action: tcp::Action::Close,
    })
    .unwrap();

    HttpResponse::Accepted().body("Close command has been sent.")
}
