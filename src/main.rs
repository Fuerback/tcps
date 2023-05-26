#[macro_use]
extern crate log;

use actix_web::{get, middleware::Logger, web, App, HttpResponse, HttpServer, Responder};
use env_logger::Env;
use std::net::TcpListener;
use std::thread;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(Env::default().default_filter_or("info"));
    info!("Bootstrapping the application");

    HttpServer::new(|| {
        App::new()
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

#[get("/start/{port}")]
async fn start(path: web::Path<String>) -> impl Responder {
    let port = path.into_inner();
    let listener = TcpListener::bind(format!("127.0.0.1:{port}")).unwrap();

    thread::spawn(move || {
        for stream in listener.incoming() {
            stream.unwrap();
            //let stream = stream.unwrap();

            //handle_connection(stream);
        }
    });

    HttpResponse::Ok().body("Server is running")
}
