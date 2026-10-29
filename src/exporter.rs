// ==============================================================================
// Module: exporter.rs
// Purpose: Embedded HTTP Server for Prometheus OpenMetrics & JSON Telemetry API
// Target: Standard Library TcpListener on port 9106
// Author: Patrick Isles (https://github.com/patrick-isles)
// License: MIT
// ==============================================================================

use crate::state::TelemetryState;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, RwLock};
use std::thread;

pub fn spawn_http_exporter(
    port: u16,
    state: Arc<RwLock<TelemetryState>>,
) -> std::io::Result<thread::JoinHandle<()>> {
    let address = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&address)?;
    println!("\x1b[1m\x1b[36m[telemetry-rs]\x1b[0m Prometheus metrics exporter online: \x1b[1mhttp://localhost:{}/metrics\x1b[0m", port);

    let handle = thread::spawn(move || {
        for stream_res in listener.incoming() {
            if let Ok(stream) = stream_res {
                let state_clone = Arc::clone(&state);
                thread::spawn(move || {
                    handle_client(stream, state_clone);
                });
            }
        }
    });

    Ok(handle)
}

fn handle_client(mut stream: TcpStream, state: Arc<RwLock<TelemetryState>>) {
    let mut buffer = [0u8; 1024];
    if let Ok(n) = stream.read(&mut buffer) {
        if n == 0 {
            return;
        }

        let request = String::from_utf8_lossy(&buffer[..n]);
        let first_line = request.lines().next().unwrap_or("");
        let parts: Vec<&str> = first_line.split_whitespace().collect();

        if parts.len() >= 2 && parts[0] == "GET" {
            let path = parts[1];
            if path == "/metrics" {
                let body = if let Ok(lock) = state.read() {
                    lock.to_prometheus()
                } else {
                    String::from("# Error reading state\n")
                };

                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
            } else if path == "/api/telemetry" || path == "/status" || path == "/" {
                let body = if let Ok(lock) = state.read() {
                    lock.to_json()
                } else {
                    String::from("{\"error\": \"state lock poisoned\"}")
                };

                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
            } else {
                let body = "404 Not Found";
                let response = format!(
                    "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
            }
        }
    }
}
