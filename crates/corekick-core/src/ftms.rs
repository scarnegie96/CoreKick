//! Bluetooth Fitness Machine Service (FTMS, 0x1826) and Heart Rate (0x180D) helpers.
//!
//! All multi-byte fields are little-endian. References: Bluetooth SIG
//! "Fitness Machine Service 1.0" §4.9 (Indoor Bike Data), §4.16 (Control Point),
//! §4.17 (Machine Status).

use serde::Serialize;

pub const FTMS_SERVICE: u16 = 0x1826;
pub const FTMS_FEATURE: u16 = 0x2ACC;
pub const INDOOR_BIKE_DATA: u16 = 0x2AD2;
pub const TRAINING_STATUS: u16 = 0x2AD3;
pub const SUPPORTED_RESISTANCE_RANGE: u16 = 0x2AD6;
pub const SUPPORTED_POWER_RANGE: u16 = 0x2AD8;
pub const CONTROL_POINT: u16 = 0x2AD9;
pub const MACHINE_STATUS: u16 = 0x2ADA;

pub const CYCLING_POWER_SERVICE: u16 = 0x1818;
pub const HEART_RATE_SERVICE: u16 = 0x180D;
pub const HEART_RATE_MEASUREMENT: u16 = 0x2A37;

/// 128-bit UUID string for a 16-bit SIG-assigned id.
pub fn uuid_string(short: u16) -> String {
    format!("0000{:04x}-0000-1000-8000-00805f9b34fb", short)
}

/// Extract the 16-bit short id from a full 128-bit SIG UUID string, if it is one.
pub fn short_id(uuid: &str) -> Option<u16> {
    let u = uuid.to_ascii_lowercase();
    if u.len() == 36 && u.starts_with("0000") && u.ends_with("-0000-1000-8000-00805f9b34fb") {
        u16::from_str_radix(&u[4..8], 16).ok()
    } else {
        None
    }
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn new(b: &'a [u8]) -> Self {
        Reader { b, i: 0 }
    }
    fn u8(&mut self) -> Option<u8> {
        let v = *self.b.get(self.i)?;
        self.i += 1;
        Some(v)
    }
    fn u16(&mut self) -> Option<u16> {
        let lo = *self.b.get(self.i)? as u16;
        let hi = *self.b.get(self.i + 1)? as u16;
        self.i += 2;
        Some(lo | (hi << 8))
    }
    fn i16(&mut self) -> Option<i16> {
        self.u16().map(|v| v as i16)
    }
    fn u24(&mut self) -> Option<u32> {
        let a = *self.b.get(self.i)? as u32;
        let b = *self.b.get(self.i + 1)? as u32;
        let c = *self.b.get(self.i + 2)? as u32;
        self.i += 3;
        Some(a | (b << 8) | (c << 16))
    }
}

/// Indoor Bike Data (0x2AD2). Every field is optional: trainers (Wahoo included)
/// may split one logical record over several notifications using the
/// "More Data" flag, so consumers should merge only the fields present.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct IndoorBikeData {
    pub speed_kmh: Option<f64>,
    pub avg_speed_kmh: Option<f64>,
    pub cadence_rpm: Option<f64>,
    pub avg_cadence_rpm: Option<f64>,
    pub total_distance_m: Option<u32>,
    pub resistance_level: Option<i16>,
    pub power_w: Option<i16>,
    pub avg_power_w: Option<i16>,
    pub total_energy_kcal: Option<u16>,
    pub heart_rate: Option<u8>,
    pub elapsed_s: Option<u16>,
    pub remaining_s: Option<u16>,
}

pub fn parse_indoor_bike_data(b: &[u8]) -> Option<IndoorBikeData> {
    let mut r = Reader::new(b);
    let flags = r.u16()?;
    let mut d = IndoorBikeData::default();
    // Bit 0 "More Data": when 0, Instantaneous Speed IS present.
    if flags & 0x0001 == 0 {
        d.speed_kmh = r.u16().map(|v| v as f64 * 0.01);
    }
    if flags & 0x0002 != 0 {
        d.avg_speed_kmh = r.u16().map(|v| v as f64 * 0.01);
    }
    if flags & 0x0004 != 0 {
        d.cadence_rpm = r.u16().map(|v| v as f64 * 0.5);
    }
    if flags & 0x0008 != 0 {
        d.avg_cadence_rpm = r.u16().map(|v| v as f64 * 0.5);
    }
    if flags & 0x0010 != 0 {
        d.total_distance_m = r.u24();
    }
    if flags & 0x0020 != 0 {
        d.resistance_level = r.i16();
    }
    if flags & 0x0040 != 0 {
        d.power_w = r.i16();
    }
    if flags & 0x0080 != 0 {
        d.avg_power_w = r.i16();
    }
    if flags & 0x0100 != 0 {
        d.total_energy_kcal = r.u16();
        let _per_hour = r.u16();
        let _per_min = r.u8();
    }
    if flags & 0x0200 != 0 {
        d.heart_rate = r.u8();
    }
    if flags & 0x0400 != 0 {
        let _met = r.u8();
    }
    if flags & 0x0800 != 0 {
        d.elapsed_s = r.u16();
    }
    if flags & 0x1000 != 0 {
        d.remaining_s = r.u16();
    }
    Some(d)
}

/// FTMS Control Point op codes.
pub mod op {
    pub const REQUEST_CONTROL: u8 = 0x00;
    pub const RESET: u8 = 0x01;
    pub const SET_TARGET_RESISTANCE: u8 = 0x04;
    pub const SET_TARGET_POWER: u8 = 0x05;
    pub const START_OR_RESUME: u8 = 0x07;
    pub const STOP_OR_PAUSE: u8 = 0x08;
    pub const SET_INDOOR_BIKE_SIMULATION: u8 = 0x11;
    pub const RESPONSE_CODE: u8 = 0x80;
}

pub fn cmd_request_control() -> Vec<u8> {
    vec![op::REQUEST_CONTROL]
}

pub fn cmd_reset() -> Vec<u8> {
    vec![op::RESET]
}

pub fn cmd_start() -> Vec<u8> {
    vec![op::START_OR_RESUME]
}

/// 0x08 with parameter 0x01 = stop, 0x02 = pause.
pub fn cmd_stop(pause: bool) -> Vec<u8> {
    vec![op::STOP_OR_PAUSE, if pause { 0x02 } else { 0x01 }]
}

/// ERG mode: target power in watts (sint16, 1 W resolution).
pub fn cmd_target_power(watts: f64) -> Vec<u8> {
    let w = watts.round().clamp(0.0, i16::MAX as f64) as i16;
    let b = w.to_le_bytes();
    vec![op::SET_TARGET_POWER, b[0], b[1]]
}

/// Level mode: unitless resistance, 0.1 resolution (uint8 per FTMS 1.0).
pub fn cmd_target_resistance(level: f64) -> Vec<u8> {
    vec![op::SET_TARGET_RESISTANCE, (level * 10.0).round().clamp(0.0, 255.0) as u8]
}

/// SIM mode: Set Indoor Bike Simulation Parameters (0x11).
/// * wind speed  sint16, 0.001 m/s
/// * grade       sint16, 0.01 %
/// * crr         uint8,  0.0001
/// * cw          uint8,  0.01 kg/m   (cw = 0.5 · ρ · CdA)
pub fn cmd_simulation(grade_pct: f64, wind_mps: f64, crr: f64, cw_kg_per_m: f64) -> Vec<u8> {
    let wind = (wind_mps * 1000.0).round().clamp(i16::MIN as f64, i16::MAX as f64) as i16;
    let grade = (grade_pct * 100.0).round().clamp(i16::MIN as f64, i16::MAX as f64) as i16;
    let crr = (crr * 10_000.0).round().clamp(0.0, 255.0) as u8;
    let cw = (cw_kg_per_m * 100.0).round().clamp(0.0, 255.0) as u8;
    let w = wind.to_le_bytes();
    let g = grade.to_le_bytes();
    vec![op::SET_INDOOR_BIKE_SIMULATION, w[0], w[1], g[0], g[1], crr, cw]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultCode {
    Success,
    NotSupported,
    InvalidParameter,
    Failed,
    ControlNotPermitted,
    Unknown(u8),
}

impl From<u8> for ResultCode {
    fn from(v: u8) -> Self {
        match v {
            0x01 => ResultCode::Success,
            0x02 => ResultCode::NotSupported,
            0x03 => ResultCode::InvalidParameter,
            0x04 => ResultCode::Failed,
            0x05 => ResultCode::ControlNotPermitted,
            x => ResultCode::Unknown(x),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlResponse {
    pub request_op: u8,
    pub result: ResultCode,
}

pub fn parse_control_response(b: &[u8]) -> Option<ControlResponse> {
    if b.len() >= 3 && b[0] == op::RESPONSE_CODE {
        Some(ControlResponse { request_op: b[1], result: ResultCode::from(b[2]) })
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MachineStatus {
    Reset,
    StoppedOrPaused,
    StartedOrResumed,
    TargetPowerChanged(i16),
    SimulationChanged { grade_pct: f64 },
    ControlPermissionLost,
    Other(u8),
}

pub fn parse_machine_status(b: &[u8]) -> Option<MachineStatus> {
    let mut r = Reader::new(b);
    let code = r.u8()?;
    Some(match code {
        0x01 => MachineStatus::Reset,
        0x02 => MachineStatus::StoppedOrPaused,
        0x04 => MachineStatus::StartedOrResumed,
        0x08 => MachineStatus::TargetPowerChanged(r.i16().unwrap_or(0)),
        0x12 => {
            let _wind = r.i16();
            let g = r.i16().unwrap_or(0);
            MachineStatus::SimulationChanged { grade_pct: g as f64 / 100.0 }
        }
        0xFF => MachineStatus::ControlPermissionLost,
        x => MachineStatus::Other(x),
    })
}

/// Supported Power Range (0x2AD8): min, max (sint16 W), increment (uint16 W).
pub fn parse_power_range(b: &[u8]) -> Option<(i16, i16, u16)> {
    let mut r = Reader::new(b);
    Some((r.i16()?, r.i16()?, r.u16()?))
}

/// Heart Rate Measurement (0x2A37).
pub fn parse_heart_rate(b: &[u8]) -> Option<u16> {
    let mut r = Reader::new(b);
    let flags = r.u8()?;
    if flags & 0x01 != 0 {
        r.u16()
    } else {
        r.u8().map(|v| v as u16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_speed_cadence_power() {
        // flags 0x0044: speed present (bit0=0), cadence (bit2), power (bit6)
        // speed 3000 (30.00 km/h), cadence 180 (90 rpm), power 250 W
        let b = [0x44, 0x00, 0xB8, 0x0B, 0xB4, 0x00, 0xFA, 0x00];
        let d = parse_indoor_bike_data(&b).unwrap();
        assert_eq!(d.speed_kmh, Some(30.0));
        assert_eq!(d.cadence_rpm, Some(90.0));
        assert_eq!(d.power_w, Some(250));
        assert_eq!(d.heart_rate, None);
    }

    #[test]
    fn parses_more_data_packet_without_speed() {
        // More Data set, power + heart rate only
        let b = [0x41, 0x02, 0x2C, 0x01, 0x8C];
        let d = parse_indoor_bike_data(&b).unwrap();
        assert_eq!(d.speed_kmh, None);
        assert_eq!(d.power_w, Some(300));
        assert_eq!(d.heart_rate, Some(140));
    }

    #[test]
    fn truncated_packets_do_not_panic() {
        let b = [0x44, 0x00, 0xB8];
        let d = parse_indoor_bike_data(&b).unwrap();
        assert_eq!(d.speed_kmh, None);
        assert!(parse_indoor_bike_data(&[0x01]).is_none());
    }

    #[test]
    fn encodes_simulation() {
        // 5.25 % grade, no wind, crr .004, cw .51
        let c = cmd_simulation(5.25, 0.0, 0.004, 0.51);
        assert_eq!(c, vec![0x11, 0, 0, 0x0D, 0x02, 40, 51]);
        let neg = cmd_simulation(-2.0, 0.0, 0.004, 0.51);
        assert_eq!(i16::from_le_bytes([neg[3], neg[4]]), -200);
    }

    #[test]
    fn encodes_power_and_responses() {
        assert_eq!(cmd_target_power(250.4), vec![0x05, 0xFA, 0x00]);
        let r = parse_control_response(&[0x80, 0x05, 0x01]).unwrap();
        assert_eq!(r.request_op, op::SET_TARGET_POWER);
        assert_eq!(r.result, ResultCode::Success);
        assert_eq!(parse_control_response(&[0x80, 0x11, 0x05]).unwrap().result, ResultCode::ControlNotPermitted);
    }

    #[test]
    fn heart_rate_formats() {
        assert_eq!(parse_heart_rate(&[0x00, 72]), Some(72));
        assert_eq!(parse_heart_rate(&[0x01, 0x2C, 0x01]), Some(300));
    }

    #[test]
    fn uuid_helpers() {
        let u = uuid_string(FTMS_SERVICE);
        assert_eq!(u, "00001826-0000-1000-8000-00805f9b34fb");
        assert_eq!(short_id(&u), Some(0x1826));
        assert_eq!(short_id("a026e005-0a7d-4ab3-97fa-f1500f9feb8b"), None);
    }

    #[test]
    fn machine_status() {
        assert_eq!(parse_machine_status(&[0xFF]), Some(MachineStatus::ControlPermissionLost));
        assert_eq!(
            parse_machine_status(&[0x12, 0, 0, 0xF4, 0x01, 40, 51]),
            Some(MachineStatus::SimulationChanged { grade_pct: 5.0 })
        );
    }
}
