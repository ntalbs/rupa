use actix_files::NamedFile;
use actix_web::{
    body::BoxBody, http::StatusCode, route, web::Data, App, HttpRequest, HttpResponse, HttpServer,
    Responder,
};
use clap::Parser;
use env_logger::Env;
use log::info;
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};

#[derive(Default, Debug, Parser)]
#[clap(version, about = "A simple command-line static http server")]
struct Arguments {
    #[clap(short, long, default_value_t = 3000)]
    port: u16,
    #[clap(short, long, default_value = ".")]
    base: PathBuf,
    #[clap(short, long, default_value_t = 1)]
    workers: usize,
}

fn css() -> &'static str {
    r#"<style>
    body {
        font-family: monospace;
        font-size: 1.2rem;
        line-height: 1.2;
        margin: 1rem;
    }
    .path-header {
        padding: 5px 10px;
        color: #fff;
        background-color: #44f;
        border-radius: 5px;
    }
    a:hover {
        background-color: #ffa;
    }
    a:visited {
        color: blue;
    }
    </style>"#
}

fn files_in(dir: &PathBuf) -> io::Result<Vec<PathBuf>> {
    let mut files = vec![];

    for f in fs::read_dir(dir)? {
        let dir_entry = f?;
        files.push(dir_entry.path());
    }
    files.sort();

    Ok(files)
}

fn dir(base: &PathBuf, path: &PathBuf) -> io::Result<Vec<u8>> {
    let mut buf: Vec<u8> = Vec::new();
    buf.write_all(
        format!(
            "<html><head>{}</head><body><div class=\"path-header\">Path: {}</div><ol>",
            css(),
            &path.to_str().unwrap()
        )
        .as_bytes(),
    )?;

    if base != path {
        buf.write_all("<li><a href=\"..\">..</a></li>".as_bytes())?;
    }

    for f in files_in(path)? {
        if let (Ok(href), Some(name)) = (f.strip_prefix(base), f.file_name()) {
            let href = href.to_str().unwrap();
            let name = name.to_str().unwrap();
            buf.write_all(
                format!(
                    "<li><a href=\"/{}\">{}{}</li>",
                    href,
                    name,
                    if f.is_dir() { "/" } else { "" }
                )
                .as_bytes(),
            )?;
        }
    }

    buf.write_all(b"</ol></body><html>")?;
    Ok(buf)
}

#[route("/{_:.*}", method = "GET")]
async fn handle_get(req: HttpRequest, data: Data<PathBuf>) -> impl Responder {
    let base = data.get_ref();
    let mut actual_path = base.clone();

    if req.path() != "/" {
        actual_path = actual_path.join(&req.path()[1..]);
    }

    if !actual_path.exists() {
        info!("{} => {}", req.path(), actual_path.to_string_lossy());
        return HttpResponse::NotFound().body("Requested path does not exist.\n");
    }

    if actual_path.is_dir() {
        let index = actual_path.join("index.html");
        if index.exists() {
            info!("{} => {}", req.path(), index.to_string_lossy());
            let index = NamedFile::open_async(index).await.unwrap();
            index.into_response(&req)
        } else {
            info!(
                "{} => Listing {}",
                req.path(),
                actual_path.to_string_lossy()
            );
            HttpResponse::Ok()
                .insert_header(("Content-Type", "text/html"))
                .body(dir(base, &actual_path).unwrap())
        }
    } else {
        info!("{} => {}", req.path(), actual_path.to_string_lossy());
        let file = actix_files::NamedFile::open_async(actual_path)
            .await
            .unwrap();
        file.into_response(&req)
    }
}

#[route(
    "/{_:.*}",
    method = "POST",
    method = "PUT",
    method = "DELETE",
    method = "HEAD",
    method = "CONNECT",
    method = "OPTIONS",
    method = "TRACE",
    method = "PATCH"
)]
async fn handle_other(_: HttpRequest) -> impl Responder {
    HttpResponse::new(StatusCode::METHOD_NOT_ALLOWED).set_body(BoxBody::new("Method Not Allowed.\n"))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .format(|buf, record| {
            let level_style = buf.default_level_style(record.level());
            writeln!(
                buf,
                "{} {level_style}{}{level_style:#} {}",
                buf.timestamp(),
                record.level(),
                record.args()
            )
        })
        .init();

    let args = Arguments::parse();
    let port = args.port;
    let workers = args.workers;
    let base = args.base.canonicalize()?;

    info!("Starting server on port {port}");

    HttpServer::new(move || {
        App::new()
            .app_data(Data::new(base.clone()))
            .service(handle_get)
            .service(handle_other)
    })
    .bind(("0.0.0.0", port))?
    .workers(workers)
    .run()
    .await
}
