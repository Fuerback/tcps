use bus::{Bus, BusReader};
use crossbeam_channel::{Receiver, Sender};
use std::{
    io,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

#[derive(Debug)]
pub struct Command {
    port: u16,
    action: Action,
}

#[derive(Debug)]
enum Action {
    Start(TcpListener),
    Close,
}

pub async fn close(port: u16, tx: Sender<Command>) {
    info!("Closing server at 127.0.0.1:{port}");

    tx.send(Command {
        port,
        action: Action::Close,
    })
    .unwrap();
}

pub fn bind(port: u16, tx: Sender<Command>) -> Result<bool, String> {
    let addr = format!("127.0.0.1:{port}");
    info!("Starting TCP server at {addr}");

    let listener = TcpListener::bind(&addr);
    if let Err(e) = listener {
        let err_msg = format!("Cannot start a new TCP server at {addr}. Error: {e}");
        error!("{err_msg}");
        return Err(err_msg);
    }

    let listener = listener.unwrap();
    tx.send(Command {
        port,
        action: Action::Start(listener),
    })
    .unwrap();

    Ok(true)
}

pub fn bootstrap(
    rx: Receiver<Command>,
    bus_close: Arc<Mutex<Bus<u16>>>,
    servers: Arc<Mutex<Vec<u16>>>,
) {
    thread::spawn(move || loop {
        // TODO: Handle this error later.
        let command: Command = rx.recv().expect("Cannot extract Command");

        let servers = servers.clone();
        let bus_close = bus_close.clone();
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
                                // TODO: Check if should be an error. It could be a
                                // Option<Interrupt>
                                if let Err(err_msg) = handle_connection(p, s, stream_close_rx) {
                                    servers.lock().unwrap().retain(|&x| x != p);
                                    error!("{err_msg}");
                                    break;
                                }
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
                    let mut bus = bus_close.lock().unwrap();
                    bus.broadcast(p);
                }
            }
        }
    });
}

fn handle_connection(
    port: u16,
    mut stream: TcpStream,
    mut close_rx: BusReader<u16>,
) -> Result<(), String> {
    let addr = format!("127.0.0.1:{port}");
    loop {
        let mut read = [0; 100];
        match stream.read(&mut read) {
            Ok(n) => {
                if n == 0 {
                    info!("No further bytes to read. TCP connection was closed at {addr}.");
                    return Ok(());
                }
                let is_probe = read[6] == 0xE2;
                // TODO: Implement EBCDIC library here.
                if is_probe {
                    // Cleaning the buffer
                    let mut resp = vec![];
                    let filler = [0; 94];
                    //let probe_payload: [u8; 5] = [0xD7, 0xD9, 0xD6, 0xC2, 0xC5];
                    let filler_payload: [u8; 5] = [0xD9, 0xD9, 0xD6, 0xC2, 0xC5];
                    resp.extend_from_slice(&read[0..6]);
                    resp.extend_from_slice(&filler);
                    //resp.extend_from_slice(&probe_payload);
                    resp.extend_from_slice(&filler_payload);
                    let mut clear = [0; 5];
                    stream.read(&mut clear).unwrap();

                    info!("Probe message detected. Just echoing...");
                    stream.write_all(&resp).unwrap();
                    continue;
                }
                error!("Message type not supported yet.");

                // TODO: Handle stream here. Echo probe and discard non-probe.
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                if let Ok(close_port) = close_rx.recv_timeout(Duration::from_millis(10)) {
                    if close_port == port {
                        return Err(format!(
                            "Command Close has been received. Closing stream at {addr}."
                        ));
                    }
                }
                continue;
            }
            Err(e) => {
                // TODO: Should not stop the listener.
                return Err(format!(
                    "Encountered IO error reading the stream at {addr}. Error: {e}"
                ));
            }
        }
    }
}
