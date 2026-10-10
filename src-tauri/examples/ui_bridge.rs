//! Runs the real backend behind a local HTTP port, so the interface can be
//! driven in an ordinary browser (Playwright) against real commands, a real
//! database and real permission checks. For screenshots and end-to-end
//! checks on a machine with no desktop; never shipped.
//!
//!     cargo run --example ui_bridge -- <empty-data-folder> <port>
//!
//! The page sends `POST /invoke {"cmd": "...", "args": {...}}` instead of
//! Tauri IPC (see scripts/ui-bridge.js) and gets `{"ok": value}` or
//! `{"err": {"code", "message"}}` back.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;

use results_manager_lib::state::{AppPaths, AppState};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
use tauri::webview::InvokeRequest;

fn main() {
    let mut args = std::env::args().skip(1);
    let data_dir = PathBuf::from(args.next().expect("usage: ui_bridge <data-folder> <port>"));
    let port: u16 = args.next().and_then(|p| p.parse().ok()).unwrap_or(4399);

    let paths = AppPaths::resolve(data_dir).expect("data folder");
    let state = AppState::new(paths).expect("database");
    let app = mock_builder()
        .manage(state)
        .invoke_handler(results_manager_lib::invoke_handler())
        .build(mock_context(noop_assets()))
        .expect("mock app");
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("mock window");

    let listener = TcpListener::bind(("127.0.0.1", port)).expect("port");
    eprintln!("ui_bridge listening on http://127.0.0.1:{port}");

    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));
        let mut request_line = String::new();
        if reader.read_line(&mut request_line).is_err() {
            continue;
        }
        let mut length = 0usize;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() || line == "\r\n" || line.is_empty() {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse().unwrap_or(0);
            }
        }
        let mut body = vec![0u8; length];
        if reader.read_exact(&mut body).is_err() {
            continue;
        }

        let reply = if request_line.starts_with("OPTIONS") {
            String::new()
        } else {
            let request: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
            let cmd = request["cmd"].as_str().unwrap_or_default().to_string();
            let args = request.get("args").cloned().unwrap_or(serde_json::json!({}));
            let response = get_ipc_response(
                &webview,
                InvokeRequest {
                    cmd,
                    callback: CallbackFn(0),
                    error: CallbackFn(1),
                    // The app's own origin, so the ACL treats the page as local.
                    url: "tauri://localhost".parse().unwrap(),
                    body: InvokeBody::Json(args),
                    headers: Default::default(),
                    invoke_key: INVOKE_KEY.to_string(),
                },
            );
            match response {
                Ok(body) => {
                    let value = body.deserialize::<serde_json::Value>().unwrap_or(serde_json::Value::Null);
                    serde_json::json!({ "ok": value }).to_string()
                }
                Err(error) => serde_json::json!({ "err": error }).to_string(),
            }
        };
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n\
             Access-Control-Allow-Headers: *\r\nAccess-Control-Allow-Methods: POST, OPTIONS\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{reply}",
            reply.len()
        );
    }
}
