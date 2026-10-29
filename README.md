# Telemetry-RS (`telemetry-rs`)

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust: 2021 Edition](https://img.shields.io/badge/Rust-2021%20Edition-orange.svg)](https://www.rust-lang.org/)
[![Latency: 19.69 ns](https://img.shields.io/badge/Latency-19.69%20ns%2Fframe-brightgreen.svg)](docs/SYSTEMS_ENGINEERING.md)
[![Throughput: 50.8M pkts/s](https://img.shields.io/badge/Throughput-50.79M%20pkts%2Fsec-success.svg)](docs/SYSTEMS_ENGINEERING.md)
[![Telemetry: Prometheus](https://img.shields.io/badge/Metrics-Prometheus%20:9106-orange.svg)](http://localhost:9106/metrics)

> **High-performance, zero-allocation embedded hardware telemetry engine, real-time Digital Signal Processing (DSP) core, terminal oscilloscope, and Prometheus exporter written in pure Rust.**

`telemetry-rs` connects low-level electronic circuit measurements to modern systems observability with sub-microsecond latency. It ingests 19-byte binary serial frames over physical UART (`/dev/ttyUSB0`) or virtual Linux pseudo-terminals (`pty`), calculates real-time True RMS, Peak-to-Peak ripple, and dynamic impedance ($R = V/I, P = VI$), validates integrity using CRC-16-CCITT, and serves Prometheus metrics on `:9106` alongside an interactive high-refresh terminal oscilloscope.

---

## Architecture Topology

```
                  ┌────────────────────────────────────────┐
                  │ PHYSICAL SENSOR / SIMULATOR (ehb-sim)  │
                  │   UART Stream: 115200 8N1 / PTY        │
                  └──────────────────┬─────────────────────┘
                                     │ 19-byte binary frames (0xAA55)
                                     ▼
┌────────────────────────────────────────────────────────────────────────────┐
│                       `telemetry-rs` (RUST ENGINE)                         │
│                                                                            │
│  [ Thread 1: Hardware Ingestion Worker ]                                   │
│  • Reads byte stream from `/tmp/ehb-virtual-uart` or `/dev/ttyUSB0`        │
│  • Frame-hunting state machine locking on `0xAA 0x55`                      │
│  • Zero-copy payload slice unpacking (Zero heap allocations)               │
│  • Sub-microsecond CRC-16-CCITT bitwise table validation                   │
│                                    │                                       │
│                                    ▼ (Rust Arc<RwLock<TelemetryState>>)    │
│  [ Thread 2: Real-Time DSP Math Engine ]                                   │
│  • True RMS Calculation:           V_RMS = sqrt(1/N * Σ V_i²)              │
│  • AC Peak-to-Peak Ripple:         V_pk-pk = V_max - V_min                 │
│  • Exponential Moving Average:     y[n] = α * x[n] + (1 - α) * y[n-1]      │
│  • Dynamic Load Impedance:         R = V / I, P = V * I                    │
│                                    │                                       │
│            ┌───────────────────────┴───────────────────────┐               │
│            ▼                                               ▼               │
│  [ Thread 3: Prometheus Server ]              [ Thread 4: Terminal TUI ]   │
│  • Native HTTP server (:9106)                 • Live ANSI Oscilloscope     │
│  • OpenMetrics format for Grafana             • 60Hz Waveform Trace        │
│  • Sub-millisecond scrape latency             • 8-bit GPIO Logic LEDs      │
└────────────────────────────────────────────────────────────────────────────┘
```

---

## Empirical Performance Benchmarks

Benchmarked on AMD Ryzen 9 6900HS (Rembrandt Zen 3+, 8 cores / 16 threads @ 4.90 GHz) under Linux 7.0:

```
$ telemetry-rs --bench

=== TELEMETRY-RS HIGH-SPEED THROUGHPUT BENCHMARK ===

Benchmarking zero-copy packet parser over 5,000,000 frames...
  • Elapsed Time:         0.0984 seconds
  • Processing Latency:   19.69 ns / frame
  • Throughput:           50.79 million frames / sec
  • Verification Checksum: 25060000000

Benchmarking CRC-16-CCITT algorithm over 10,000,000 payload checks...
  • CRC-16 Throughput:    24.58 million checks / sec
  • Dummy CRC result:     0x0000
```

---

## Terminal Oscilloscope Preview

```
╔════════════════════════════════════════════════════════════════════════════════════╗
║   TELEMETRY-RS // REAL-TIME RUST HARDWARE DSP & OSCILLOSCOPE MONITOR               ║
║   Bus: /tmp/ehb-virtual-uart  | Exporter: http://localhost:9106/metrics            ║
╚════════════════════════════════════════════════════════════════════════════════════╝
 METRIC MEASUREMENTS (DC BUS & DSP ENGINE)
 VOLTAGE      [██████████████████████████░░]   5.08V ( 0.0- 5.5V)
 TRUE RMS     [█████████████████████████░░░]   5.01V ( 0.0- 5.5V)
 CURRENT      [███████░░░░░░░░░░░░░░░░░░░░░]   0.24A ( 0.0- 1.0A)
 POWER (P)    [███████░░░░░░░░░░░░░░░░░░░░░]   1.20W ( 0.0- 5.0W)
 TEMP (T)     [█████░░░░░░░░░░░░░░░░░░░░░░░]  24.12°C (10.0-90.0°C)

 LIVE VOLTAGE OSCILLOSCOPE TRACE (Time Domain)
   5.5V ┤▄▅▅▅▅▅▅▅▅▅▄▄▄▄▄▄▄▄▄▄▅▄▅▅▅▅
   4.4V ┤██████████████████████████
   3.3V ┤██████████████████████████
   2.2V ┤██████████████████████████
   1.1V ┤██████████████████████████
  0.0V ┴────────────────────────────────────────▶ t (recent samples)

 8-CHANNEL DIGITAL LOGIC ANALYZER (GPIO PORT)
  D0:[● HIGH] D1:[○ LOW ] D2:[● HIGH] D3:[○ LOW ]
  D4:[○ LOW ] D5:[○ LOW ] D6:[○ LOW ] D7:[○ LOW ]   Register: 0b00000101 (0x05)

 ELECTRICAL DSP PARAMETERS & FAULT MATRIX
  Dynamic Impedance:   21.55 Ω (Ohm's Law: R = V / I)
  AC Ripple (Noise):   0.171 V (Peak-to-Peak V_pk-pk)
  Energy Dissipated: 0.00000 Wh (0.00 Joules)
  ADC Quantization:  4095 / 4095 (12-bit ADC @ 1.22 mV/LSB)
  Fault Protection:  OverVoltage: [NORMAL] | Brownout: [NORMAL] | OverTemp: [NORMAL]
  Stream Integrity:  Packets RX: 26     | Seq: #000025 | CRC Drops: 0 (0.00%)
```

---

## Installation & Quickstart

### 1. Build and Install
```bash
git clone https://github.com/patrick-isles/telemetry-rs.git
cd telemetry-rs
./install.sh
```

### 2. Run the Hardware Simulator
Stream simulated hardware telemetry over a virtual PTY loopback:
```bash
ehb-sim
```

### 3. Launch the Rust Telemetry Engine
```bash
# Interactive Terminal Oscilloscope
telemetry-rs

# Or run headless as a background Prometheus daemon
telemetry-rs --headless --port 9106

# Or print a single snapshot and exit
telemetry-rs --once
```

---

## Prometheus Configuration

Add to `/etc/prometheus/prometheus.yml`:
```yaml
scrape_configs:
  - job_name: 'telemetry_rs'
    scrape_interval: 1s
    static_configs:
      - targets: ['localhost:9106']
```

### Exported Metrics:
* `eet_voltage_volts`: Instantaneous DC rail potential
* `eet_current_amperes`: Current measured across low-side shunt
* `eet_voltage_rms_volts`: True RMS potential ($V_{RMS} = \sqrt{\frac{1}{N}\sum V_i^2}$)
* `eet_voltage_ripple_pk_pk`: Peak-to-Peak AC ripple noise
* `eet_power_watts`: Dissipated power ($P = V \cdot I$)
* `eet_load_resistance_ohms`: Dynamic impedance ($R = V / I$)
* `eet_energy_watt_hours`: Total accumulated energy ($E = \int P \, dt$)
* `eet_gpio_pin_state{pin="0..7"}`: Digital logic states
* `eet_fault_active{fault="overvoltage|brownout|overtemp"}`: Electrical alarm trip bits

---

## Author & Academic Background

**Patrick Isles**  
* Portland Community College — Electronic Engineering Technology (`EET-100D`)
* Portland State University — B.A. Philosophy
* GitHub: [@patrick-isles](https://github.com/patrick-isles)

Bridging first-principles epistemology and circuit theory with low-level systems programming in Rust. Demonstrating compile-time memory safety, zero-copy deserialization, and deterministic real-time signal processing for mission-critical hardware observability.

---

## License

MIT License. See [LICENSE](LICENSE) for details.
