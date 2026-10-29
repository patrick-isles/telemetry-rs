// ==============================================================================
// Tool: telemetry-rs
// Purpose: High-Performance Zero-Allocation Rust Telemetry Engine & DSP Pipeline
// Target: Linux PTY Loopback or Physical Embedded Serial Port (/dev/ttyUSB0)
// Author: Patrick Isles (https://github.com/patrick-isles)
// License: MIT
// ==============================================================================

pub mod protocol;
pub mod dsp;
pub mod state;
pub mod serial;
pub mod exporter;
pub mod tui;

use state::TelemetryState;
use std::env;
use std::sync::{Arc, RwLock};
use std::time::Instant;

fn print_help() {
    println!(
        "\x1b[1mtelemetry-rs v1.0.0\x1b[0m\n\
        High-Performance Zero-Allocation Hardware Telemetry Engine & DSP Pipeline\n\n\
        \x1b[1mUSAGE:\x1b[0m\n    \
            telemetry-rs [OPTIONS]\n\n\
        \x1b[1mOPTIONS:\x1b[0m\n    \
            -d, --device <PATH>    Serial device path (default: /tmp/ehb-virtual-uart)\n    \
            -p, --port <PORT>      Prometheus HTTP exporter port (default: 9106)\n    \
            -r, --rate <HZ>        Terminal UI refresh rate (default: 10.0)\n    \
            -v, --verbose          Print every raw decoded frame to stdout\n    \
                --headless         Run as background daemon without TUI\n    \
                --once             Print single snapshot frame to terminal and exit\n    \
                --bench            Run built-in zero-copy throughput micro-benchmarks\n    \
            -h, --help             Display this help message\n"
    );
}

fn run_micro_benchmark() {
    println!("\x1b[1m\x1b[36m=== TELEMETRY-RS HIGH-SPEED THROUGHPUT BENCHMARK ===\x1b[0m\n");

    let mut payload = Vec::new();
    payload.extend_from_slice(&5012u16.to_be_bytes());
    payload.extend_from_slice(&248u16.to_be_bytes());
    payload.extend_from_slice(&4095u16.to_be_bytes());
    payload.extend_from_slice(&2540i16.to_be_bytes());
    payload.push(0b00100101);
    payload.push(0x00);
    payload.extend_from_slice(&1000u32.to_be_bytes());

    let crc = protocol::crc16_ccitt(&payload);

    let mut frame = Vec::new();
    frame.push(protocol::SYNC_BYTE_0);
    frame.push(protocol::SYNC_BYTE_1);
    frame.push(protocol::EXPECTED_PAYLOAD_LEN);
    frame.extend_from_slice(&payload);
    frame.extend_from_slice(&crc.to_be_bytes());

    const ITERATIONS: usize = 5_000_000;
    println!("Benchmarking zero-copy packet parser over {} frames...", ITERATIONS);

    let t0 = Instant::now();
    let mut checksum_acc: u64 = 0;

    for _ in 0..ITERATIONS {
        let packet = protocol::TelemetryPacket::parse(&frame).expect("benchmark frame must parse");
        checksum_acc = checksum_acc.wrapping_add(packet.voltage_mv as u64);
    }

    let elapsed = t0.elapsed();
    let secs = elapsed.as_secs_f64();
    let frames_per_sec = ITERATIONS as f64 / secs;
    let ns_per_frame = (secs * 1e9) / ITERATIONS as f64;

    println!("  • Elapsed Time:         {:.4} seconds", secs);
    println!("  • Processing Latency:   \x1b[1m\x1b[32m{:.2} ns / frame\x1b[0m", ns_per_frame);
    println!("  • Throughput:           \x1b[1m\x1b[32m{:.2} million frames / sec\x1b[0m", frames_per_sec / 1e6);
    println!("  • Verification Checksum: {}\n", checksum_acc);

    println!("Benchmarking CRC-16-CCITT algorithm over 10,000,000 payload checks...");
    let t_crc = Instant::now();
    let mut dummy_crc = 0u16;
    for _ in 0..10_000_000 {
        dummy_crc ^= protocol::crc16_ccitt(&payload);
    }
    let crc_elapsed = t_crc.elapsed().as_secs_f64();
    let crc_per_sec = 10_000_000.0 / crc_elapsed;
    println!("  • CRC-16 Throughput:    \x1b[1m\x1b[32m{:.2} million checks / sec\x1b[0m", crc_per_sec / 1e6);
    println!("  • Dummy CRC result:     0x{:04X}\n", dummy_crc);
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut device_path = String::from("/tmp/ehb-virtual-uart");
    let mut http_port: u16 = 9106;
    let mut rate_hz: f64 = 10.0;
    let mut verbose = false;
    let mut headless = false;
    let mut once = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print_help();
                return;
            }
            "--bench" => {
                run_micro_benchmark();
                return;
            }
            "-v" | "--verbose" => {
                verbose = true;
            }
            "--headless" => {
                headless = true;
            }
            "--once" => {
                once = true;
            }
            "-d" | "--device" => {
                if i + 1 < args.len() {
                    device_path = args[i + 1].clone();
                    i += 1;
                }
            }
            "-p" | "--port" => {
                if i + 1 < args.len() {
                    if let Ok(p) = args[i + 1].parse() {
                        http_port = p;
                    }
                    i += 1;
                }
            }
            "-r" | "--rate" => {
                if i + 1 < args.len() {
                    if let Ok(r) = args[i + 1].parse() {
                        rate_hz = r;
                    }
                    i += 1;
                }
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                print_help();
                return;
            }
        }
        i += 1;
    }

    let state = Arc::new(RwLock::new(TelemetryState::new()));

    // 1. Spawn Serial / PTY ingestion worker thread
    let _serial_handle = serial::spawn_serial_listener(device_path.clone(), Arc::clone(&state), verbose);

    // 2. Spawn Prometheus HTTP metrics server thread
    if let Err(e) = exporter::spawn_http_exporter(http_port, Arc::clone(&state)) {
        eprintln!("\x1b[31m[telemetry-rs] Failed to bind HTTP server to port {}: {}\x1b[0m", http_port, e);
    }

    // 3. Run UI or idle loop
    if headless {
        println!("\x1b[1m\x1b[32m[telemetry-rs]\x1b[0m Running in headless daemon mode. Press [Ctrl+C] to exit.");
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    } else {
        if once {
            let t_wait = Instant::now();
            while t_wait.elapsed().as_millis() < 2500 {
                let count = if let Ok(lock) = state.read() {
                    lock.packets_total.load(std::sync::atomic::Ordering::Relaxed)
                } else {
                    0
                };
                if count >= 2 {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
        tui::run_tui_loop(Arc::clone(&state), device_path, http_port, rate_hz, once);
    }
}
