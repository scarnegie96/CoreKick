//! SQLite persistence. Rich objects are stored as JSON with a few indexed columns.

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use crate::model::{Event, FtpEntry, Profile, RideRecord, RideSamples, RideSummary};
use crate::route::Route;
use crate::workout::Workout;

pub struct Store {
    conn: Connection,
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS profile (id INTEGER PRIMARY KEY CHECK (id = 1), json TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS routes (id TEXT PRIMARY KEY, created_at TEXT NOT NULL, json TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS workouts (id TEXT PRIMARY KEY, created_at TEXT NOT NULL, json TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS events (id TEXT PRIMARY KEY, created_at TEXT NOT NULL, json TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS rides (
    id TEXT PRIMARY KEY,
    started_at TEXT NOT NULL,
    route_id TEXT,
    event_id TEXT,
    laps INTEGER NOT NULL DEFAULT 1,
    completed INTEGER NOT NULL DEFAULT 0,
    route_time_s REAL,
    summary TEXT NOT NULL,
    samples TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS rides_started ON rides(started_at);
CREATE INDEX IF NOT EXISTS rides_route ON rides(route_id, laps, completed);
CREATE INDEX IF NOT EXISTS rides_event ON rides(event_id);
CREATE TABLE IF NOT EXISTS ftp_history (id INTEGER PRIMARY KEY AUTOINCREMENT, date TEXT NOT NULL, ftp REAL NOT NULL, source TEXT NOT NULL);
"#;

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        conn.pragma_update(None, "journal_mode", "WAL").ok();
        conn.pragma_update(None, "foreign_keys", "ON").ok();
        let s = Store { conn };
        s.migrate()?;
        Ok(s)
    }

    pub fn open_in_memory() -> Result<Self> {
        let s = Store { conn: Connection::open_in_memory()? };
        s.migrate()?;
        Ok(s)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(SCHEMA)?;
        Ok(())
    }

    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self.conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0)).optional()?)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    // ---------------- profile ----------------
    pub fn profile(&self) -> Result<Profile> {
        let j: Option<String> = self.conn.query_row("SELECT json FROM profile WHERE id = 1", [], |r| r.get(0)).optional()?;
        Ok(match j {
            Some(j) => serde_json::from_str(&j).unwrap_or_default(),
            None => Profile::default(),
        })
    }

    pub fn save_profile(&self, p: &Profile) -> Result<()> {
        self.conn.execute(
            "INSERT INTO profile (id, json) VALUES (1, ?1) ON CONFLICT(id) DO UPDATE SET json = excluded.json",
            [serde_json::to_string(p)?],
        )?;
        Ok(())
    }

    // ---------------- generic JSON tables ----------------
    fn list_json<T: serde::de::DeserializeOwned>(&self, table: &str) -> Result<Vec<T>> {
        let mut st = self.conn.prepare(&format!("SELECT json FROM {table} ORDER BY created_at DESC"))?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for j in rows {
            if let Ok(v) = serde_json::from_str(&j?) {
                out.push(v);
            }
        }
        Ok(out)
    }

    fn get_json<T: serde::de::DeserializeOwned>(&self, table: &str, id: &str) -> Result<Option<T>> {
        let j: Option<String> =
            self.conn.query_row(&format!("SELECT json FROM {table} WHERE id = ?1"), [id], |r| r.get(0)).optional()?;
        Ok(match j {
            Some(j) => Some(serde_json::from_str(&j)?),
            None => None,
        })
    }

    fn put_json<T: serde::Serialize>(&self, table: &str, id: &str, created_at: &str, v: &T) -> Result<()> {
        self.conn.execute(
            &format!(
                "INSERT INTO {table} (id, created_at, json) VALUES (?1, ?2, ?3)
                 ON CONFLICT(id) DO UPDATE SET json = excluded.json"
            ),
            params![id, created_at, serde_json::to_string(v)?],
        )?;
        Ok(())
    }

    fn delete(&self, table: &str, id: &str) -> Result<()> {
        self.conn.execute(&format!("DELETE FROM {table} WHERE id = ?1"), [id])?;
        Ok(())
    }

    pub fn routes(&self) -> Result<Vec<Route>> {
        self.list_json("routes")
    }
    pub fn route(&self, id: &str) -> Result<Option<Route>> {
        self.get_json("routes", id)
    }
    pub fn save_route(&self, r: &Route) -> Result<()> {
        self.put_json("routes", &r.id, &r.created_at, r)
    }
    pub fn delete_route(&self, id: &str) -> Result<()> {
        self.delete("routes", id)
    }

    pub fn workouts(&self) -> Result<Vec<Workout>> {
        self.list_json("workouts")
    }
    pub fn workout(&self, id: &str) -> Result<Option<Workout>> {
        self.get_json("workouts", id)
    }
    pub fn save_workout(&self, w: &Workout) -> Result<()> {
        self.put_json("workouts", &w.id, &w.created_at, w)
    }
    pub fn delete_workout(&self, id: &str) -> Result<()> {
        self.delete("workouts", id)
    }

    pub fn events(&self) -> Result<Vec<Event>> {
        self.list_json("events")
    }
    pub fn event(&self, id: &str) -> Result<Option<Event>> {
        self.get_json("events", id)
    }
    pub fn save_event(&self, e: &Event) -> Result<()> {
        self.put_json("events", &e.id, &e.created_at, e)
    }
    pub fn delete_event(&self, id: &str) -> Result<()> {
        self.delete("events", id)
    }

    // ---------------- rides ----------------
    pub fn save_ride(&self, r: &RideRecord) -> Result<()> {
        let s = &r.summary;
        self.conn.execute(
            "INSERT INTO rides (id, started_at, route_id, event_id, laps, completed, route_time_s, summary, samples)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET summary = excluded.summary, samples = excluded.samples",
            params![
                s.id,
                s.started_at,
                s.route_id,
                s.event_id,
                s.laps,
                s.completed as i32,
                s.route_time_s,
                serde_json::to_string(s)?,
                serde_json::to_string(&r.samples)?
            ],
        )?;
        Ok(())
    }

    pub fn update_summary(&self, s: &RideSummary) -> Result<()> {
        self.conn.execute("UPDATE rides SET summary = ?2 WHERE id = ?1", params![s.id, serde_json::to_string(s)?])?;
        Ok(())
    }

    pub fn ride_summaries(&self) -> Result<Vec<RideSummary>> {
        let mut st = self.conn.prepare("SELECT summary FROM rides ORDER BY started_at DESC")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for j in rows {
            if let Ok(v) = serde_json::from_str(&j?) {
                out.push(v);
            }
        }
        Ok(out)
    }

    pub fn ride(&self, id: &str) -> Result<Option<RideRecord>> {
        let row: Option<(String, String)> = self
            .conn
            .query_row("SELECT summary, samples FROM rides WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        Ok(match row {
            Some((s, sm)) => Some(RideRecord {
                summary: serde_json::from_str(&s)?,
                samples: serde_json::from_str::<RideSamples>(&sm).unwrap_or_default(),
            }),
            None => None,
        })
    }

    pub fn delete_ride(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM rides WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Fastest completed attempt at a route with the same number of laps.
    pub fn best_route_ride(&self, route_id: &str, laps: u32) -> Result<Option<RideRecord>> {
        let id: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM rides WHERE route_id = ?1 AND laps = ?2 AND completed = 1 AND route_time_s IS NOT NULL
                 ORDER BY route_time_s ASC LIMIT 1",
                params![route_id, laps],
                |r| r.get(0),
            )
            .optional()?;
        match id {
            Some(id) => self.ride(&id),
            None => Ok(None),
        }
    }

    pub fn rides_for_route(&self, route_id: &str) -> Result<Vec<RideSummary>> {
        self.summaries_where("route_id", route_id)
    }

    pub fn rides_for_event(&self, event_id: &str) -> Result<Vec<RideSummary>> {
        self.summaries_where("event_id", event_id)
    }

    fn summaries_where(&self, col: &str, val: &str) -> Result<Vec<RideSummary>> {
        let mut st = self.conn.prepare(&format!("SELECT summary FROM rides WHERE {col} = ?1 ORDER BY started_at DESC"))?;
        let rows = st.query_map([val], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for j in rows {
            if let Ok(v) = serde_json::from_str(&j?) {
                out.push(v);
            }
        }
        Ok(out)
    }

    // ---------------- FTP history ----------------
    pub fn add_ftp(&self, e: &FtpEntry) -> Result<()> {
        self.conn.execute("INSERT INTO ftp_history (date, ftp, source) VALUES (?1, ?2, ?3)", params![e.date, e.ftp, e.source])?;
        Ok(())
    }

    pub fn ftp_history(&self) -> Result<Vec<FtpEntry>> {
        let mut st = self.conn.prepare("SELECT date, ftp, source FROM ftp_history ORDER BY date ASC, id ASC")?;
        let rows = st.query_map([], |r| Ok(FtpEntry { date: r.get(0)?, ftp: r.get(1)?, source: r.get(2)? }))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route::seed_routes;
    use crate::workout::builtin_workouts;

    #[test]
    fn roundtrips() {
        let s = Store::open_in_memory().unwrap();
        let mut p = s.profile().unwrap();
        p.ftp = 301.0;
        s.save_profile(&p).unwrap();
        assert_eq!(s.profile().unwrap().ftp, 301.0);

        let r = seed_routes().remove(0);
        s.save_route(&r).unwrap();
        assert_eq!(s.route(&r.id).unwrap().unwrap(), r);
        assert_eq!(s.routes().unwrap().len(), 1);
        s.delete_route(&r.id).unwrap();
        assert!(s.route(&r.id).unwrap().is_none());

        let mut w = builtin_workouts().remove(0);
        w.id = "user-1".into();
        s.save_workout(&w).unwrap();
        assert_eq!(s.workout("user-1").unwrap().unwrap().name, w.name);

        let mk = |id: &str, t: f64| RideRecord {
            summary: RideSummary { id: id.into(), started_at: format!("2026-11-0{}T08:00:00Z", id.len()), route_id: Some("R".into()), laps: 1, completed: true, route_time_s: Some(t), ..Default::default() },
            samples: RideSamples::default(),
        };
        s.save_ride(&mk("a", 1200.0)).unwrap();
        s.save_ride(&mk("bb", 1100.0)).unwrap();
        assert_eq!(s.best_route_ride("R", 1).unwrap().unwrap().summary.id, "bb");
        assert!(s.best_route_ride("R", 2).unwrap().is_none());
        assert_eq!(s.ride_summaries().unwrap().len(), 2);
        s.add_ftp(&FtpEntry { date: "2026-11-01".into(), ftp: 250.0, source: "manual".into() }).unwrap();
        assert_eq!(s.ftp_history().unwrap().len(), 1);
        s.set_meta("seeded", "1").unwrap();
        assert_eq!(s.meta("seeded").unwrap().as_deref(), Some("1"));
    }
}
