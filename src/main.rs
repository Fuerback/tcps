#[macro_use]
extern crate log;

use actix_web::{get, middleware::Logger, put, web, App, HttpResponse, HttpServer, Responder};
use crossbeam_channel::{unbounded, Sender};
use env_logger::Env;
use std::net::TcpListener;
use std::thread;

enum Action {
    Start(TcpListener),
}

struct Command {
    port: u16,
    action: Action,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(Env::default().default_filter_or("info"));
    let (tx, rx) = unbounded();
    info!("Bootstrapping the application");

    thread::spawn(move || loop {
        let command: Command = rx.recv().unwrap();
        match command.action {
            Action::Start(l) => {
                let p = command.port;
                thread::spawn(move || {
                    for stream in l.incoming() {
                        stream.unwrap();
                        //let stream = stream.unwrap();

                        //handle_connection(stream);
                    }
                });
                // Add listening into servers vectors
                info!("Server added at 127.0.0.1:{p}");
            }
        }
    });

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(tx.clone()))
            .wrap(Logger::default())
            .wrap(Logger::new("%a %{User-Agent}i"))
            .service(status)
            .service(start)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}

#[get("/status")]
async fn status() -> impl Responder {
    HttpResponse::Ok().body("OK")
}

#[put("/servers/{port}")]
async fn start(data: web::Data<Sender<Command>>, path: web::Path<u16>) -> impl Responder {
    let tx = data;
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
    tx.send(Command {
        port: port,
        action: Action::Start(listener),
    })
    .unwrap();

    HttpResponse::Ok().body("Server is running")
}
