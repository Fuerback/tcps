#[macro_use]
extern crate log;

use actix_web::{get, middleware::Logger, put, web, App, HttpResponse, HttpServer, Responder};
use bus::Bus;
use crossbeam_channel::{unbounded, Sender};
use env_logger::Env;
use std::{
    io,
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

enum Action {
    Start(TcpListener),
}

struct Command {
    port: u16,
    action: Action,
}

#[derive(Clone)]
struct AppData {
    servers: Arc<Mutex<Vec<u16>>>,
    tx: Sender<Command>,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(Env::default().default_filter_or("info"));
    let (tx, rx) = unbounded();
    let mut bus_close = Bus::<u16>::new(100);
    let servers = Arc::new(Mutex::new(Vec::new()));
    let cloned_servers = servers.clone();
    info!("Bootstrapping the application");

    thread::spawn(move || loop {
        // TODO: Handle this error later.
        let command: Command = rx.recv().unwrap();
        match command.action {
            Action::Start(l) => {
                let p = command.port;
                let addr = format!("127.0.0.1:{p}");
                let mut close_rx = bus_close.add_rx();
                // TODO: Handle this error later.
                info!("Setting nonblocking for server ");
                l.set_nonblocking(true).expect("error on set non-blocking");
                info!("Calling incoming...");

                for stream in l.incoming() {
                    {
                        // TODO: Handle it later properly.
                        let mut v = cloned_servers.lock().unwrap();
                        if !v.contains(&p) {
                            info!("Server added at {addr}");
                            v.push(p);
                        }
                    }

                    match stream {
                        Ok(_s) => {
                            // do something with the TcpStream
                            //handle_connection(s);
                        }
                        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                            if let Ok(close_port) =
                                close_rx.recv_timeout(Duration::from_millis(100))
                            {
                                if close_port == p {
                                    info!("Closing server at 127.0.0.1:{p}");
                                    cloned_servers.lock().unwrap().remove(p.into());
                                    break;
                                }
                            }
                            continue;
                        }
                        Err(e) => {
                            // TODO: Handle this error later.
                            info!("Server removed at {addr}");
                            cloned_servers.lock().unwrap().retain(|&x| x != p);
                            panic!("encountered IO error: {e}");
                        }
                    }
                }
            }
        }
    });

    let app_data = AppData {
        servers: servers,
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

#[put("/servers/{port}")]
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
    tx.send(Command {
        port: port,
        action: Action::Start(listener),
    })
    .unwrap();

    HttpResponse::Ok().body("Server is running")
}
