//! Garmin FIT activity encoder.
//!
//! FIT is the format Strava, Garmin Connect, TrainingPeaks and intervals.icu
//! prefer. We write a standard activity file:
//! `file_id → event(start) → record×N → event(stop) → lap×N → session → activity`.
//! Sport is cycling with sub-sport *virtual activity*, so Strava files the ride
//! as a **Virtual Ride** (no GPS, power/HR/cadence/speed/distance/altitude).

use chrono::{DateTime, FixedOffset, Utc};

use crate::model::{RideRecord, Split};

/// Seconds between the Unix epoch and the FIT epoch (1989-12-31T00:00:00Z).
pub const FIT_EPOCH_OFFSET: i64 = 631_065_600;

const PROFILE_VERSION: u16 = 2132;

// Base types
const ENUM: u8 = 0x00;
const UINT8: u8 = 0x02;
const UINT16: u8 = 0x84;
const UINT32: u8 = 0x86;
const UINT32Z: u8 = 0x8C;

const CRC_TABLE: [u16; 16] = [
    0x0000, 0xCC01, 0xD801, 0x1400, 0xF001, 0x3C00, 0x2800, 0xE401, 0xA001, 0x6C00, 0x7800, 0xB401, 0x5000, 0x9C01, 0x8801, 0x4400,
];

pub fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &b in data {
        let mut tmp = CRC_TABLE[(crc & 0xF) as usize];
        crc = (crc >> 4) & 0x0FFF;
        crc = crc ^ tmp ^ CRC_TABLE[(b & 0xF) as usize];
        tmp = CRC_TABLE[(crc & 0xF) as usize];
        crc = (crc >> 4) & 0x0FFF;
        crc = crc ^ tmp ^ CRC_TABLE[((b >> 4) & 0xF) as usize];
    }
    crc
}

/// A field value. Invalid (missing) values use the FIT "invalid" sentinel.
#[derive(Clone, Copy)]
enum V {
    E(u8),
    U8(Option<u8>),
    U16(Option<u16>),
    U32(Option<u32>),
    U32Z(u32),
}

impl V {
    fn base(&self) -> (u8, u8) {
        match self {
            V::E(_) => (ENUM, 1),
            V::U8(_) => (UINT8, 1),
            V::U16(_) => (UINT16, 2),
            V::U32(_) => (UINT32, 4),
            V::U32Z(_) => (UINT32Z, 4),
        }
    }
    fn write(&self, out: &mut Vec<u8>) {
        match *self {
            V::E(v) => out.push(v),
            V::U8(v) => out.push(v.unwrap_or(0xFF)),
            V::U16(v) => out.extend_from_slice(&v.unwrap_or(0xFFFF).to_le_bytes()),
            V::U32(v) => out.extend_from_slice(&v.unwrap_or(0xFFFF_FFFF).to_le_bytes()),
            V::U32Z(v) => out.extend_from_slice(&v.to_le_bytes()),
        }
    }
}

struct Writer {
    data: Vec<u8>,
    /// local message type → (global number, field signature)
    defs: Vec<(u16, Vec<(u8, u8, u8)>)>,
}

impl Writer {
    fn new() -> Self {
        Writer { data: Vec::new(), defs: Vec::new() }
    }

    /// Write a message, emitting a definition first if this layout is new.
    fn msg(&mut self, global: u16, fields: &[(u8, V)]) {
        let sig: Vec<(u8, u8, u8)> = fields
            .iter()
            .map(|(n, v)| {
                let (bt, sz) = v.base();
                (*n, sz, bt)
            })
            .collect();
        let local = match self.defs.iter().position(|(g, s)| *g == global && *s == sig) {
            Some(i) => i as u8,
            None => {
                // Up to 16 local types; recycle slot by global number if needed.
                let slot = if self.defs.len() < 16 { self.defs.len() } else { (global % 16) as usize };
                if slot == self.defs.len() {
                    self.defs.push((global, sig.clone()));
                } else {
                    self.defs[slot] = (global, sig.clone());
                }
                let slot = slot as u8;
                self.data.push(0x40 | slot);
                self.data.push(0); // reserved
                self.data.push(0); // little endian
                self.data.extend_from_slice(&global.to_le_bytes());
                self.data.push(sig.len() as u8);
                for (n, sz, bt) in &sig {
                    self.data.extend_from_slice(&[*n, *sz, *bt]);
                }
                slot
            }
        };
        self.data.push(local);
        for (_, v) in fields {
            v.write(&mut self.data);
        }
    }

    fn finish(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.data.len() + 16);
        let mut hdr = vec![14u8, 0x20];
        hdr.extend_from_slice(&PROFILE_VERSION.to_le_bytes());
        hdr.extend_from_slice(&(self.data.len() as u32).to_le_bytes());
        hdr.extend_from_slice(b".FIT");
        let hcrc = crc16(&hdr);
        hdr.extend_from_slice(&hcrc.to_le_bytes());
        out.extend_from_slice(&hdr);
        out.extend_from_slice(&self.data);
        let crc = crc16(&out);
        out.extend_from_slice(&crc.to_le_bytes());
        out
    }
}

fn u8o(x: f64) -> Option<u8> {
    if x.is_finite() && x > 0.0 {
        Some(x.round().clamp(0.0, 254.0) as u8)
    } else {
        None
    }
}
fn u16s(x: f64, scale: f64) -> Option<u16> {
    if x.is_finite() && x >= 0.0 {
        Some((x * scale).round().clamp(0.0, 65_534.0) as u16)
    } else {
        None
    }
}
fn u32s(x: f64, scale: f64) -> Option<u32> {
    if x.is_finite() && x >= 0.0 {
        Some((x * scale).round().clamp(0.0, 4_294_967_294.0) as u32)
    } else {
        None
    }
}

// message numbers
const FILE_ID: u16 = 0;
const SESSION: u16 = 18;
const LAP: u16 = 19;
const RECORD: u16 = 20;
const EVENT: u16 = 21;
const DEVICE_INFO: u16 = 23;
const ACTIVITY: u16 = 34;

fn lap_fields(ts: u32, start: u32, sp: &Split, ftp: f64) -> Vec<(u8, V)> {
    let _ = ftp;
    vec![
        (253, V::U32(Some(ts))),
        (0, V::E(9)),  // event: lap
        (1, V::E(1)),  // event_type: stop
        (2, V::U32(Some(start))),
        (7, V::U32(u32s(sp.duration_s, 1000.0))),
        (8, V::U32(u32s(sp.duration_s, 1000.0))),
        (9, V::U32(u32s(sp.distance_m, 100.0))),
        (13, V::U16(u16s(sp.avg_speed_kmh / 3.6, 1000.0))),
        (15, V::U8(sp.avg_hr.and_then(u8o))),
        (17, V::U8(u8o(sp.avg_cadence))),
        (19, V::U16(u16s(sp.avg_power, 1.0))),
        (20, V::U16(u16s(sp.max_power, 1.0))),
        (21, V::U16(u16s(sp.elevation_gain_m, 1.0))),
        (25, V::E(2)),  // sport: cycling
        (39, V::E(58)), // sub_sport: virtual_activity
        (33, V::U16(u16s(sp.np, 1.0))),
        (41, V::U32(u32s(sp.avg_power * sp.duration_s, 1.0))),
    ]
}

/// Encode a ride as a FIT activity file.
pub fn to_fit(r: &RideRecord) -> Vec<u8> {
    let s = &r.summary;
    let sm = &r.samples;
    let start_unix = DateTime::<FixedOffset>::parse_from_rfc3339(&s.started_at).map(|d| d.with_timezone(&Utc).timestamp()).unwrap_or_else(|_| Utc::now().timestamp());
    let fit_ts = |unix: i64| (unix - FIT_EPOCH_OFFSET).max(0) as u32;
    let t0 = fit_ts(start_unix);
    let n = sm.len();
    let t_end = t0 + n.max(1) as u32;
    let local_offset = DateTime::<FixedOffset>::parse_from_rfc3339(&s.started_at).map(|d| d.offset().local_minus_utc() as i64).unwrap_or(0);

    let mut w = Writer::new();
    w.msg(
        FILE_ID,
        &[
            (0, V::E(4)),                   // type: activity
            (1, V::U16(Some(255))),         // manufacturer: development
            (2, V::U16(Some(1))),           // product
            (3, V::U32Z(0x0C0E_4B1C)),      // serial number
            (4, V::U32(Some(t0))),          // time_created
        ],
    );
    w.msg(DEVICE_INFO, &[(253, V::U32(Some(t0))), (0, V::U8(Some(0))), (2, V::U16(Some(255))), (4, V::U16(Some(1))), (5, V::U16(Some(100)))]);
    w.msg(EVENT, &[(253, V::U32(Some(t0))), (0, V::E(0)), (1, V::E(0))]); // timer start

    for i in 0..n {
        let hr = sm.hr[i] as f64;
        w.msg(
            RECORD,
            &[
                (253, V::U32(Some(t0 + sm.t[i]))),
                (2, V::U16(u16s(sm.ele[i] as f64 + 500.0, 5.0))), // altitude: scale 5, offset 500
                (3, V::U8(if hr > 0.0 { u8o(hr) } else { None })),
                (4, V::U8(Some((sm.cadence[i] as f64).round().clamp(0.0, 254.0) as u8))),
                (5, V::U32(u32s(sm.distance[i] as f64, 100.0))),
                (6, V::U16(u16s(sm.speed[i] as f64, 1000.0))),
                (7, V::U16(u16s(sm.power[i] as f64, 1.0))),
            ],
        );
    }
    w.msg(EVENT, &[(253, V::U32(Some(t_end))), (0, V::E(0)), (1, V::E(4))]); // timer stop_all

    // Laps: the rider's manual laps if they tile the ride, otherwise one lap.
    let mut laps: Vec<Split> = s.splits.iter().filter(|x| x.kind == "lap").cloned().collect();
    laps.sort_by(|a, b| a.start_s.partial_cmp(&b.start_s).unwrap_or(std::cmp::Ordering::Equal));
    if laps.is_empty() {
        laps.push(Split {
            index: 1,
            kind: "lap".into(),
            name: "Lap 1".into(),
            start_s: 0.0,
            duration_s: s.elapsed_s.max(n as f64),
            distance_m: s.distance_m,
            elevation_gain_m: s.elevation_gain_m,
            avg_power: s.avg_power,
            max_power: s.max_power,
            np: s.np,
            avg_hr: s.avg_hr,
            avg_cadence: s.avg_cadence,
            avg_speed_kmh: s.avg_speed_mps * 3.6,
            avg_grade_pct: 0.0,
        });
    }
    for sp in &laps {
        let st = t0 + sp.start_s.round() as u32;
        let end = st + sp.duration_s.round() as u32;
        w.msg(LAP, &lap_fields(end, st, sp, s.ftp));
    }

    w.msg(
        SESSION,
        &[
            (253, V::U32(Some(t_end))),
            (0, V::E(8)), // event: session
            (1, V::E(1)), // stop
            (2, V::U32(Some(t0))),
            (5, V::E(2)),  // cycling
            (6, V::E(58)), // virtual activity
            (7, V::U32(u32s(s.elapsed_s, 1000.0))),
            (8, V::U32(u32s(s.elapsed_s, 1000.0))),
            (9, V::U32(u32s(s.distance_m, 100.0))),
            (11, V::U16(u16s(s.calories_kcal, 1.0))),
            (14, V::U16(u16s(s.avg_speed_mps, 1000.0))),
            (15, V::U16(u16s(s.max_speed_mps, 1000.0))),
            (16, V::U8(s.avg_hr.and_then(u8o))),
            (17, V::U8(s.max_hr.and_then(u8o))),
            (18, V::U8(u8o(s.avg_cadence))),
            (19, V::U8(u8o(s.max_cadence))),
            (20, V::U16(u16s(s.avg_power, 1.0))),
            (21, V::U16(u16s(s.max_power, 1.0))),
            (22, V::U16(u16s(s.elevation_gain_m, 1.0))),
            (25, V::U16(Some(0))),                     // first_lap_index
            (26, V::U16(Some(laps.len() as u16))),     // num_laps
            (34, V::U16(u16s(s.np, 1.0))),
            (35, V::U16(u16s(s.tss, 10.0))),
            (36, V::U16(u16s(s.intensity_factor, 1000.0))),
            (45, V::U16(u16s(s.ftp, 1.0))),
            (48, V::U32(u32s(s.work_kj * 1000.0, 1.0))),
        ],
    );
    w.msg(
        ACTIVITY,
        &[
            (253, V::U32(Some(t_end))),
            (0, V::U32(u32s(s.elapsed_s, 1000.0))),
            (1, V::U16(Some(1))),
            (2, V::E(0)),  // manual
            (3, V::E(26)), // event: activity
            (4, V::E(1)),  // stop
            (5, V::U32(Some((t_end as i64 + local_offset).max(0) as u32))),
        ],
    );
    w.finish()
}

// ---------------------------------------------------------------------------
// Minimal decoder (used by tests and for sanity checks).
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct Decoded {
    pub counts: std::collections::BTreeMap<u16, usize>,
    pub records: Vec<std::collections::BTreeMap<u8, u64>>,
    pub sessions: Vec<std::collections::BTreeMap<u8, u64>>,
}

pub fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    if bytes.len() < 16 || &bytes[8..12] != b".FIT" {
        return Err("not a FIT file".into());
    }
    let hsize = bytes[0] as usize;
    let dsize = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    if bytes.len() != hsize + dsize + 2 {
        return Err(format!("size mismatch {} vs {}", bytes.len(), hsize + dsize + 2));
    }
    if crc16(&bytes[..hsize + dsize]) != u16::from_le_bytes([bytes[hsize + dsize], bytes[hsize + dsize + 1]]) {
        return Err("file CRC mismatch".into());
    }
    if hsize == 14 && crc16(&bytes[..12]) != u16::from_le_bytes([bytes[12], bytes[13]]) {
        return Err("header CRC mismatch".into());
    }
    let mut defs: std::collections::HashMap<u8, (u16, Vec<(u8, u8)>)> = Default::default();
    let mut out = Decoded::default();
    let mut i = hsize;
    let end = hsize + dsize;
    while i < end {
        let h = bytes[i];
        i += 1;
        if h & 0x80 != 0 {
            return Err("compressed headers not supported".into());
        }
        let local = h & 0x0F;
        if h & 0x40 != 0 {
            let global = u16::from_le_bytes([bytes[i + 2], bytes[i + 3]]);
            let nf = bytes[i + 4] as usize;
            i += 5;
            let mut f = Vec::new();
            for k in 0..nf {
                f.push((bytes[i + 3 * k], bytes[i + 3 * k + 1]));
            }
            i += 3 * nf;
            defs.insert(local, (global, f));
        } else {
            let (global, fields) = defs.get(&local).ok_or("data before definition")?.clone();
            let mut m = std::collections::BTreeMap::new();
            for (num, sz) in fields {
                let mut v = 0u64;
                for k in 0..sz as usize {
                    v |= (bytes[i + k] as u64) << (8 * k);
                }
                m.insert(num, v);
                i += sz as usize;
            }
            *out.counts.entry(global).or_default() += 1;
            if global == RECORD {
                out.records.push(m);
            } else if global == SESSION {
                out.sessions.push(m);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{RideSamples, RideSummary};

    pub fn sample_ride() -> RideRecord {
        let mut sm = RideSamples::default();
        for i in 0..600u32 {
            sm.t.push(i);
            sm.power.push(200.0 + (i % 10) as f32);
            sm.cadence.push(90.0);
            sm.hr.push(if i < 5 { 0.0 } else { 140.0 });
            sm.speed.push(9.0);
            sm.distance.push(i as f32 * 9.0);
            sm.ele.push(100.0 + i as f32 * 0.1);
            sm.grade.push(1.0);
            sm.target.push(0.0);
        }
        RideRecord {
            summary: RideSummary {
                name: "Test".into(),
                started_at: "2026-11-23T18:00:00+00:00".into(),
                elapsed_s: 600.0,
                moving_s: 600.0,
                distance_m: 5391.0,
                avg_power: 204.5,
                max_power: 209.0,
                np: 205.0,
                avg_hr: Some(140.0),
                max_hr: Some(140.0),
                avg_cadence: 90.0,
                max_cadence: 90.0,
                calories_kcal: 122.0,
                work_kj: 122.7,
                tss: 14.2,
                intensity_factor: 0.93,
                ftp: 220.0,
                avg_speed_mps: 9.0,
                max_speed_mps: 9.0,
                elevation_gain_m: 59.9,
                ..Default::default()
            },
            samples: sm,
        }
    }

    #[test]
    fn crc_known_value() {
        // CRC of a standard 12-byte header for a 0-length data section.
        assert_eq!(crc16(b""), 0);
        assert_ne!(crc16(b".FIT"), 0);
    }

    #[test]
    fn roundtrip() {
        let r = sample_ride();
        let bytes = to_fit(&r);
        let d = decode(&bytes).unwrap();
        assert_eq!(d.counts[&RECORD], 600);
        assert_eq!(d.counts[&SESSION], 1);
        assert_eq!(d.counts[&LAP], 1);
        assert_eq!(d.counts[&ACTIVITY], 1);
        assert_eq!(d.counts[&FILE_ID], 1);
        let first = &d.records[0];
        assert_eq!(first[&7], 200); // power
        assert_eq!(first[&3], 0xFF); // missing HR → invalid
        assert_eq!(d.records[10][&3], 140);
        assert_eq!(first[&2], ((100.0 + 500.0) * 5.0) as u64); // altitude
        let start = DateTime::parse_from_rfc3339("2026-11-23T18:00:00+00:00").unwrap().timestamp() - FIT_EPOCH_OFFSET;
        assert_eq!(first[&253], start as u64);
        let sess = &d.sessions[0];
        assert_eq!(sess[&5], 2);
        assert_eq!(sess[&6], 58);
        assert_eq!(sess[&9], 539_100);
    }

    #[test]
    fn manual_laps_become_fit_laps() {
        let mut r = sample_ride();
        for (k, a) in [(1u32, 0usize), (2, 300)] {
            r.summary.splits.push(Split::from_samples(&r.samples, a, a + 300, "lap", format!("Lap {k}"), k).unwrap());
        }
        let d = decode(&to_fit(&r)).unwrap();
        assert_eq!(d.counts[&LAP], 2);
    }
}
