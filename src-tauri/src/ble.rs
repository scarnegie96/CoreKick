//! Bluetooth LE: scanning, FTMS trainer control and heart-rate monitors (btleplug).
//!
//! Each connected device runs a *supervisor* task that (re)connects, subscribes,
//! performs the FTMS control handshake and then multiplexes notifications and
//! outgoing control-point writes in one `select!` loop. Control-point writes are
//! serialised (one outstanding request at a time, as FTMS requires) and
//! coalesced so only the newest SIM/ERG target is ever sent.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use btleplug::api::bleuuid::uuid_from_u16;
use btleplug::api::{Central, CentralEvent, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral, PeripheralId};
use corekick_core::engine::{TrainerCommand, TrainerInput};
use corekick_core::ftms::{self, MachineStatus, ResultCode};
use futures::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    Trainer,
    HeartRate,
    Power,
}

#[derive(Clone, Debug, Serialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub rssi: Option<i16>,
    pub kind: DeviceKind,
    pub ftms: bool,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LinkState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct Link {
    pub state: LinkState,
    pub device_id: Option<String>,
    pub name: Option<String>,
    pub detail: Option<String>,
    pub power_range: Option<(i16, i16)>,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct BleStatus {
    pub available: bool,
    pub error: Option<String>,
    pub scanning: bool,
    pub trainer: Link,
    pub hrm: Link,
}

#[derive(Default)]
pub struct Telemetry {
    pub power_w: f64,
    pub cadence_rpm: f64,
    pub speed_kmh: Option<f64>,
    pub trainer_hr: Option<f64>,
    pub last_trainer: Option<Instant>,
    pub hr: Option<f64>,
    pub last_hr: Option<Instant>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Trainer,
    Hrm,
}

pub struct Ble {
    app: AppHandle,
    adapter: tokio::sync::Mutex<Option<Adapter>>,
    devices: Mutex<HashMap<String, (PeripheralId, DeviceInfo)>>,
    pub telemetry: Mutex<Telemetry>,
    status: Mutex<BleStatus>,
    trainer_tx: Mutex<Option<mpsc::UnboundedSender<TrainerCommand>>>,
    trainer_cancel: Mutex<Option<Arc<AtomicBool>>>,
    hrm_cancel: Mutex<Option<Arc<AtomicBool>>>,
    scan_cancel: Mutex<Option<Arc<AtomicBool>>>,
}

impl Ble {
    pub fn new(app: AppHandle) -> Arc<Self> {
        Arc::new(Ble {
            app,
            adapter: tokio::sync::Mutex::new(None),
            devices: Mutex::new(HashMap::new()),
            telemetry: Mutex::new(Telemetry::default()),
            status: Mutex::new(BleStatus { available: true, ..Default::default() }),
            trainer_tx: Mutex::new(None),
            trainer_cancel: Mutex::new(None),
            hrm_cancel: Mutex::new(None),
            scan_cancel: Mutex::new(None),
        })
    }

    pub fn status(&self) -> BleStatus {
        self.status.lock().unwrap().clone()
    }

    fn update_status(&self, f: impl FnOnce(&mut BleStatus)) {
        let s = {
            let mut st = self.status.lock().unwrap();
            f(&mut st);
            st.clone()
        };
        let _ = self.app.emit("ble://status", s);
    }

    fn set_link(&self, role: Role, f: impl FnOnce(&mut Link)) {
        self.update_status(|s| f(if role == Role::Trainer { &mut s.trainer } else { &mut s.hrm }));
    }

    async fn adapter(&self) -> Result<Adapter> {
        let mut guard = self.adapter.lock().await;
        if let Some(a) = guard.as_ref() {
            return Ok(a.clone());
        }
        let res: Result<Adapter> = async {
            let manager = Manager::new().await.context("Bluetooth is unavailable")?;
            let adapters = manager.adapters().await.context("Could not list Bluetooth adapters")?;
            adapters.into_iter().next().ok_or_else(|| anyhow!("No Bluetooth adapter found"))
        }
        .await;
        match res {
            Ok(a) => {
                *guard = Some(a.clone());
                self.update_status(|s| {
                    s.available = true;
                    s.error = None;
                });
                Ok(a)
            }
            Err(e) => {
                let msg = format!("{e:#}");
                self.update_status(|s| {
                    s.available = false;
                    s.error = Some(msg.clone());
                });
                Err(e)
            }
        }
    }

    pub fn devices(&self) -> Vec<DeviceInfo> {
        let mut v: Vec<DeviceInfo> = self.devices.lock().unwrap().values().map(|(_, d)| d.clone()).collect();
        v.sort_by(|a, b| {
            let rank = |d: &DeviceInfo| match d.kind {
                DeviceKind::Trainer => 0,
                DeviceKind::Power => 1,
                DeviceKind::HeartRate => 2,
            };
            rank(a).cmp(&rank(b)).then(b.rssi.unwrap_or(-127).cmp(&a.rssi.unwrap_or(-127)))
        });
        v
    }

    pub async fn start_scan(self: &Arc<Self>, seconds: u64) -> Result<()> {
        let adapter = self.adapter().await?;
        if let Some(c) = self.scan_cancel.lock().unwrap().take() {
            c.store(true, Ordering::SeqCst);
        }
        let mut events = adapter.events().await?;
        // Unfiltered: some trainers only advertise FTMS in the scan response.
        adapter.start_scan(ScanFilter::default()).await.context("Failed to start scanning")?;
        let cancel = Arc::new(AtomicBool::new(false));
        *self.scan_cancel.lock().unwrap() = Some(cancel.clone());
        self.update_status(|s| s.scanning = true);
        let me = self.clone();
        tauri::async_runtime::spawn(async move {
            let deadline = Instant::now() + Duration::from_secs(seconds.clamp(5, 120));
            while !cancel.load(Ordering::SeqCst) && Instant::now() < deadline {
                let ev = tokio::time::timeout(Duration::from_millis(500), events.next()).await;
                let id = match ev {
                    Ok(Some(CentralEvent::DeviceDiscovered(id)))
                    | Ok(Some(CentralEvent::DeviceUpdated(id)))
                    | Ok(Some(CentralEvent::ServicesAdvertisement { id, .. })) => id,
                    Ok(None) => break,
                    _ => continue,
                };
                if let Ok(p) = adapter.peripheral(&id).await {
                    if let Ok(Some(props)) = p.properties().await {
                        if let Some(info) = classify(&id, props.local_name.clone(), &props.services, props.rssi) {
                            let changed = {
                                let mut d = me.devices.lock().unwrap();
                                let prev = d.insert(info.id.clone(), (id.clone(), info.clone()));
                                prev.map(|(_, p)| p.name != info.name || p.kind != info.kind).unwrap_or(true)
                            };
                            if changed {
                                let _ = me.app.emit("ble://devices", me.devices());
                            }
                        }
                    }
                }
            }
            let _ = adapter.stop_scan().await;
            me.update_status(|s| s.scanning = false);
            let _ = me.app.emit("ble://devices", me.devices());
        });
        Ok(())
    }

    pub fn stop_scan(&self) {
        if let Some(c) = self.scan_cancel.lock().unwrap().take() {
            c.store(true, Ordering::SeqCst);
        }
    }

    async fn peripheral(&self, id: &str) -> Result<Peripheral> {
        let adapter = self.adapter().await?;
        let pid = self
            .devices
            .lock()
            .unwrap()
            .get(id)
            .map(|(p, _)| p.clone())
            .ok_or_else(|| anyhow!("Unknown device – scan again"))?;
        Ok(adapter.peripheral(&pid).await?)
    }

    pub fn connect_trainer(self: &Arc<Self>, id: String) {
        self.disconnect_trainer();
        let (tx, rx) = mpsc::unbounded_channel();
        let cancel = Arc::new(AtomicBool::new(false));
        *self.trainer_tx.lock().unwrap() = Some(tx);
        *self.trainer_cancel.lock().unwrap() = Some(cancel.clone());
        let name = self.devices.lock().unwrap().get(&id).map(|(_, d)| d.name.clone());
        self.set_link(Role::Trainer, |l| {
            *l = Link { state: LinkState::Connecting, device_id: Some(id.clone()), name, detail: None, power_range: None }
        });
        let me = self.clone();
        tauri::async_runtime::spawn(async move { me.trainer_supervisor(id, cancel, rx).await });
    }

    pub fn disconnect_trainer(&self) {
        if let Some(c) = self.trainer_cancel.lock().unwrap().take() {
            c.store(true, Ordering::SeqCst);
        }
        *self.trainer_tx.lock().unwrap() = None;
    }

    pub fn connect_hrm(self: &Arc<Self>, id: String) {
        self.disconnect_hrm();
        let cancel = Arc::new(AtomicBool::new(false));
        *self.hrm_cancel.lock().unwrap() = Some(cancel.clone());
        let name = self.devices.lock().unwrap().get(&id).map(|(_, d)| d.name.clone());
        self.set_link(Role::Hrm, |l| {
            *l = Link { state: LinkState::Connecting, device_id: Some(id.clone()), name, detail: None, power_range: None }
        });
        let me = self.clone();
        tauri::async_runtime::spawn(async move { me.hrm_supervisor(id, cancel).await });
    }

    pub fn disconnect_hrm(&self) {
        if let Some(c) = self.hrm_cancel.lock().unwrap().take() {
            c.store(true, Ordering::SeqCst);
        }
    }

    pub fn send(&self, cmd: TrainerCommand) {
        if let Some(tx) = self.trainer_tx.lock().unwrap().as_ref() {
            let _ = tx.send(cmd);
        }
    }

    /// Current readings as engine input. Data older than 3 s counts as "not riding".
    pub fn input(&self) -> TrainerInput {
        let t = self.telemetry.lock().unwrap();
        let fresh = t.last_trainer.map(|x| x.elapsed() < Duration::from_secs(3)).unwrap_or(false);
        let hr_fresh = t.last_hr.map(|x| x.elapsed() < Duration::from_secs(5)).unwrap_or(false);
        TrainerInput {
            power_w: if fresh { t.power_w } else { 0.0 },
            cadence_rpm: if fresh { t.cadence_rpm } else { 0.0 },
            heart_rate: if hr_fresh { t.hr } else if fresh { t.trainer_hr } else { None },
            connected: fresh,
        }
    }

    pub fn trainer_connected(&self) -> bool {
        self.status.lock().unwrap().trainer.state == LinkState::Connected
    }

    // ------------------------------------------------------------------
    // Trainer
    // ------------------------------------------------------------------

    async fn trainer_supervisor(
        self: Arc<Self>,
        id: String,
        cancel: Arc<AtomicBool>,
        mut rx: mpsc::UnboundedReceiver<TrainerCommand>,
    ) {
        let mut last_cmd: Option<TrainerCommand> = None;
        let mut attempt = 0u32;
        while !cancel.load(Ordering::SeqCst) {
            let res = self.trainer_session(&id, &cancel, &mut rx, &mut last_cmd).await;
            if cancel.load(Ordering::SeqCst) {
                break;
            }
            attempt += 1;
            let detail = match res {
                Ok(()) => "Connection closed".to_string(),
                Err(e) => format!("{e:#}"),
            };
            log::warn!("trainer session ended: {detail}");
            self.set_link(Role::Trainer, |l| {
                l.state = LinkState::Reconnecting;
                l.detail = Some(format!("{detail} – retrying…"));
            });
            let wait = Duration::from_millis((1000 * attempt.min(5) as u64).max(1000));
            let until = Instant::now() + wait;
            while Instant::now() < until && !cancel.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
        if let Ok(p) = self.peripheral(&id).await {
            let _ = p.disconnect().await;
        }
        {
            let mut t = self.telemetry.lock().unwrap();
            t.last_trainer = None;
            t.power_w = 0.0;
            t.cadence_rpm = 0.0;
        }
        // Only reset the link if a new connection hasn't replaced us.
        let replaced = self.trainer_cancel.lock().unwrap().as_ref().map(|c| !Arc::ptr_eq(c, &cancel)).unwrap_or(false);
        if !replaced {
            self.set_link(Role::Trainer, |l| *l = Link::default());
        }
    }

    async fn trainer_session(
        &self,
        id: &str,
        cancel: &AtomicBool,
        rx: &mut mpsc::UnboundedReceiver<TrainerCommand>,
        last_cmd: &mut Option<TrainerCommand>,
    ) -> Result<()> {
        let p = self.peripheral(id).await?;
        if !p.is_connected().await.unwrap_or(false) {
            tokio::time::timeout(Duration::from_secs(20), p.connect())
                .await
                .map_err(|_| anyhow!("Timed out connecting"))?
                .context("Connect failed")?;
        }
        p.discover_services().await.context("Service discovery failed")?;
        let chars = p.characteristics();
        let find = |short: u16| chars.iter().find(|c| c.uuid == uuid_from_u16(short)).cloned();
        let bike = find(ftms::INDOOR_BIKE_DATA).ok_or_else(|| {
            anyhow!("No FTMS Indoor Bike Data. Update your KICKR firmware in the Wahoo app to enable FTMS.")
        })?;
        let cp = find(ftms::CONTROL_POINT).ok_or_else(|| anyhow!("Trainer has no FTMS Control Point"))?;
        let status_char = find(ftms::MACHINE_STATUS);
        let power_range = match find(ftms::SUPPORTED_POWER_RANGE) {
            Some(c) => p.read(&c).await.ok().and_then(|b| ftms::parse_power_range(&b)).map(|(a, b, _)| (a, b)),
            None => None,
        };
        // Resistance range uses the same layout (sint16, sint16, uint16; 0.1 resolution).
        let resistance_range = match find(ftms::SUPPORTED_RESISTANCE_RANGE) {
            Some(c) => p.read(&c).await.ok().and_then(|b| ftms::parse_power_range(&b)).map(|(a, b, _)| (a, b)),
            None => None,
        };

        let mut notes = p.notifications().await?;
        p.subscribe(&bike).await.context("Subscribe to bike data failed")?;
        p.subscribe(&cp).await.context("Subscribe to control point failed")?;
        if let Some(s) = &status_char {
            let _ = p.subscribe(s).await;
        }

        let mut queue: VecDeque<Vec<u8>> = VecDeque::new();
        queue.push_back(ftms::cmd_request_control());
        queue.push_back(ftms::cmd_start());
        if let Some(c) = last_cmd {
            queue.push_back(encode(c, resistance_range));
        }
        let mut pending: Option<(u8, Instant)> = None;
        let mut has_control = false;
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        let session_start = Instant::now();

        self.set_link(Role::Trainer, |l| {
            l.state = LinkState::Connected;
            l.detail = Some("Requesting control…".into());
            l.power_range = power_range;
        });

        loop {
            if pending.is_none() {
                if let Some(b) = queue.pop_front() {
                    p.write(&cp, &b, WriteType::WithResponse).await.context("Control point write failed")?;
                    pending = Some((b[0], Instant::now()));
                }
            }
            tokio::select! {
                n = notes.next() => {
                    let Some(n) = n else { bail!("Trainer disconnected") };
                    let short = ftms::short_id(&n.uuid.to_string());
                    match short {
                        Some(ftms::INDOOR_BIKE_DATA) => {
                            if let Some(d) = ftms::parse_indoor_bike_data(&n.value) {
                                let mut t = self.telemetry.lock().unwrap();
                                if let Some(pw) = d.power_w { t.power_w = pw.max(0) as f64; }
                                if let Some(c) = d.cadence_rpm { t.cadence_rpm = c; }
                                if d.speed_kmh.is_some() { t.speed_kmh = d.speed_kmh; }
                                if let Some(h) = d.heart_rate { t.trainer_hr = if h > 0 { Some(h as f64) } else { None }; }
                                t.last_trainer = Some(Instant::now());
                            }
                        }
                        Some(ftms::CONTROL_POINT) => {
                            if let Some(r) = ftms::parse_control_response(&n.value) {
                                if pending.map(|(op, _)| op == r.request_op).unwrap_or(false) {
                                    pending = None;
                                }
                                match (r.request_op, r.result) {
                                    (ftms::op::REQUEST_CONTROL, ResultCode::Success) => {
                                        has_control = true;
                                        self.set_link(Role::Trainer, |l| l.detail = None);
                                    }
                                    (_, ResultCode::ControlNotPermitted) => {
                                        has_control = false;
                                        queue.push_front(ftms::cmd_request_control());
                                        if let Some(c) = last_cmd { queue.push_back(encode(c, resistance_range)); }
                                    }
                                    (op, res) if res != ResultCode::Success => {
                                        log::warn!("FTMS op 0x{op:02x} -> {res:?}");
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Some(ftms::MACHINE_STATUS) => {
                            if let Some(MachineStatus::ControlPermissionLost) = ftms::parse_machine_status(&n.value) {
                                has_control = false;
                                queue.push_front(ftms::cmd_start());
                                queue.push_front(ftms::cmd_request_control());
                                if let Some(c) = last_cmd { queue.push_back(encode(c, resistance_range)); }
                            }
                        }
                        _ => {}
                    }
                }
                c = rx.recv() => {
                    let Some(c) = c else { return Ok(()) };
                    *last_cmd = Some(c);
                    queue.retain(|b| {
                        b[0] != ftms::op::SET_TARGET_POWER
                            && b[0] != ftms::op::SET_INDOOR_BIKE_SIMULATION
                            && b[0] != ftms::op::SET_TARGET_RESISTANCE
                    });
                    queue.push_back(encode(&c, resistance_range));
                }
                _ = tick.tick() => {
                    if cancel.load(Ordering::SeqCst) {
                        return Ok(());
                    }
                    if let Some((op, at)) = pending {
                        if at.elapsed() > Duration::from_secs(2) {
                            log::warn!("FTMS op 0x{op:02x} timed out");
                            pending = None;
                            if op == ftms::op::REQUEST_CONTROL && !has_control {
                                queue.push_front(ftms::cmd_request_control());
                            }
                        }
                    }
                    let stale = self.telemetry.lock().unwrap().last_trainer.map(|t| t.elapsed() > Duration::from_secs(6)).unwrap_or(true);
                    if stale && session_start.elapsed() > Duration::from_secs(8) && !p.is_connected().await.unwrap_or(false) {
                        bail!("Trainer stopped responding");
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Heart-rate monitor
    // ------------------------------------------------------------------

    async fn hrm_supervisor(self: Arc<Self>, id: String, cancel: Arc<AtomicBool>) {
        let mut attempt = 0u32;
        while !cancel.load(Ordering::SeqCst) {
            let res = self.hrm_session(&id, &cancel).await;
            if cancel.load(Ordering::SeqCst) {
                break;
            }
            attempt += 1;
            let detail = match res {
                Ok(()) => "Connection closed".into(),
                Err(e) => format!("{e:#}"),
            };
            self.set_link(Role::Hrm, |l| {
                l.state = LinkState::Reconnecting;
                l.detail = Some(format!("{detail} – retrying…"));
            });
            tokio::time::sleep(Duration::from_millis(1000 * attempt.min(5) as u64)).await;
        }
        if let Ok(p) = self.peripheral(&id).await {
            let _ = p.disconnect().await;
        }
        self.telemetry.lock().unwrap().last_hr = None;
        let replaced = self.hrm_cancel.lock().unwrap().as_ref().map(|c| !Arc::ptr_eq(c, &cancel)).unwrap_or(false);
        if !replaced {
            self.set_link(Role::Hrm, |l| *l = Link::default());
        }
    }

    async fn hrm_session(&self, id: &str, cancel: &AtomicBool) -> Result<()> {
        let p = self.peripheral(id).await?;
        if !p.is_connected().await.unwrap_or(false) {
            tokio::time::timeout(Duration::from_secs(20), p.connect())
                .await
                .map_err(|_| anyhow!("Timed out connecting"))?
                .context("Connect failed")?;
        }
        p.discover_services().await?;
        let hr = p
            .characteristics()
            .into_iter()
            .find(|c| c.uuid == uuid_from_u16(ftms::HEART_RATE_MEASUREMENT))
            .ok_or_else(|| anyhow!("Device has no heart rate measurement"))?;
        let mut notes = p.notifications().await?;
        p.subscribe(&hr).await?;
        self.set_link(Role::Hrm, |l| {
            l.state = LinkState::Connected;
            l.detail = None;
        });
        let mut tick = tokio::time::interval(Duration::from_millis(500));
        loop {
            tokio::select! {
                n = notes.next() => {
                    let Some(n) = n else { bail!("Heart rate monitor disconnected") };
                    if let Some(bpm) = ftms::parse_heart_rate(&n.value) {
                        let mut t = self.telemetry.lock().unwrap();
                        t.hr = if bpm > 0 { Some(bpm as f64) } else { None };
                        t.last_hr = Some(Instant::now());
                    }
                }
                _ = tick.tick() => {
                    if cancel.load(Ordering::SeqCst) { return Ok(()); }
                }
            }
        }
    }
}

/// `resistance_range` is the trainer's Supported Resistance Level Range in raw
/// 0.1 units; level percentages are mapped onto it (default 0–100 %).
fn encode(c: &TrainerCommand, resistance_range: Option<(i16, i16)>) -> Vec<u8> {
    match *c {
        TrainerCommand::Simulation { grade_pct, wind_mps, crr, cw } => ftms::cmd_simulation(grade_pct, wind_mps, crr, cw),
        TrainerCommand::TargetPower { watts } => ftms::cmd_target_power(watts),
        TrainerCommand::Resistance { level_pct } => {
            let (lo, hi) = resistance_range.filter(|(a, b)| b > a).map(|(a, b)| (a as f64 / 10.0, b as f64 / 10.0)).unwrap_or((0.0, 25.4));
            ftms::cmd_target_resistance(lo + (hi - lo) * level_pct.clamp(0.0, 100.0) / 100.0)
        }
    }
}

fn classify(id: &PeripheralId, name: Option<String>, services: &[uuid::Uuid], rssi: Option<i16>) -> Option<DeviceInfo> {
    let has = |s: u16| services.iter().any(|u| *u == uuid_from_u16(s));
    let ftms = has(ftms::FTMS_SERVICE);
    let lname = name.clone().unwrap_or_default().to_lowercase();
    let kind = if ftms || lname.contains("kickr") {
        DeviceKind::Trainer
    } else if has(ftms::HEART_RATE_SERVICE) {
        DeviceKind::HeartRate
    } else if has(ftms::CYCLING_POWER_SERVICE) {
        DeviceKind::Power
    } else {
        return None;
    };
    Some(DeviceInfo {
        id: id.to_string(),
        name: name.unwrap_or_else(|| match kind {
            DeviceKind::Trainer => "Smart trainer".into(),
            DeviceKind::HeartRate => "Heart rate monitor".into(),
            DeviceKind::Power => "Power meter".into(),
        }),
        rssi,
        kind,
        ftms,
    })
}
