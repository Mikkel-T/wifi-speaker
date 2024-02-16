use actix_multipart::Multipart;
use actix_web::{get, post, web, App, HttpServer, Responder};
use askama::Template;
use futures::{StreamExt, TryStreamExt};
use rodio::{Decoder, OutputStream, Sink, Source};
use std::{
    io::{BufReader, Cursor},
    sync::Mutex,
};

struct AppState {
    queue: Mutex<Vec<String>>,
    sink: Mutex<Sink>,
}

#[derive(Template)]
#[template(path = "index.html", escape = "none")]
struct IndexTemplate {
    queue: QueueTemplate,
    speed: f32,
    volume: f32,
    status: String,
}

#[derive(Template)]
#[template(path = "queue.html")]
struct QueueTemplate {
    queue: Vec<String>,
}

#[get("/")]
async fn index(data: web::Data<AppState>) -> impl Responder {
    let status = if data.clone().sink.lock().unwrap().is_paused() {
        "Paused".to_string()
    } else {
        "Playing".to_string()
    };

    let vol = data.clone().sink.lock().unwrap().volume() * 100.0;
    let s = data.clone().sink.lock().unwrap().speed();

    IndexTemplate {
        volume: vol,
        speed: s,
        queue: get_queue(data.clone()),
        status,
    }
}

#[get("/play")]
async fn play(data: web::Data<AppState>) -> impl Responder {
    data.sink.lock().unwrap().play();

    "Playing"
}

#[get("/pause")]
async fn pause(data: web::Data<AppState>) -> impl Responder {
    data.sink.lock().unwrap().pause();

    "Paused"
}

#[get("/stop")]
async fn stop(data: web::Data<AppState>) -> impl Responder {
    data.sink.lock().unwrap().stop();

    render_queue(Vec::new())
}

#[get("/skip")]
async fn skip(data: web::Data<AppState>) -> impl Responder {
    let l = data.sink.lock().unwrap().len() - 1;
    let q = data.queue.lock().unwrap();
    data.sink.lock().unwrap().skip_one();

    render_queue(q[q.len() - l..].to_vec())
}

#[get("/speed/{speed}")]
async fn speed(data: web::Data<AppState>, speed: web::Path<f32>) -> impl Responder {
    data.sink.lock().unwrap().set_speed(*speed);

    data.clone().sink.lock().unwrap().speed().to_string()
}

#[get("/volume/{volume}")]
async fn volume(data: web::Data<AppState>, volume: web::Path<f32>) -> impl Responder {
    data.sink.lock().unwrap().set_volume(*volume);

    (data.clone().sink.lock().unwrap().volume() * 100.0)
        .round()
        .to_string()
}

#[post("/upload")]
async fn upload(mut payload: Multipart, data: web::Data<AppState>) -> impl Responder {
    while let Ok(Some(mut field)) = payload.try_next().await {
        let mut buffer = Vec::new();
        let content_disposition = field.content_disposition().clone();
        let filename = content_disposition
            .get_filename()
            .expect("No filename found");

        while let Some(chunk) = field.next().await {
            let data = chunk.unwrap();
            buffer.extend_from_slice(&data);
        }

        data.queue.lock().unwrap().push(filename.to_string());

        let audio_source = Cursor::new(buffer);
        let decoder = Decoder::new(BufReader::new(audio_source)).unwrap();
        let source = decoder.convert_samples::<f32>();

        data.sink.lock().unwrap().append(source);
    }

    get_queue(data)
}

#[get("/queue")]
async fn queue(data: web::Data<AppState>) -> impl Responder {
    get_queue(data)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let (_stream, stream_handle) = OutputStream::try_default().unwrap();
    let sink = Sink::try_new(&stream_handle).unwrap();
    let song_queue = Vec::new();

    let data = web::Data::new(AppState {
        queue: Mutex::new(song_queue.clone()),
        sink: Mutex::new(sink),
    });

    HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .service(index)
            .service(play)
            .service(pause)
            .service(stop)
            .service(skip)
            .service(volume)
            .service(speed)
            .service(upload)
            .service(queue)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}

fn get_queue(data: web::Data<AppState>) -> QueueTemplate {
    let q = data.queue.lock().unwrap();

    render_queue(q[q.len() - data.sink.lock().unwrap().len()..].to_vec())
}

fn render_queue(data: Vec<String>) -> QueueTemplate {
    QueueTemplate { queue: data }
}
