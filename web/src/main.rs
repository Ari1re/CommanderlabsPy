//! Commander Lab — browser edition.
//!
//! Serves the same UI as the desktop/Android app on http://127.0.0.1:7878 and
//! opens your browser. Useful on systems without a webview (NixOS without
//! webkit, servers) and for using the app from a phone on your network.
//!
//!   commander-lab-web [--port 7878] [--host 0.0.0.0] [--no-browser] [--data DIR]

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use commander_lab_core::api::{default_data_dir, Engine};
use rust_embed::RustEmbed;
use serde_json::{json, Value};
use std::{net::SocketAddr, sync::Arc};

#[derive(RustEmbed)]
#[folder = "../frontend/dist"]
struct Assets;

struct Options {
    host: String,
    port: u16,
    open_browser: bool,
    data: std::path::PathBuf,
}

/// Hand-rolled flag parsing (four flags didn't justify clap). Bad values keep
/// the default, and unknown flags print a warning instead of exiting.
fn parse_args() -> Options {
    let mut opts = Options {
        host: "127.0.0.1".into(),
        port: 7878,
        open_browser: true,
        data: default_data_dir(),
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--host" => opts.host = args.next().unwrap_or(opts.host),
            "--port" => opts.port = args.next().and_then(|p| p.parse().ok()).unwrap_or(opts.port),
            "--no-browser" => opts.open_browser = false,
            "--data" => opts.data = args.next().map(Into::into).unwrap_or(opts.data),
            "-h" | "--help" => {
                println!(
                    "Commander Lab {} (browser edition)\n\n\
                     Usage: commander-lab-web [--port 7878] [--host 127.0.0.1] [--no-browser] [--data DIR]\n\n\
                     --host 0.0.0.0 lets other devices on your network (e.g. a phone) open it.",
                    commander_lab_core::VERSION
                );
                std::process::exit(0);
            }
            other => eprintln!("Ignoring unknown option {other}"),
        }
    }
    opts
}

/// POST /api/<cmd>: the same `Engine::call` as the desktop app, run on a
/// blocking thread. Engine errors come back as 200 with `{ok:false,error}`
/// (the UI shows them as toasts); only a panic gives a 500.
/// Security note: with --host 0.0.0.0 anyone on the network can call every
/// command, including deleting decks. There is no auth; keep it on trusted
/// networks.
async fn api(State(engine): State<Arc<Engine>>, Path(cmd): Path<String>, body: Option<Json<Value>>) -> Response {
    let args = body.map(|Json(v)| v).unwrap_or(Value::Null);
    let result = tokio::task::spawn_blocking(move || engine.call(&cmd, args)).await;
    match result {
        Ok(Ok(value)) => Json(json!({ "ok": true, "data": value })).into_response(),
        Ok(Err(err)) => Json(json!({ "ok": false, "error": format!("{err:#}") })).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "ok": false, "error": err.to_string() }))).into_response(),
    }
}

/// Serve the built UI embedded in the binary. Unknown paths get index.html
/// so hash routes and reloads keep working.
async fn static_files(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    let file = Assets::get(path).or_else(|| Assets::get("index.html"));
    match file {
        Some(content) => Response::builder()
            .header(header::CONTENT_TYPE, content.metadata.mimetype())
            .body(Body::from(content.data.into_owned()))
            .unwrap(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Start the browser edition. Steps: open the engine, mount /api and
/// /health, bind the first free port from --port up to +19, print the URL
/// (open the browser unless --no-browser), serve until Ctrl+C, then shut the
/// engine down cleanly.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts = parse_args();
    let engine = Engine::new(&opts.data, "web")?;
    println!("Commander Lab {} — data in {}", commander_lab_core::VERSION, engine.root().display());

    let app = Router::new()
        .route("/api/{cmd}", post(api))
        .route("/health", get(|| async { "ok" }))
        .fallback(static_files)
        .with_state(engine.clone());

    // Use the requested port, or the next free one.
    let mut listener = None;
    for port in opts.port..opts.port.saturating_add(20) {
        let addr: SocketAddr = format!("{}:{}", opts.host, port).parse()?;
        if let Ok(l) = tokio::net::TcpListener::bind(addr).await {
            listener = Some(l);
            break;
        }
    }
    let listener = listener.ok_or_else(|| anyhow::anyhow!("No free port near {}", opts.port))?;
    let addr = listener.local_addr()?;
    let browse = if opts.host == "0.0.0.0" {
        format!("http://127.0.0.1:{}", addr.port())
    } else {
        format!("http://{addr}")
    };
    println!("Open {browse}  (Ctrl+C to quit)");
    if opts.host == "0.0.0.0" {
        println!("Other devices on your network can use http://<this-computer's-IP>:{}", addr.port());
    }
    if opts.open_browser {
        let _ = open::that(&browse);
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    engine.shutdown();
    Ok(())
}
