//! Expansion of recurring events into dated occurrences.

use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};

use crate::model::{Event, Freq};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Occurrence {
    pub event_id: String,
    pub event_name: String,
    pub color: String,
    /// Local date-time, ISO 8601 without offset
    pub at: String,
}

pub fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok()
}

fn parse_time(s: &str) -> NaiveTime {
    NaiveTime::parse_from_str(s.trim(), "%H:%M").unwrap_or_else(|_| NaiveTime::from_hms_opt(18, 0, 0).unwrap())
}

pub fn occurs_on(ev: &Event, d: NaiveDate) -> bool {
    let r = &ev.recurrence;
    let Some(start) = parse_date(&r.start_date) else { return false };
    if d < start {
        return false;
    }
    if let Some(end) = r.end_date.as_deref().and_then(parse_date) {
        if d > end {
            return false;
        }
    }
    let interval = r.interval.max(1) as i64;
    let days = (d - start).num_days();
    match r.freq {
        Freq::Once => d == start,
        Freq::Daily => days % interval == 0,
        Freq::EveryNDays => days % interval == 0,
        Freq::Weekly => {
            let wd = d.weekday().num_days_from_monday() as u8;
            let ok_day = if r.weekdays.is_empty() {
                wd == start.weekday().num_days_from_monday() as u8
            } else {
                r.weekdays.contains(&wd)
            };
            // week index relative to the Monday of the start week
            let start_monday = start - chrono::Duration::days(start.weekday().num_days_from_monday() as i64);
            let week = (d - start_monday).num_days() / 7;
            ok_day && week % interval == 0
        }
    }
}

pub fn occurrences(ev: &Event, from: NaiveDate, to: NaiveDate) -> Vec<NaiveDateTime> {
    if !ev.active || to < from {
        return vec![];
    }
    let time = parse_time(&ev.recurrence.time);
    let mut out = Vec::new();
    let mut d = from;
    while d <= to && out.len() < 400 {
        if occurs_on(ev, d) {
            out.push(d.and_time(time));
        }
        d = d.succ_opt().unwrap();
    }
    out
}

pub fn upcoming(events: &[Event], from: NaiveDate, days: i64) -> Vec<Occurrence> {
    let to = from + chrono::Duration::days(days.max(0));
    let mut v: Vec<Occurrence> = events
        .iter()
        .flat_map(|e| {
            occurrences(e, from, to).into_iter().map(move |dt| Occurrence {
                event_id: e.id.clone(),
                event_name: e.name.clone(),
                color: e.color.clone(),
                at: dt.format("%Y-%m-%dT%H:%M:%S").to_string(),
            })
        })
        .collect();
    v.sort_by(|a, b| a.at.cmp(&b.at));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EventTarget, Recurrence};

    fn ev(freq: Freq, weekdays: Vec<u8>, interval: u32) -> Event {
        Event {
            id: "e".into(),
            name: "E".into(),
            description: String::new(),
            target: EventTarget::Route { route_id: "r".into(), laps: 1 },
            recurrence: Recurrence { freq, start_date: "2026-11-02".into(), time: "07:30".into(), end_date: None, weekdays, interval },
            created_at: String::new(),
            color: "#fff".into(),
            active: true,
        }
    }

    fn d(s: &str) -> NaiveDate {
        parse_date(s).unwrap()
    }

    #[test]
    fn weekly_tue_thu() {
        let e = ev(Freq::Weekly, vec![1, 3], 1); // 2026-11-02 is a Monday
        let o = occurrences(&e, d("2026-11-01"), d("2026-11-15"));
        let days: Vec<String> = o.iter().map(|x| x.format("%d").to_string()).collect();
        assert_eq!(days, vec!["03", "05", "10", "12"]);
        assert_eq!(o[0].format("%H:%M").to_string(), "07:30");
    }

    #[test]
    fn fortnightly() {
        let e = ev(Freq::Weekly, vec![], 2);
        let o = occurrences(&e, d("2026-11-01"), d("2026-11-30"));
        let days: Vec<String> = o.iter().map(|x| x.format("%d").to_string()).collect();
        assert_eq!(days, vec!["02", "16", "30"]);
    }

    #[test]
    fn once_daily_every_n() {
        assert_eq!(occurrences(&ev(Freq::Once, vec![], 1), d("2026-10-01"), d("2026-12-01")).len(), 1);
        assert_eq!(occurrences(&ev(Freq::Daily, vec![], 1), d("2026-11-02"), d("2026-11-08")).len(), 7);
        assert_eq!(occurrences(&ev(Freq::EveryNDays, vec![], 3), d("2026-11-02"), d("2026-11-11")).len(), 4);
    }

    #[test]
    fn end_date_and_inactive() {
        let mut e = ev(Freq::Daily, vec![], 1);
        e.recurrence.end_date = Some("2026-11-04".into());
        assert_eq!(occurrences(&e, d("2026-11-01"), d("2026-11-30")).len(), 3);
        e.active = false;
        assert!(occurrences(&e, d("2026-11-01"), d("2026-11-30")).is_empty());
    }
}
