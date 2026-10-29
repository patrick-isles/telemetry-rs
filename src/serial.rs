// ==============================================================================
// Module: serial.rs
// Purpose: Hardware Serial / PTY Ingestion Worker with POSIX Raw Mode
// Target: Linux Virtual PTY Loopback or Physical /dev/ttyUSB0
// Author: Patrick Isles (https://github.com/patrick-isles)
// License: MIT
// ==============================================================================

use crate::protocol::extract_frame_from_stream;
use crate::state::TelemetryState;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

/// Configure a file descriptor into POSIX raw mode to prevent newline translations
pub fn set_fd_raw_mode(fd: i32) {
    unsafe {
        let mut termios: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut termios) == 0 {
            libc::cfmakeraw(&mut termios);
            libc::tcsetattr(fd, libc::TCSANOW, &termios);
        }
    }
}

/// Spawns the serial background listener thread
pub fn spawn_serial_listener(
    device_path: String,
    state: Arc<RwLock<TelemetryState>>,
    verbose: bool,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        println!("\x1b[1m\x1b[36m[telemetry-rs]\x1b[0m Monitoring hardware bus on: \x1b[1m{}\x1b[0m", device_path);

        let mut read_buffer = [0u8; 128];
        let mut stream_accumulator = Vec::with_capacity(512);

        loop {
            if !Path::new(&device_path).exists() {
                thread::sleep(Duration::from_millis(50));
                continue;
            }

            let file_res = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOCTTY)
                .open(&device_path);

            match file_res {
                Ok(mut file) => {
                    let fd = file.as_raw_fd();
                    set_fd_raw_mode(fd);
                    println!("  \x1b[32m✓ Hardware connection established to {}\x1b[0m", device_path);

                    loop {
                        match file.read(&mut read_buffer) {
                            Ok(0) => {
                                // PTY master closed or disconnected — break to re-open loop
                                thread::sleep(Duration::from_millis(100));
                                break;
                            }
                            Ok(n) => {
                                stream_accumulator.extend_from_slice(&read_buffer[..n]);

                                while let Some(result) = extract_frame_from_stream(&mut stream_accumulator) {
                                    match result {
                                        Ok(packet) => {
                                            if verbose {
                                                println!(
                                                    "[RX #{:06}] {:.3}V | {:.3}A | {:.1}°C | GPIO: 0b{:08b} | CRC: OK",
                                                    packet.sequence_id,
                                                    packet.voltage_v(),
                                                    packet.current_a(),
                                                    packet.temperature_c(),
                                                    packet.gpio_byte
                                                );
                                            }
                                            if let Ok(mut lock) = state.write() {
                                                lock.update(packet);
                                            }
                                        }
                                        Err(e) => {
                                            if verbose {
                                                println!("\x1b[31m[CRC ERROR]\x1b[0m {:?} — Dropping frame.", e);
                                            }
                                            if let Ok(lock) = state.read() {
                                                lock.record_crc_error();
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                println!("  \x1b[33m! Serial stream read error: {}. Reconnecting...\x1b[0m", e);
                                break;
                            }
                        }
                    }
                }
                Err(_) => {
                    thread::sleep(Duration::from_millis(500));
                }
            }
        }
    })
}
