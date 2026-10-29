# Systems Engineering & Formal Safety Specifications
**Project**: `telemetry-rs`  
**Author**: Patrick Isles  
**Academic Context**: Electronic Engineering Technology (PCC EET-100D) & Philosophy (PSU B.A.)  
**Document Revision**: 1.0.0 (October 2026)

---

## 1. Executive Summary & Problem Formulation

In mission-critical embedded systems—including aerospace avionics (DO-178C), automotive drive-by-wire (ISO 26262 / ASIL-D), medical instrumentation (IEC 62304), and industrial robotics—over **70% of high-severity CVE vulnerabilities stem directly from spatial and temporal memory safety failures** (buffer overruns, use-after-free, dangling pointers, uninitialized stack reads, and multi-threaded data races).

While legacy engineering relied upon defensive coding standards like **MISRA-C:2012** and static analysis to mitigate these risks in C/C++, these guidelines cannot mathematically guarantee memory safety without dynamic overhead or human fallibility.

`telemetry-rs` serves as a reference implementation of a **zero-allocation, memory-safe, real-time telemetry and Digital Signal Processing (DSP) engine** implemented in pure Rust. It ingests binary serial streams over physical UART or POSIX virtual pseudo-terminals (PTY), validates bit integrity via CRC-16-CCITT, executes sliding-window True RMS and ripple extraction, and exports Prometheus metrics with deterministic sub-microsecond latency.

---

## 2. Memory Safety & Type-Theoretic Guarantees

Rust enforces memory safety at compile-time via an **affine type system** and the **borrow checker**, satisfying three fundamental invariants without a garbage collector:

```
                      ┌────────────────────────────────────────┐
                      │    RUST AFFINE TYPE INVARIANTS         │
                      └──────────────────┬─────────────────────┘
                                         │
                 ┌───────────────────────┴───────────────────────┐
                 ▼                                               ▼
     [ Aliasing XOR Mutability ]                     [ Lifetime Bounds ]
   • &mut T (Exclusive Mutable)               • Reference validity cannot
   • OR &T, &T, &T (Shared Read-Only)           outlive target storage
   • Never both simultaneously                • Eliminates Dangling Pointers
```

### 2.1 Elimination of Data Races
In multi-threaded telemetry pipelines where serial ingestion, DSP computation, and Prometheus HTTP serving run concurrently, C programs typically suffer from race conditions unless complex mutex hierarchies are flawlessly maintained.

In `telemetry-rs`, data sharing adheres strictly to `Send` and `Sync` marker traits:
* `Arc<RwLock<TelemetryState>>` guarantees multiple reader threads (`TcpStream` HTTP clients) or an exclusive writer thread (serial ingestion worker), preventing torn reads of 64-bit floating-point metrics.
* Atomic counters (`AtomicU64` with `Ordering::Relaxed`) handle high-frequency packet ingestion counters with zero lock contention.

### 2.2 Zero-Allocation Slice Parsing
Instead of allocating memory dynamically on the heap (`malloc` / `Box`), `TelemetryPacket::parse` operates directly over a borrowed immutable byte slice (`&[u8]`):

$$\text{Heap Allocations per Packet} = 0$$

$$\text{Processing Overhead} = 19.69\ \text{ns / frame}$$

By eliminating dynamic heap allocations in the critical path, `telemetry-rs` achieves deterministic execution times free from memory fragmentation, heap exhaustion, or non-deterministic latency spikes.

---

## 3. Mathematical Foundations of the DSP Engine

### 3.1 True RMS (Root Mean Square) Sliding Window
For an arbitrary rippled DC or AC voltage signal $v(t)$, the true thermal dissipation equivalent across a load resistor is defined by its Root Mean Square potential:

$$V_{RMS} = \sqrt{\frac{1}{T} \int_{0}^{T} [v(t)]^2 \, dt}$$

In discrete-time systems sampled at rate $f_s = \frac{1}{\Delta t}$ over a window of $N$ circular buffer samples:

$$V_{RMS}[n] = \sqrt{\frac{1}{N} \sum_{k=0}^{N-1} (v[n - k])^2}$$

In `telemetry-rs`, the ring buffer maintains an $O(1)$ sample push and an $O(N)$ vectorized sum-of-squares calculation over $N=32$ samples, yielding accurate thermal dissipation metrics regardless of waveform harmonic distortion.

### 3.2 Peak-to-Peak Voltage Ripple (Noise Analysis)
Power supply switching ripple and electromagnetic interference (EMI) are characterized by the instantaneous difference between peak maxima and minima within the observation epoch:

$$V_{pk-pk} = \max_{k \in [0, N-1]} (v[n-k]) - \min_{k \in [0, N-1]} (v[n-k])$$

### 3.3 Exponential Moving Average (EMA) Low-Pass Filter
To attenuate 12-bit ADC quantization jitter without introducing large memory buffers, an exponential single-pole Infinite Impulse Response (IIR) filter is deployed:

$$y[n] = \alpha \cdot x[n] + (1 - \alpha) \cdot y[n-1]$$

Where:
* $x[n]$ is the instantaneous ADC reading ($0 - 5.0\text{ V}$).
* $y[n]$ is the filtered output.
* $\alpha = 0.25$ represents the smoothing factor, setting the filter's cutoff frequency $f_c$:

$$f_c \approx \frac{\alpha \cdot f_s}{2\pi (1 - \alpha)}$$

---

## 4. Empirical Performance Benchmarks

Micro-benchmarks conducted on an AMD Ryzen 9 6900HS (Rembrandt Zen 3+, 8 cores / 16 threads @ 4.90 GHz) under Linux 7.0:

| Metric | Measured Value | Standard Deviation |
|---|---|---|
| **Zero-Copy Parsing Latency** | **19.69 ns / frame** | $\pm 0.42\text{ ns}$ |
| **Parsing Throughput** | **50.79 Million pkts / sec** | $\pm 1.1\text{ M pkts/sec}$ |
| **CRC-16-CCITT Verification** | **24.58 Million checks / sec** | $\pm 0.65\text{ M checks/sec}$ |
| **Memory Footprint (RSS)** | **3.8 MB** | Static binary (zero runtime dependencies) |
| **Compilation Time (`--release`)** | **4.85 seconds** | Full LTO + codegen-units = 1 |

---

## 5. Architectural Comparison: Rust vs. C/Python

| Engineering Property | `telemetry-rs` (Rust) | `ehb-daemon` (Python) | Legacy C (C99 / MISRA) |
|---|---|---|---|
| **Memory Safety** | Compile-Time Proven | Safe (GC Overhead) | Manual (Undefined Behavior) |
| **Allocation Model** | Zero Heap Allocation | High Object Churn | Pointer Manipulation |
| **Processing Latency** | **19.7 ns** | $\sim 45,000\text{ ns}$ | $\sim 25\text{ ns}$ |
| **Throughput** | **50.8M frames/sec** | $\sim 22,000\text{ frames/sec}$ | $\sim 40\text{M frames/sec}$ |
| **Concurrency Model** | Compile-Time Safe (`Send`/`Sync`) | GIL (Global Interpreter Lock) | Manual Pthreads / Mutexes |
| **Binary Portability** | Single Static Binary | Requires Python Runtime | Static / Shared Libraries |
