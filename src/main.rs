use actix_multipart::Multipart;
use actix_web::{get, post, web, App, HttpServer, Responder};
use askama::Template;
use futures::{StreamExt, TryStreamExt};
use rodio::{Decoder, OutputStream, Sink, Source};
use std::{
    io::{BufReader, Cursor},
    sync::{Arc, Mutex},
};

struct AppState {
    queue: Mutex<Vec<String>>,
    sink: Arc<Mutex<Sink>>,
}

#[derive(Template)]
#[template(path = "index.html", escape = "none")]
struct IndexTemplate {
    queue: QueueTemplate,
    speed: f32,
    status: String,
}

#[derive(Template)]
#[template(path = "queue.html")]
struct QueueTemplate {
    queue: Vec<String>,
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
    data.sink.lock().unwrap().skip_one();

    let q = data.queue.lock().unwrap();

    render_queue(q[q.len() - l..].to_vec())
}

#[get("/speed/{speed}")]
async fn speed(data: web::Data<AppState>, speed: web::Path<f32>) -> impl Responder {
    data.sink.lock().unwrap().set_speed(*speed);

    data.clone().sink.lock().unwrap().speed().to_string()
}

#[get("/queue")]
async fn queue(data: web::Data<AppState>) -> impl Responder {
    get_queue(data)
}

#[get("/")]
async fn index(data: web::Data<AppState>) -> impl Responder {
    let status = if data.clone().sink.lock().unwrap().is_paused() {
        "Paused".to_string()
    } else {
        "Playing".to_string()
    };

    IndexTemplate {
        queue: get_queue(data.clone()),
        speed: data.clone().sink.lock().unwrap().speed(),
        status,
    }
}
#[post("/upload")]
async fn upload(mut payload: Multipart, data: web::Data<AppState>) -> impl Responder {
    let mut buffer = Vec::new();
    let mut title = String::new();

    while let Ok(Some(mut field)) = payload.try_next().await {
        let content_disposition = field.content_disposition().clone();
        let filename = content_disposition
            .get_filename()
            .expect("No filename found");

        while let Some(chunk) = field.next().await {
            let data = chunk.unwrap();
            buffer.extend_from_slice(&data);
        }

        title = filename.to_string();
    }

    let audio_source = Cursor::new(buffer);
    let decoder = Decoder::new(BufReader::new(audio_source)).unwrap();
    let source = decoder.convert_samples::<f32>();

    data.queue.lock().unwrap().push(title);
    data.sink.lock().unwrap().append(source);

    get_queue(data)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let (_stream, stream_handle) = OutputStream::try_default().unwrap();

    let sink = Sink::try_new(&stream_handle).unwrap();

    let snk = Arc::new(Mutex::new(sink));
    let song_queue = Vec::new();

    let data = web::Data::new(AppState {
        queue: Mutex::new(song_queue.clone()),
        sink: Arc::clone(&snk),
    });

    HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .service(index)
            .service(play)
            .service(pause)
            .service(speed)
            .service(upload)
            .service(queue)
            .service(stop)
            .service(skip)
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
