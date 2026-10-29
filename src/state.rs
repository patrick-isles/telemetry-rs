// ==============================================================================
// Module: state.rs
// Purpose: Thread-Safe Telemetry State & Prometheus Exporter Formatter
// Target: Arc<RwLock<TelemetryState>> for Concurrent Ingestion and Rendering
// Author: Patrick Isles (https://github.com/patrick-isles)
// License: MIT
// ==============================================================================

use crate::dsp::SignalPipeline;
use crate::protocol::TelemetryPacket;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

pub const HISTORY_CAPACITY: usize = 64;

#[derive(Debug)]
pub struct TelemetryState {
    pub latest_packet: Option<TelemetryPacket>,
    pub pipeline: SignalPipeline,
    pub voltage_v: f64,
    pub current_a: f64,
    pub power_w: f64,
    pub resistance_ohms: f64,
    pub voltage_rms: f64,
    pub voltage_ripple_pk_pk: f64,
    pub raw_adc: u16,
    pub temperature_c: f64,
    pub gpio_byte: u8,
    pub fault_byte: u8,
    pub sequence_id: u32,
    pub history: Vec<f64>,
    pub packets_total: Arc<AtomicU64>,
    pub crc_errors_total: Arc<AtomicU64>,
    pub last_update: Instant,
    pub start_time: Instant,
}

impl TelemetryState {
    pub fn new() -> Self {
        Self {
            latest_packet: None,
            pipeline: SignalPipeline::new(32, 0.25),
            voltage_v: 0.0,
            current_a: 0.0,
            power_w: 0.0,
            resistance_ohms: 0.0,
            voltage_rms: 0.0,
            voltage_ripple_pk_pk: 0.0,
            raw_adc: 0,
            temperature_c: 0.0,
            gpio_byte: 0,
            fault_byte: 0,
            sequence_id: 0,
            history: Vec::with_capacity(HISTORY_CAPACITY),
            packets_total: Arc::new(AtomicU64::new(0)),
            crc_errors_total: Arc::new(AtomicU64::new(0)),
            last_update: Instant::now(),
            start_time: Instant::now(),
        }
    }

    /// Update state with a freshly validated packet
    pub fn update(&mut self, packet: TelemetryPacket) {
        let v = packet.voltage_v();
        let i = packet.current_a();
        let now_secs = self.start_time.elapsed().as_secs_f64();

        // Feed DSP pipeline
        self.pipeline.process_sample(v, i, now_secs);

        self.latest_packet = Some(packet);
        self.voltage_v = v;
        self.current_a = i;
        self.power_w = packet.power_w();
        self.resistance_ohms = packet.resistance_ohms();
        self.voltage_rms = self.pipeline.voltage_rms();
        self.voltage_ripple_pk_pk = self.pipeline.voltage_ripple_pk_pk();
        self.raw_adc = packet.raw_adc;
        self.temperature_c = packet.temperature_c();
        self.gpio_byte = packet.gpio_byte;
        self.fault_byte = packet.fault_byte;
        self.sequence_id = packet.sequence_id;
        self.last_update = Instant::now();

        // Update history sparkline buffer
        if self.history.len() >= HISTORY_CAPACITY {
            self.history.remove(0);
        }
        self.history.push(v);

        self.packets_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_crc_error(&self) {
        self.crc_errors_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn is_stale(&self) -> bool {
        self.last_update.elapsed().as_secs_f64() > 2.0
    }

    /// Serialize state to standard Prometheus OpenMetrics format
    pub fn to_prometheus(&self) -> String {
        let pkts = self.packets_total.load(Ordering::Relaxed);
        let crc_err = self.crc_errors_total.load(Ordering::Relaxed);
        let stale = if self.is_stale() { 1 } else { 0 };

        let mut out = String::with_capacity(1024);
        out.push_str("# HELP eet_voltage_volts Measured DC bus voltage in Volts\n");
        out.push_str("# TYPE eet_voltage_volts gauge\n");
        out.push_str(&format!("eet_voltage_volts {:.4}\n\n", self.voltage_v));

        out.push_str("# HELP eet_current_amperes Measured DC bus current in Amperes\n");
        out.push_str("# TYPE eet_current_amperes gauge\n");
        out.push_str(&format!("eet_current_amperes {:.4}\n\n", self.current_a));

        out.push_str("# HELP eet_voltage_rms_volts Calculated True RMS bus potential in Volts\n");
        out.push_str("# TYPE eet_voltage_rms_volts gauge\n");
        out.push_str(&format!("eet_voltage_rms_volts {:.4}\n\n", self.voltage_rms));

        out.push_str("# HELP eet_voltage_ripple_pk_pk Peak-to-Peak AC voltage ripple in Volts\n");
        out.push_str("# TYPE eet_voltage_ripple_pk_pk gauge\n");
        out.push_str(&format!("eet_voltage_ripple_pk_pk {:.4}\n\n", self.voltage_ripple_pk_pk));

        out.push_str("# HELP eet_power_watts Calculated electrical power dissipation in Watts (P = V * I)\n");
        out.push_str("# TYPE eet_power_watts gauge\n");
        out.push_str(&format!("eet_power_watts {:.4}\n\n", self.power_w));

        out.push_str("# HELP eet_load_resistance_ohms Dynamic load resistance in Ohms (R = V / I)\n");
        out.push_str("# TYPE eet_load_resistance_ohms gauge\n");
        out.push_str(&format!("eet_load_resistance_ohms {:.3}\n\n", self.resistance_ohms));

        out.push_str("# HELP eet_energy_watt_hours Total electrical energy accumulated in Watt-hours\n");
        out.push_str("# TYPE eet_energy_watt_hours counter\n");
        out.push_str(&format!("eet_energy_watt_hours {:.6}\n\n", self.pipeline.energy_watt_hours()));

        out.push_str("# HELP eet_temperature_celsius Measured thermal sensor temperature in Celsius\n");
        out.push_str("# TYPE eet_temperature_celsius gauge\n");
        out.push_str(&format!("eet_temperature_celsius {:.2}\n\n", self.temperature_c));

        out.push_str("# HELP eet_adc_raw_value Raw 12-bit ADC integer register reading\n");
        out.push_str("# TYPE eet_adc_raw_value gauge\n");
        out.push_str(&format!("eet_adc_raw_value {}\n\n", self.raw_adc));

        out.push_str("# HELP eet_gpio_pin_state Digital logic state of GPIO pins (1=HIGH, 0=LOW)\n");
        out.push_str("# TYPE eet_gpio_pin_state gauge\n");
        for pin in 0..8 {
            let state = if (self.gpio_byte & (1 << pin)) != 0 { 1 } else { 0 };
            out.push_str(&format!("eet_gpio_pin_state{{pin=\"{}\"}} {}\n", pin, state));
        }

        out.push_str("\n# HELP eet_packets_total Monotonic counter of valid telemetry packets received\n");
        out.push_str("# TYPE eet_packets_total counter\n");
        out.push_str(&format!("eet_packets_total {}\n\n", pkts));

        out.push_str("# HELP eet_crc_errors_total Count of corrupted serial frames dropped by CRC-16 check\n");
        out.push_str("# TYPE eet_crc_errors_total counter\n");
        out.push_str(&format!("eet_crc_errors_total {}\n\n", crc_err));

        out.push_str("# HELP eet_fault_active Hardware fault alarm bitmask\n");
        out.push_str("# TYPE eet_fault_active gauge\n");
        out.push_str(&format!("eet_fault_active{{fault=\"overvoltage\"}} {}\n", if (self.fault_byte & 0x01) != 0 { 1 } else { 0 }));
        out.push_str(&format!("eet_fault_active{{fault=\"brownout\"}} {}\n", if (self.fault_byte & 0x02) != 0 { 1 } else { 0 }));
        out.push_str(&format!("eet_fault_active{{fault=\"overtemp\"}} {}\n\n", if (self.fault_byte & 0x04) != 0 { 1 } else { 0 }));

        out.push_str("# HELP eet_telemetry_stale 1 if serial stream interrupted for >2s, 0 otherwise\n");
        out.push_str("# TYPE eet_telemetry_stale gauge\n");
        out.push_str(&format!("eet_telemetry_stale {}\n", stale));

        out
    }

    /// Serialize state to JSON for HTTP API or CLI consumption
    pub fn to_json(&self) -> String {
        let pkts = self.packets_total.load(Ordering::Relaxed);
        let crc_err = self.crc_errors_total.load(Ordering::Relaxed);
        format!(
            "{{\"voltage_v\": {:.3}, \"current_a\": {:.3}, \"power_w\": {:.3}, \"resistance_ohms\": {:.2}, \"voltage_rms\": {:.3}, \"voltage_ripple_pk_pk\": {:.3}, \"energy_wh\": {:.5}, \"raw_adc\": {}, \"temperature_c\": {:.1}, \"gpio_byte\": {}, \"fault_byte\": {}, \"sequence_id\": {}, \"packets_total\": {}, \"crc_errors\": {}, \"stale\": {}}}",
            self.voltage_v,
            self.current_a,
            self.power_w,
            self.resistance_ohms,
            self.voltage_rms,
            self.voltage_ripple_pk_pk,
            self.pipeline.energy_watt_hours(),
            self.raw_adc,
            self.temperature_c,
            self.gpio_byte,
            self.fault_byte,
            self.sequence_id,
            pkts,
            crc_err,
            self.is_stale()
        )
    }
}
