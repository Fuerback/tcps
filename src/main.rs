#[macro_use]
extern crate log;

use actix_web::{
    delete, get, middleware::Logger, put, web, App, HttpResponse, HttpServer, Responder,
};
use bus::{Bus, BusReader};
use crossbeam_channel::{unbounded, Sender};
use env_logger::Env;
use std::{
    io,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

#[derive(Debug)]
enum Action {
    Start(TcpListener),
    Close,
}

#[derive(Debug)]
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
    env_logger::init_from_env(Env::default().default_filter_or("debug"));
    let (tx, rx) = unbounded();
    // TODO: Change it, bus capacity must be a variable.
    let bus_close = Arc::new(Mutex::new(Bus::<u16>::new(100)));
    let servers = Arc::new(Mutex::new(Vec::new()));
    let serv_clon = servers.clone();
    let bus_close_clone = bus_close.clone();
    info!("Bootstrapping the application");

    thread::spawn(move || loop {
        // TODO: Handle this error later.
        let command: Command = rx.recv().expect("Cannot extract Command");

        let servers = serv_clon.clone();
        let bus_close = bus_close_clone.clone();
        match command.action {
            Action::Start(l) => {
                let p = command.port;
                let addr = format!("127.0.0.1:{p}");
                let mut acpt_close_rx = bus_close.lock().unwrap().add_rx();
                // TODO: Handle this error later.
                l.set_nonblocking(true).expect("error on set non-blocking");

                thread::spawn(move || {
                    for stream in l.incoming() {
                        {
                            // TODO: Handle it later properly.
                            let mut v = servers.lock().unwrap();
                            if !v.contains(&p) {
                                info!("Server added at {addr}");
                                v.push(p);
                            }
                        }

                        let stream_close_rx = bus_close.lock().unwrap().add_rx();
                        match stream {
                            Ok(s) => {
                                handle_connection(p, s, stream_close_rx, servers.clone());
                            }
                            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                                if let Ok(close_port) =
                                    acpt_close_rx.recv_timeout(Duration::from_millis(10))
                                {
                                    if close_port == p {
                                        info!("Command Close has been received. Closing connection at {addr}.");
                                        servers.lock().unwrap().retain(|&x| x != p);
                                        break;
                                    }
                                }
                                continue;
                            }
                            Err(e) => {
                                servers.lock().unwrap().retain(|&x| x != p);
                                error!("Encountered IO error while accepting connection at {addr}. Error: {e}");
                                break;
                            }
                        }
                    }
                });
            }
            Action::Close => {
                let p = command.port;
                info!("Broadcasting close command for port: {p}");
                {
                    let mut bus = bus_close_clone.lock().unwrap();
                    bus.broadcast(p);
                }
            }
        }
    });

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

fn handle_connection(
    port: u16,
    mut stream: TcpStream,
    mut close_rx: BusReader<u16>,
    servers: Arc<Mutex<Vec<u16>>>,
) {
    let addr = format!("127.0.0.1:{port}");
    loop {
        let mut read = [0; 5];
        match stream.read(&mut read) {
            Ok(n) => {
                if n == 0 {
                    info!("No further bytes to read. TCP connection was closed at {addr}.");
                    break;
                }
                // TODO: Handle stream here. Echo probe and discard non-probe.
                stream.write_all(&read[0..n]).unwrap();
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                if let Ok(close_port) = close_rx.recv_timeout(Duration::from_millis(10)) {
                    if close_port == port {
                        info!("Command Close has been received. Closing stream at {addr}.");
                        servers.lock().unwrap().retain(|&x| x != port);
                        break;
                    }
                }
                continue;
            }
            Err(e) => {
                servers.lock().unwrap().retain(|&x| x != port);
                error!("Encountered IO error reading the stream at {addr}. Error: {e}");
                break;
            }
        }
    }
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
    tx.send(Command {
        port,
        action: Action::Start(listener),
    })
    .unwrap();

    HttpResponse::Ok().body("Server is running")
}

#[delete("/server/{port}")]
async fn close(data: web::Data<AppData>, path: web::Path<u16>) -> impl Responder {
    let tx = data.tx.clone();
    let port = path.into_inner();
    info!("Closing server at 127.0.0.1:{port}");

    tx.send(Command {
        port,
        action: Action::Close,
    })
    .unwrap();

    HttpResponse::Accepted().body("Close command has been sent.")
}
