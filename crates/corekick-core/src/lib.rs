//! CoreKick core: everything that does not depend on Bluetooth or the UI.
//!
//! * [`ftms`]      – Bluetooth FTMS / Heart Rate byte-level protocol
//! * [`physics`]   – rider + bike physics used for virtual speed
//! * [`route`]     – routes, elevation profiles, climb detection, generator, GPX import
//! * [`workout`]   – structured workouts (ERG) and the built-in library
//! * [`fitness_test`] – FTP test protocols and result evaluation
//! * [`metrics`]   – NP / IF / TSS / power bests / zones / energy & calories
//! * [`gears`]     – virtual shifting for single-speed setups (Zwift Cog)
//! * [`engine`]    – the real-time ride engine (pure, deterministic, testable)
//! * [`db`]        – SQLite persistence
//! * [`stats`]     – account level statistics (fitness, power curve, records)
//! * [`events`]    – recurring event schedule expansion
//! * [`export`]    – TCX export

pub mod db;
pub mod engine;
pub mod events;
pub mod export;
pub mod fit;
pub mod fitness_test;
pub mod ftms;
pub mod ftp_estimate;
pub mod gears;
pub mod metrics;
pub mod model;
pub mod physics;
pub mod rng;
pub mod route;
pub mod stats;
pub mod workout;

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn now_rfc3339() -> String {
    chrono::Local::now().to_rfc3339()
}
