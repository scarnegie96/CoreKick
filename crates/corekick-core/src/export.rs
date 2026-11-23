//! TCX export (Garmin Training Center XML) – accepted by Strava, TrainingPeaks,
//! intervals.icu, Garmin Connect. Includes power via the ActivityExtension v2.

use chrono::{DateTime, Duration, FixedOffset, Utc};

use crate::model::RideRecord;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub fn to_tcx(r: &RideRecord) -> String {
    let s = &r.summary;
    let start: DateTime<Utc> = DateTime::<FixedOffset>::parse_from_rfc3339(&s.started_at)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    let iso = |d: DateTime<Utc>| d.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let mut x = String::with_capacity(r.samples.len() * 260 + 1024);
    x.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>
<TrainingCenterDatabase xmlns="http://www.garmin.com/xmlschemas/TrainingCenterDatabase/v2" xmlns:ns3="http://www.garmin.com/xmlschemas/ActivityExtension/v2">
  <Activities>
    <Activity Sport="Biking">
"#);
    x.push_str(&format!("      <Id>{}</Id>\n", iso(start)));
    x.push_str(&format!("      <Lap StartTime=\"{}\">\n", iso(start)));
    x.push_str(&format!("        <TotalTimeSeconds>{:.1}</TotalTimeSeconds>\n", s.elapsed_s));
    x.push_str(&format!("        <DistanceMeters>{:.1}</DistanceMeters>\n", s.distance_m));
    x.push_str(&format!("        <MaximumSpeed>{:.2}</MaximumSpeed>\n", s.max_speed_mps));
    x.push_str(&format!("        <Calories>{:.0}</Calories>\n", s.calories_kcal));
    if let Some(h) = s.avg_hr {
        x.push_str(&format!("        <AverageHeartRateBpm><Value>{:.0}</Value></AverageHeartRateBpm>\n", h));
    }
    if let Some(h) = s.max_hr {
        x.push_str(&format!("        <MaximumHeartRateBpm><Value>{:.0}</Value></MaximumHeartRateBpm>\n", h));
    }
    x.push_str("        <Intensity>Active</Intensity>\n");
    if s.avg_cadence > 0.0 {
        x.push_str(&format!("        <Cadence>{:.0}</Cadence>\n", s.avg_cadence));
    }
    x.push_str("        <TriggerMethod>Manual</TriggerMethod>\n        <Track>\n");
    let sm = &r.samples;
    for i in 0..sm.len() {
        let t = start + Duration::seconds(sm.t[i] as i64);
        x.push_str("          <Trackpoint>\n");
        x.push_str(&format!("            <Time>{}</Time>\n", iso(t)));
        x.push_str(&format!("            <AltitudeMeters>{:.1}</AltitudeMeters>\n", sm.ele[i]));
        x.push_str(&format!("            <DistanceMeters>{:.1}</DistanceMeters>\n", sm.distance[i]));
        if sm.hr[i] > 0.0 {
            x.push_str(&format!("            <HeartRateBpm><Value>{:.0}</Value></HeartRateBpm>\n", sm.hr[i]));
        }
        x.push_str(&format!("            <Cadence>{:.0}</Cadence>\n", sm.cadence[i].min(254.0)));
        x.push_str(&format!(
            "            <Extensions><ns3:TPX><ns3:Speed>{:.2}</ns3:Speed><ns3:Watts>{:.0}</ns3:Watts></ns3:TPX></Extensions>\n",
            sm.speed[i], sm.power[i]
        ));
        x.push_str("          </Trackpoint>\n");
    }
    x.push_str("        </Track>\n");
    x.push_str(&format!(
        "        <Extensions><ns3:LX><ns3:AvgSpeed>{:.2}</ns3:AvgSpeed><ns3:AvgWatts>{:.0}</ns3:AvgWatts><ns3:MaxWatts>{:.0}</ns3:MaxWatts></ns3:LX></Extensions>\n",
        s.avg_speed_mps, s.avg_power, s.max_power
    ));
    x.push_str("      </Lap>\n");
    x.push_str(&format!("      <Notes>{}</Notes>\n", esc(&format!("{} – CoreKick", s.name))));
    x.push_str("      <Creator xsi:type=\"Device_t\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"><Name>CoreKick</Name></Creator>\n");
    x.push_str("    </Activity>\n  </Activities>\n</TrainingCenterDatabase>\n");
    x
}

/// Simple CSV of the 1 Hz samples.
pub fn to_csv(r: &RideRecord) -> String {
    let sm = &r.samples;
    let mut out = String::from("time_s,power_w,cadence_rpm,heart_rate_bpm,speed_kmh,distance_m,elevation_m,grade_pct,target_w\n");
    for i in 0..sm.len() {
        out.push_str(&format!(
            "{},{:.0},{:.0},{:.0},{:.2},{:.1},{:.1},{:.1},{:.0}\n",
            sm.t[i],
            sm.power[i],
            sm.cadence[i],
            sm.hr[i],
            sm.speed[i] * 3.6,
            sm.distance[i],
            sm.ele[i],
            sm.grade[i],
            sm.target[i]
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{RideSamples, RideSummary};

    #[test]
    fn tcx_is_valid_xml() {
        let mut sm = RideSamples::default();
        for i in 0..10 {
            sm.t.push(i);
            sm.power.push(200.0);
            sm.cadence.push(90.0);
            sm.hr.push(if i % 2 == 0 { 0.0 } else { 140.0 });
            sm.speed.push(9.0);
            sm.distance.push(i as f32 * 9.0);
            sm.ele.push(10.0);
            sm.grade.push(0.0);
            sm.target.push(0.0);
        }
        let r = RideRecord {
            summary: RideSummary { name: "A & B <test>".into(), started_at: "2026-11-23T07:00:00+00:00".into(), elapsed_s: 10.0, ..Default::default() },
            samples: sm,
        };
        let x = to_tcx(&r);
        let doc = roxmltree::Document::parse(&x).expect("valid xml");
        assert_eq!(doc.descendants().filter(|n| n.tag_name().name() == "Trackpoint").count(), 10);
        assert!(x.contains("<Time>2026-11-23T07:00:09Z</Time>"));
        assert_eq!(to_csv(&r).lines().count(), 11);
    }
}
