use actix_web::{delete, get, put, web, HttpResponse, Responder};

use crate::tcp;

#[get("/status")]
async fn status() -> impl Responder {
    HttpResponse::Ok().body("OK")
}

#[get("/servers")]
async fn get_servers(data: web::Data<crate::AppData>) -> impl Responder {
    // TODO: Handle it properly later.
    let servers = &data.servers.lock().unwrap();
    HttpResponse::Ok().body(format!("Servers: {:?}", servers))
}

#[put("/server/{port}")]
async fn start(data: web::Data<crate::AppData>, path: web::Path<u16>) -> impl Responder {
    let port = path.into_inner();
    let tx = data.tx.clone();

    match tcp::bind(port, tx) {
        Ok(_) => HttpResponse::Ok().body("Server is running."),
        Err(msg) => HttpResponse::InternalServerError().body(msg),
    }
}

#[delete("/server/{port}")]
async fn close(data: web::Data<crate::AppData>, path: web::Path<u16>) -> impl Responder {
    let port = path.into_inner();
    let tx = data.tx.clone();

    tcp::close(port, tx).await;

    HttpResponse::Accepted().body("Close command has been sent.")
}
