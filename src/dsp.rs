// ==============================================================================
// Module: dsp.rs
// Purpose: Real-Time Digital Signal Processing (DSP) & Electrical Metrics Core
// Target: True RMS, Peak-to-Peak Ripple, EMA Filter, and Dynamic Impedance
// Author: Patrick Isles (https://github.com/patrick-isles)
// License: MIT
// ==============================================================================

/// Circular buffer for sliding-window True RMS and Peak-to-Peak calculation
#[derive(Debug, Clone)]
pub struct SlidingWindow {
    samples: Vec<f64>,
    capacity: usize,
    index: usize,
    count: usize,
}

impl SlidingWindow {
    pub fn new(capacity: usize) -> Self {
        Self {
            samples: vec![0.0; capacity],
            capacity,
            index: 0,
            count: 0,
        }
    }

    /// Push a new voltage sample into the ring buffer
    pub fn push(&mut self, sample: f64) {
        self.samples[self.index] = sample;
        self.index = (self.index + 1) % self.capacity;
        if self.count < self.capacity {
            self.count += 1;
        }
    }

    /// Calculate True Root Mean Square (RMS): sqrt(1/N * sum(V_i^2))
    pub fn true_rms(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        let sum_sq: f64 = self.samples[..self.count].iter().map(|&v| v * v).sum();
        (sum_sq / self.count as f64).sqrt()
    }

    /// Calculate AC Peak-to-Peak Ripple: V_max - V_min
    pub fn peak_to_peak(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        let mut min_val = f64::MAX;
        let mut max_val = f64::MIN;
        for &v in &self.samples[..self.count] {
            if v < min_val { min_val = v; }
            if v > max_val { max_val = v; }
        }
        (max_val - min_val).max(0.0)
    }

    /// Get current sample count
    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

/// Exponential Moving Average (EMA) Low-Pass Filter
#[derive(Debug, Clone)]
pub struct EmaFilter {
    alpha: f64,
    current_value: Option<f64>,
}

impl EmaFilter {
    pub fn new(alpha: f64) -> Self {
        let clamped_alpha = alpha.clamp(0.001, 1.0);
        Self {
            alpha: clamped_alpha,
            current_value: None,
        }
    }

    /// Update filter with new raw measurement: y[n] = alpha * x[n] + (1 - alpha) * y[n-1]
    pub fn update(&mut self, measurement: f64) -> f64 {
        match self.current_value {
            Some(prev) => {
                let filtered = self.alpha * measurement + (1.0 - self.alpha) * prev;
                self.current_value = Some(filtered);
                filtered
            }
            None => {
                self.current_value = Some(measurement);
                measurement
            }
        }
    }

    pub fn value(&self) -> f64 {
        self.current_value.unwrap_or(0.0)
    }
}

/// Comprehensive Electrical DSP Pipeline
#[derive(Debug, Clone)]
pub struct SignalPipeline {
    pub v_window: SlidingWindow,
    pub i_window: SlidingWindow,
    pub v_filter: EmaFilter,
    pub i_filter: EmaFilter,
    pub total_energy_joules: f64,
    pub last_timestamp_secs: Option<f64>,
}

impl SignalPipeline {
    pub fn new(window_size: usize, ema_alpha: f64) -> Self {
        Self {
            v_window: SlidingWindow::new(window_size),
            i_window: SlidingWindow::new(window_size),
            v_filter: EmaFilter::new(ema_alpha),
            i_filter: EmaFilter::new(ema_alpha),
            total_energy_joules: 0.0,
            last_timestamp_secs: None,
        }
    }

    /// Process a new measurement tick, updating filters, RMS windows, and energy accumulation
    pub fn process_sample(&mut self, voltage_v: f64, current_a: f64, timestamp_secs: f64) {
        // Update RMS sliding windows
        self.v_window.push(voltage_v);
        self.i_window.push(current_a);

        // Update low-pass EMA filters
        self.v_filter.update(voltage_v);
        self.i_filter.update(current_a);

        // Integrate energy: E = P * dt (Joules)
        if let Some(prev_t) = self.last_timestamp_secs {
            let dt = timestamp_secs - prev_t;
            if dt > 0.0 && dt < 1.0 { // Guard against clock jumps or gaps
                let power = voltage_v * current_a;
                self.total_energy_joules += power * dt;
            }
        }
        self.last_timestamp_secs = Some(timestamp_secs);
    }

    /// True RMS Voltage
    #[inline]
    pub fn voltage_rms(&self) -> f64 {
        self.v_window.true_rms()
    }

    /// Peak-to-Peak Voltage Ripple (Noise)
    #[inline]
    pub fn voltage_ripple_pk_pk(&self) -> f64 {
        self.v_window.peak_to_peak()
    }

    /// Filtered Voltage
    #[inline]
    pub fn filtered_voltage(&self) -> f64 {
        self.v_filter.value()
    }

    /// Filtered Current
    #[inline]
    pub fn filtered_current(&self) -> f64 {
        self.i_filter.value()
    }

    /// Accumulated Energy in Watt-Hours (Wh)
    #[inline]
    pub fn energy_watt_hours(&self) -> f64 {
        self.total_energy_joules / 3600.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_true_rms_pure_dc() {
        let mut window = SlidingWindow::new(32);
        for _ in 0..32 {
            window.push(5.0);
        }
        assert!((window.true_rms() - 5.0).abs() < 1e-6);
        assert!((window.peak_to_peak() - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_true_rms_ac_square_wave() {
        // Square wave between +5V and -5V has RMS of exactly 5.0V
        let mut window = SlidingWindow::new(10);
        for _ in 0..5 { window.push(5.0); }
        for _ in 0..5 { window.push(-5.0); }
        assert!((window.true_rms() - 5.0).abs() < 1e-6);
        assert!((window.peak_to_peak() - 10.0).abs() < 1e-6);
    }

    #[test]
    fn test_ema_filter_smoothing() {
        let mut ema = EmaFilter::new(0.1);
        let first = ema.update(10.0);
        assert_eq!(first, 10.0);

        // Step change to 20.0 with alpha=0.1
        let second = ema.update(20.0);
        // expected: 0.1 * 20.0 + 0.9 * 10.0 = 2.0 + 9.0 = 11.0
        assert!((second - 11.0).abs() < 1e-6);
    }

    #[test]
    fn test_energy_accumulation() {
        let mut pipeline = SignalPipeline::new(16, 0.2);
        // 5V at 1A = 5 Watts for 2 seconds -> 10 Joules
        pipeline.process_sample(5.0, 1.0, 0.0);
        pipeline.process_sample(5.0, 1.0, 0.5);
        pipeline.process_sample(5.0, 1.0, 1.0);
        pipeline.process_sample(5.0, 1.0, 1.5);
        pipeline.process_sample(5.0, 1.0, 2.0);

        assert!((pipeline.total_energy_joules - 10.0).abs() < 1e-3);
        assert!((pipeline.energy_watt_hours() - (10.0 / 3600.0)).abs() < 1e-6);
    }
}
