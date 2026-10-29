// ==============================================================================
// Module: tui.rs
// Purpose: Interactive Terminal Oscilloscope & Telemetry Dashboard
// Target: High-Refresh ANSI Terminal Canvas & 8-Bit Logic Analyzer
// Author: Patrick Isles (https://github.com/patrick-isles)
// License: MIT
// ==============================================================================

use crate::state::TelemetryState;
use std::io::{self, Write};
use std::sync::atomic::Ordering;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

pub const C_RESET: &str = "\x1b[0m";
pub const C_BOLD: &str = "\x1b[1m";
pub const C_DIM: &str = "\x1b[2m";
pub const C_RED: &str = "\x1b[31m";
pub const C_GREEN: &str = "\x1b[32m";
pub const C_YELLOW: &str = "\x1b[33m";
pub const C_BLUE: &str = "\x1b[34m";
pub const C_CYAN: &str = "\x1b[36m";
pub const C_WHITE: &str = "\x1b[37m";
pub const C_CLEAR: &str = "\x1b[2J\x1b[H";
pub const C_HIDE_CURSOR: &str = "\x1b[?25l";
pub const C_SHOW_CURSOR: &str = "\x1b[?25h";

fn draw_gauge(label: &str, val: f64, min_v: f64, max_v: f64, unit: &str, width: usize, color: &str) -> String {
    let clamped = val.clamp(min_v, max_v);
    let ratio = if max_v > min_v { (clamped - min_v) / (max_v - min_v) } else { 0.0 };
    let filled = (ratio * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);

    let bar = format!("{}{}{}{}{}", color, "█".repeat(filled), C_DIM, "░".repeat(empty), C_RESET);
    format!("{:<12} [{}] {}{:6.2}{}{} {}({:4.1}-{:4.1}{}){}", label, bar, C_BOLD, val, unit, C_RESET, C_DIM, min_v, max_v, unit, C_RESET)
}

fn render_waveform_trace(history: &[f64], min_val: f64, max_val: f64, height: usize) -> Vec<String> {
    let symbols = [" ", " ", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
    let mut lines = Vec::new();
    let val_range = if max_val > min_val { max_val - min_val } else { 1.0 };

    for row in (0..height).rev() {
        let row_min = min_val + (row as f64 / height as f64) * val_range;
        let row_max = min_val + ((row + 1) as f64 / height as f64) * val_range;
        let mut row_chars = String::new();

        for &v in history {
            if v >= row_max {
                row_chars.push('█');
            } else if v < row_min {
                row_chars.push(' ');
            } else {
                let frac = (v - row_min) / (row_max - row_min);
                let idx = ((frac * (symbols.len() - 1) as f64).round() as usize).min(symbols.len() - 1);
                row_chars.push_str(symbols[idx]);
            }
        }
        let label = format!("{:4.1}V ┤", row_max);
        lines.push(format!("{}{}{}{}{}", C_DIM, label, C_RESET, C_CYAN, row_chars));
    }
    lines
}

pub fn render_frame(state: &TelemetryState, device_path: &str, http_port: u16) -> String {
    let pkts = state.packets_total.load(Ordering::Relaxed);
    let crc_err = state.crc_errors_total.load(Ordering::Relaxed);
    let crc_pct = if pkts > 0 { (crc_err as f64 / pkts as f64) * 100.0 } else { 0.0 };

    let mut out = Vec::new();
    out.push(format!("{}", C_CLEAR));
    out.push(format!("{}╔════════════════════════════════════════════════════════════════════════════════════╗{}", C_BOLD, C_RESET));
    out.push(format!("{}║   TELEMETRY-RS // REAL-TIME RUST HARDWARE DSP & OSCILLOSCOPE MONITOR               ║{}", C_BOLD, C_RESET));
    out.push(format!("{}║   Bus: {}{:<22}{} | Exporter: {}http://localhost:{}/metrics{}   ║{}",
        C_BOLD, C_CYAN, device_path, C_RESET, C_GREEN, http_port, C_RESET, C_RESET));
    out.push(format!("{}╚════════════════════════════════════════════════════════════════════════════════════╝{}", C_BOLD, C_RESET));

    out.push(format!(" {}METRIC MEASUREMENTS (DC BUS & DSP ENGINE){}", C_BOLD, C_RESET));
    out.push(format!(" {}", draw_gauge("VOLTAGE", state.voltage_v, 0.0, 5.5, "V", 28, C_CYAN)));
    out.push(format!(" {}", draw_gauge("TRUE RMS", state.voltage_rms, 0.0, 5.5, "V", 28, C_BLUE)));
    out.push(format!(" {}", draw_gauge("CURRENT", state.current_a, 0.0, 1.0, "A", 28, C_YELLOW)));
    out.push(format!(" {}", draw_gauge("POWER (P)", state.power_w, 0.0, 5.0, "W", 28, C_GREEN)));
    out.push(format!(" {}", draw_gauge("TEMP (T)", state.temperature_c, 10.0, 90.0, "°C", 28, if state.temperature_c > 60.0 { C_RED } else { C_BLUE })));

    out.push(format!("\n {}LIVE VOLTAGE OSCILLOSCOPE TRACE (Time Domain){}", C_BOLD, C_RESET));
    let sparklines = render_waveform_trace(&state.history, 0.0, 5.5, 5);
    for sl in sparklines {
        out.push(format!("  {}", sl));
    }
    out.push(format!("  {}0.0V ┴{}▶ t (recent samples){}", C_DIM, "─".repeat(state.history.len().max(40)), C_RESET));

    out.push(format!("\n {}8-CHANNEL DIGITAL LOGIC ANALYZER (GPIO PORT){}", C_BOLD, C_RESET));
    let mut pin_boxes = Vec::new();
    for pin in 0..8 {
        let is_high = (state.gpio_byte & (1 << pin)) != 0;
        let badge = if is_high {
            format!("{}● HIGH{}", C_GREEN, C_RESET)
        } else {
            format!("{}○ LOW {}", C_DIM, C_RESET)
        };
        pin_boxes.push(format!("D{}:[{}]", pin, badge));
    }
    out.push(format!("  {}", pin_boxes[..4].join(" ")));
    out.push(format!("  {}   {}Register: 0b{:08b} (0x{:02X}){}",
        pin_boxes[4..].join(" "), C_DIM, state.gpio_byte, state.gpio_byte, C_RESET));

    out.push(format!("\n {}ELECTRICAL DSP PARAMETERS & FAULT MATRIX{}", C_BOLD, C_RESET));
    let ov_status = if (state.fault_byte & 0x01) != 0 {
        format!("{}[TRIPPED ⚡]{}", C_RED, C_RESET)
    } else {
        format!("{}[NORMAL]{}", C_GREEN, C_RESET)
    };
    let bo_status = if (state.fault_byte & 0x02) != 0 {
        format!("{}[BROWNOUT ⚠]{}", C_RED, C_RESET)
    } else {
        format!("{}[NORMAL]{}", C_GREEN, C_RESET)
    };
    let ot_status = if (state.fault_byte & 0x04) != 0 {
        format!("{}[OVERTEMP 🔥]{}", C_RED, C_RESET)
    } else {
        format!("{}[NORMAL]{}", C_GREEN, C_RESET)
    };

    out.push(format!("  Dynamic Impedance: {}{:7.2} Ω{} (Ohm's Law: R = V / I)", C_BOLD, state.resistance_ohms, C_RESET));
    out.push(format!("  AC Ripple (Noise): {}{:7.3} V{} (Peak-to-Peak V_pk-pk)", C_BOLD, state.voltage_ripple_pk_pk, C_RESET));
    out.push(format!("  Energy Dissipated: {}{:7.5} Wh{} ({:.2} Joules)", C_BOLD, state.pipeline.energy_watt_hours(), C_RESET, state.pipeline.total_energy_joules));
    out.push(format!("  ADC Quantization:  {}{:4} / 4095{} (12-bit ADC @ 1.22 mV/LSB)", C_BOLD, state.raw_adc, C_RESET));
    out.push(format!("  Fault Protection:  OverVoltage: {} | Brownout: {} | OverTemp: {}", ov_status, bo_status, ot_status));
    out.push(format!("  Stream Integrity:  Packets RX: {}{:<6}{} | Seq: #{:06} | CRC Drops: {} ({:.2}%)",
        C_BOLD, pkts, C_RESET, state.sequence_id, crc_err, crc_pct));

    out.join("\n")
}

pub fn run_tui_loop(
    state: Arc<RwLock<TelemetryState>>,
    device_path: String,
    http_port: u16,
    rate_hz: f64,
    once: bool,
) {
    let mut stdout = io::stdout();
    let interval = Duration::from_millis(((1.0 / rate_hz.max(1.0).min(60.0)) * 1000.0) as u64);

    if !once {
        let _ = write!(stdout, "{}", C_HIDE_CURSOR);
        let _ = stdout.flush();
    }

    loop {
        let frame_str = if let Ok(lock) = state.read() {
            render_frame(&lock, &device_path, http_port)
        } else {
            String::from("Error acquiring telemetry lock")
        };

        let _ = writeln!(stdout, "{}\n{}Press [Ctrl+C] to exit telemetry-rs.{}", frame_str, C_DIM, C_RESET);
        let _ = stdout.flush();

        if once {
            break;
        }

        thread::sleep(interval);
    }

    if !once {
        let _ = write!(stdout, "{}\n", C_SHOW_CURSOR);
        let _ = stdout.flush();
    }
}
