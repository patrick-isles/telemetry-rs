// ==============================================================================
// Module: protocol.rs
// Purpose: Zero-Allocation Binary Telemetry Frame Parser & CRC-16-CCITT Engine
// Target: 19-Byte Embedded Serial Frames (0xAA55 Framing)
// Author: Patrick Isles (https://github.com/patrick-isles)
// License: MIT
// ==============================================================================

pub const SYNC_BYTE_0: u8 = 0xAA;
pub const SYNC_BYTE_1: u8 = 0x55;
pub const EXPECTED_PAYLOAD_LEN: u8 = 14;
pub const FULL_FRAME_LEN: usize = 19; // 2 SYNC + 1 LEN + 14 PAYLOAD + 2 CRC

/// Standard CRC-16-CCITT calculation (Polynomial 0x1021, Initial value 0xFFFF)
#[inline]
pub fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            if (crc & 0x8000) != 0 {
                crc = ((crc << 1) ^ 0x1021) & 0xFFFF;
            } else {
                crc = (crc << 1) & 0xFFFF;
            }
        }
    }
    crc
}

/// Decoded hardware telemetry measurement
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TelemetryPacket {
    pub voltage_mv: u16,
    pub current_ma: u16,
    pub raw_adc: u16,
    pub temp_cdeg: i16,
    pub gpio_byte: u8,
    pub fault_byte: u8,
    pub sequence_id: u32,
    pub crc16: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    BufferTooShort,
    InvalidSyncHeader,
    InvalidPayloadLength(u8),
    CrcMismatch { expected: u16, actual: u16 },
}

impl TelemetryPacket {
    /// Zero-copy parser: decodes a 19-byte slice with zero heap allocations
    pub fn parse(buf: &[u8]) -> Result<Self, ParseError> {
        if buf.len() < FULL_FRAME_LEN {
            return Err(ParseError::BufferTooShort);
        }

        if buf[0] != SYNC_BYTE_0 || buf[1] != SYNC_BYTE_1 {
            return Err(ParseError::InvalidSyncHeader);
        }

        let payload_len = buf[2];
        if payload_len != EXPECTED_PAYLOAD_LEN {
            return Err(ParseError::InvalidPayloadLength(payload_len));
        }

        // Payload is bytes 3..17 (14 bytes)
        let payload = &buf[3..17];
        let rx_crc = u16::from_be_bytes([buf[17], buf[18]]);
        let calc_crc = crc16_ccitt(payload);

        if rx_crc != calc_crc {
            return Err(ParseError::CrcMismatch {
                expected: calc_crc,
                actual: rx_crc,
            });
        }

        // Extract fields using big-endian byte decoding
        let voltage_mv = u16::from_be_bytes([payload[0], payload[1]]);
        let current_ma = u16::from_be_bytes([payload[2], payload[3]]);
        let raw_adc = u16::from_be_bytes([payload[4], payload[5]]);
        let temp_cdeg = i16::from_be_bytes([payload[6], payload[7]]);
        let gpio_byte = payload[8];
        let fault_byte = payload[9];
        let sequence_id = u32::from_be_bytes([payload[10], payload[11], payload[12], payload[13]]);

        Ok(Self {
            voltage_mv,
            current_ma,
            raw_adc,
            temp_cdeg,
            gpio_byte,
            fault_byte,
            sequence_id,
            crc16: rx_crc,
        })
    }

    /// Convert raw millivolts to Volts
    #[inline]
    pub fn voltage_v(&self) -> f64 {
        self.voltage_mv as f64 / 1000.0
    }

    /// Convert raw milliamperes to Amperes
    #[inline]
    pub fn current_a(&self) -> f64 {
        self.current_ma as f64 / 1000.0
    }

    /// Calculate instantaneous power dissipation: P = V * I (Watts)
    #[inline]
    pub fn power_w(&self) -> f64 {
        self.voltage_v() * self.current_a()
    }

    /// Calculate dynamic load impedance: R = V / I (Ohms)
    #[inline]
    pub fn resistance_ohms(&self) -> f64 {
        let i = self.current_a();
        if i > 0.0001 {
            self.voltage_v() / i
        } else {
            0.0
        }
    }

    /// Convert centi-degrees Celsius to degrees Celsius
    #[inline]
    pub fn temperature_c(&self) -> f64 {
        self.temp_cdeg as f64 / 100.0
    }

    /// Query digital logic state of pin 0..7
    #[inline]
    pub fn is_pin_high(&self, pin: u8) -> bool {
        if pin < 8 {
            (self.gpio_byte & (1 << pin)) != 0
        } else {
            false
        }
    }

    /// Check if over-voltage fault bit (bit 0) is tripped
    #[inline]
    pub fn is_overvoltage(&self) -> bool {
        (self.fault_byte & 0x01) != 0
    }

    /// Check if brownout fault bit (bit 1) is tripped
    #[inline]
    pub fn is_brownout(&self) -> bool {
        (self.fault_byte & 0x02) != 0
    }

    /// Check if over-temperature fault bit (bit 2) is tripped
    #[inline]
    pub fn is_overtemp(&self) -> bool {
        (self.fault_byte & 0x04) != 0
    }
}

/// Search a byte buffer for valid frames, extracting packets and draining parsed bytes
pub fn extract_frame_from_stream(buf: &mut Vec<u8>) -> Option<Result<TelemetryPacket, ParseError>> {
    while buf.len() >= 2 {
        // Look for sync header [0xAA, 0x55]
        if let Some(pos) = buf.windows(2).position(|w| w == [SYNC_BYTE_0, SYNC_BYTE_1]) {
            if pos > 0 {
                // Drop leading garbage bytes
                buf.drain(0..pos);
            }

            if buf.len() < FULL_FRAME_LEN {
                // Wait for more bytes to complete full frame
                return None;
            }

            match TelemetryPacket::parse(&buf[..FULL_FRAME_LEN]) {
                Ok(packet) => {
                    buf.drain(0..FULL_FRAME_LEN);
                    return Some(Ok(packet));
                }
                Err(err) => {
                    // Drop corrupted sync header and re-hunt
                    buf.drain(0..2);
                    return Some(Err(err));
                }
            }
        } else {
            // No sync header found; keep only the last byte in case it's 0xAA
            let keep = if buf.last() == Some(&SYNC_BYTE_0) { 1 } else { 0 };
            let drain_len = buf.len() - keep;
            buf.drain(0..drain_len);
            return None;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc16_standard_ccitt_vector() {
        // Known CCITT test vector: ASCII "123456789" produces 0x29B1
        let vector = b"123456789";
        assert_eq!(crc16_ccitt(vector), 0x29B1);
    }

    #[test]
    fn test_valid_frame_parsing_and_metrics() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&5000u16.to_be_bytes()); // 5.000 V
        payload.extend_from_slice(&250u16.to_be_bytes());  // 0.250 A
        payload.extend_from_slice(&4095u16.to_be_bytes()); // 12-bit ADC max
        payload.extend_from_slice(&2540i16.to_be_bytes()); // 25.40 C
        payload.push(0b00000101);                          // Pin 0 and Pin 2 HIGH
        payload.push(0x00);                                // Normal faults
        payload.extend_from_slice(&42u32.to_be_bytes());   // Seq 42

        let crc = crc16_ccitt(&payload);

        let mut frame = Vec::new();
        frame.push(SYNC_BYTE_0);
        frame.push(SYNC_BYTE_1);
        frame.push(EXPECTED_PAYLOAD_LEN);
        frame.extend_from_slice(&payload);
        frame.extend_from_slice(&crc.to_be_bytes());

        assert_eq!(frame.len(), FULL_FRAME_LEN);

        let packet = TelemetryPacket::parse(&frame).expect("Failed to parse valid frame");
        assert_eq!(packet.voltage_mv, 5000);
        assert_eq!(packet.current_ma, 250);
        assert_eq!(packet.raw_adc, 4095);
        assert_eq!(packet.temp_cdeg, 2540);
        assert_eq!(packet.gpio_byte, 0b00000101);
        assert_eq!(packet.sequence_id, 42);
        assert_eq!(packet.crc16, crc);

        // Verify electrical math
        assert!((packet.voltage_v() - 5.000).abs() < 1e-6);
        assert!((packet.current_a() - 0.250).abs() < 1e-6);
        assert!((packet.power_w() - 1.250).abs() < 1e-6);
        assert!((packet.resistance_ohms() - 20.000).abs() < 1e-6);
        assert!((packet.temperature_c() - 25.40).abs() < 1e-6);

        // Verify GPIO pins
        assert!(packet.is_pin_high(0));
        assert!(!packet.is_pin_high(1));
        assert!(packet.is_pin_high(2));
    }

    #[test]
    fn test_corrupted_crc_detection() {
        let mut frame = vec![0u8; FULL_FRAME_LEN];
        frame[0] = SYNC_BYTE_0;
        frame[1] = SYNC_BYTE_1;
        frame[2] = EXPECTED_PAYLOAD_LEN;
        // All zeros payload -> CRC is 0x0F47, but we pass 0xFFFF
        frame[17] = 0xFF;
        frame[18] = 0xFF;

        let result = TelemetryPacket::parse(&frame);
        assert!(matches!(result, Err(ParseError::CrcMismatch { .. })));
    }

    #[test]
    fn test_stream_resynchronization_with_noise() {
        let mut stream = vec![0x12, 0x34, 0x56, 0xAA, 0x00, 0xDE, 0xAD];
        
        // Append valid frame
        let payload = vec![0u8; 14];
        let crc = crc16_ccitt(&payload);
        stream.push(SYNC_BYTE_0);
        stream.push(SYNC_BYTE_1);
        stream.push(EXPECTED_PAYLOAD_LEN);
        stream.extend_from_slice(&payload);
        stream.extend_from_slice(&crc.to_be_bytes());

        let res = extract_frame_from_stream(&mut stream);
        assert!(res.is_some());
        assert!(res.unwrap().is_ok());
    }
}
