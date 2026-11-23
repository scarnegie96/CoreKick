//! Structured workouts. Power targets are fractions of FTP so a workout scales
//! automatically as fitness changes. `Free` blocks hand control back to the
//! rider (SIM mode at a set gradient) – used for test efforts and free riding.
//! `Set` blocks repeat a group of blocks (e.g. 3 × (4 × 30/30)).
//!
//! Also here: a compact text format (`10m 50-75%, 3x(8m @95%, 4m @55%)`),
//! Zwift `.zwo` import/export and `.erg`/`.mrc` import.

use serde::{Deserialize, Serialize};

use crate::metrics;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    Steady {
        duration_s: u32,
        power: f64,
        #[serde(default)]
        cadence: Option<u32>,
        #[serde(default)]
        label: Option<String>,
    },
    Ramp {
        duration_s: u32,
        from: f64,
        to: f64,
        #[serde(default)]
        label: Option<String>,
    },
    Intervals {
        repeats: u32,
        on_s: u32,
        on_power: f64,
        off_s: u32,
        off_power: f64,
        #[serde(default)]
        on_cadence: Option<u32>,
    },
    Free {
        duration_s: u32,
        #[serde(default)]
        grade_pct: f64,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        effort: bool,
    },
    /// Repeat a group of blocks.
    Set {
        repeats: u32,
        blocks: Vec<Block>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Workout {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub category: String,
    pub blocks: Vec<Block>,
    #[serde(default)]
    pub builtin: bool,
    #[serde(default)]
    pub created_at: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StepTarget {
    /// ERG target as fraction of FTP, linearly interpolated over the step.
    Erg { from: f64, to: f64 },
    /// Rider controls power; trainer simulates a gradient.
    Free { grade_pct: f64 },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Step {
    pub start_s: u32,
    pub duration_s: u32,
    pub target: StepTarget,
    pub label: String,
    pub effort: bool,
    pub cadence: Option<u32>,
}

impl Step {
    pub fn end_s(&self) -> u32 {
        self.start_s + self.duration_s
    }
    /// Target fraction of FTP at `t` seconds into this step.
    pub fn fraction_at(&self, t: f64) -> Option<f64> {
        match self.target {
            StepTarget::Erg { from, to } => {
                let f = if self.duration_s > 0 { (t / self.duration_s as f64).clamp(0.0, 1.0) } else { 0.0 };
                Some(from + (to - from) * f)
            }
            StepTarget::Free { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WorkoutEstimate {
    pub duration_s: u32,
    pub avg_power: f64,
    pub np: f64,
    pub intensity_factor: f64,
    pub tss: f64,
    pub work_kj: f64,
    pub zone_s: [f64; 7],
}

fn zone_label(f: f64) -> &'static str {
    match f {
        x if x <= 0.55 => "Recovery",
        x if x <= 0.75 => "Endurance",
        x if x <= 0.90 => "Tempo",
        x if x <= 1.05 => "Threshold",
        x if x <= 1.20 => "VO2 Max",
        _ => "Anaerobic",
    }
}

fn push_step(out: &mut Vec<Step>, t: &mut u32, prefix: &str, dur: u32, target: StepTarget, label: String, effort: bool, cadence: Option<u32>) {
    if dur == 0 {
        return;
    }
    let label = if prefix.is_empty() { label } else { format!("{prefix} · {label}") };
    out.push(Step { start_s: *t, duration_s: dur, target, label, effort, cadence });
    *t += dur;
}

fn expand(blocks: &[Block], prefix: &str, out: &mut Vec<Step>, t: &mut u32) {
    for b in blocks {
        match b {
            Block::Steady { duration_s, power, cadence, label } => push_step(
                out,
                t,
                prefix,
                *duration_s,
                StepTarget::Erg { from: *power, to: *power },
                label.clone().unwrap_or_else(|| zone_label(*power).into()),
                *power > 1.05,
                *cadence,
            ),
            Block::Ramp { duration_s, from, to, label } => push_step(
                out,
                t,
                prefix,
                *duration_s,
                StepTarget::Erg { from: *from, to: *to },
                label.clone().unwrap_or_else(|| if to >= from { "Ramp up".into() } else { "Ramp down".into() }),
                false,
                None,
            ),
            Block::Intervals { repeats, on_s, on_power, off_s, off_power, on_cadence } => {
                for i in 0..*repeats {
                    push_step(out, t, prefix, *on_s, StepTarget::Erg { from: *on_power, to: *on_power }, format!("Interval {}/{}", i + 1, repeats), true, *on_cadence);
                    push_step(out, t, prefix, *off_s, StepTarget::Erg { from: *off_power, to: *off_power }, "Recover".into(), false, None);
                }
            }
            Block::Free { duration_s, grade_pct, label, effort } => push_step(
                out,
                t,
                prefix,
                *duration_s,
                StepTarget::Free { grade_pct: *grade_pct },
                label.clone().unwrap_or_else(|| if *effort { "All-out effort".into() } else { "Free ride".into() }),
                *effort,
                None,
            ),
            Block::Set { repeats, blocks } => {
                for k in 0..*repeats {
                    let p = if prefix.is_empty() { format!("Set {}/{}", k + 1, repeats) } else { format!("{prefix} · Set {}/{}", k + 1, repeats) };
                    expand(blocks, &p, out, t);
                }
            }
        }
    }
}

pub fn blocks_to_steps(blocks: &[Block]) -> Vec<Step> {
    let mut out = Vec::new();
    let mut t = 0u32;
    expand(blocks, "", &mut out, &mut t);
    out
}

/// Re-number `start_s` so steps are contiguous from `from_index` onwards.
pub fn reflow(steps: &mut [Step], from_index: usize) {
    for i in from_index.max(1)..steps.len() {
        steps[i].start_s = steps[i - 1].end_s();
    }
}

pub fn estimate_steps(steps: &[Step], ftp: f64) -> WorkoutEstimate {
    let mut p = Vec::new();
    for s in steps {
        for k in 0..s.duration_s {
            let f = s.fraction_at(k as f64 + 0.5).unwrap_or(if s.effort { 0.95 } else { 0.6 });
            p.push(f * ftp);
        }
    }
    let np = metrics::normalized_power(&p);
    let secs = p.len() as f64;
    WorkoutEstimate {
        duration_s: p.len() as u32,
        avg_power: metrics::mean(&p),
        np,
        intensity_factor: metrics::intensity_factor(np, ftp),
        tss: metrics::tss(secs, np, ftp),
        work_kj: p.iter().sum::<f64>() / 1000.0,
        zone_s: metrics::time_in_zones(&p, ftp),
    }
}

impl Workout {
    pub fn steps(&self) -> Vec<Step> {
        blocks_to_steps(&self.blocks)
    }

    pub fn duration_s(&self) -> u32 {
        self.steps().last().map(|s| s.end_s()).unwrap_or(0)
    }

    /// Expected load at a given FTP. Free blocks are assumed to be ridden at
    /// 95 % FTP if marked as efforts, otherwise 60 %.
    pub fn estimate(&self, ftp: f64) -> WorkoutEstimate {
        estimate_steps(&self.steps(), ftp)
    }
}

/// Locate the step active at workout time `t`. Returns (index, seconds into step).
pub fn step_at(steps: &[Step], t: f64) -> Option<(usize, f64)> {
    if t < 0.0 {
        return None;
    }
    let i = steps.partition_point(|s| (s.end_s() as f64) <= t);
    steps.get(i).map(|s| (i, t - s.start_s as f64))
}

// ===========================================================================
// Text format
// ===========================================================================
//
//   10m 50-75%                  ramp
//   20m @ 90% 85rpm             steady with cadence
//   5x 1m @120% / 1m @50%       intervals
//   3x (30s @150%, 30s @50%)    set (nestable)
//   2m free | 1m effort @2%     rider-controlled (effort counts for tests)
//   200w / 1:30 / 1h / 45s      absolute watts, mm:ss, hours, seconds
// Items are separated by commas, semicolons or new lines.

#[derive(Clone, Copy, Debug)]
enum Spec {
    Power { from: f64, to: f64, cadence: Option<u32> },
    Free { effort: bool, grade: f64 },
}

struct Parser {
    s: Vec<char>,
    i: usize,
    ftp: f64,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }
    fn ws(&mut self) {
        while matches!(self.peek(), Some(' ') | Some('\t')) {
            self.i += 1;
        }
    }
    fn ws_all(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.i += 1;
        }
    }
    fn sep(&mut self) -> bool {
        let mut any = false;
        loop {
            self.ws();
            match self.peek() {
                Some(',') | Some(';') | Some('\n') | Some('\r') => {
                    self.i += 1;
                    any = true;
                }
                _ => return any,
            }
        }
    }
    fn err(&self, msg: &str) -> String {
        let ctx: String = self.s.iter().skip(self.i).take(18).collect();
        if ctx.trim().is_empty() {
            format!("{msg} (at end)")
        } else {
            format!("{msg} near “{}”", ctx.trim())
        }
    }
    fn word(&mut self) -> String {
        let st = self.i;
        while matches!(self.peek(), Some(c) if c.is_alphabetic()) {
            self.i += 1;
        }
        self.s[st..self.i].iter().collect::<String>().to_lowercase()
    }
    fn number(&mut self) -> Option<f64> {
        let st = self.i;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '.') {
            self.i += 1;
        }
        if st == self.i {
            return None;
        }
        self.s[st..self.i].iter().collect::<String>().parse().ok()
    }

    fn plan(&mut self, nested: bool) -> Result<Vec<Block>, String> {
        let mut out = Vec::new();
        loop {
            self.ws_all();
            match self.peek() {
                None => break,
                Some(')') if nested => break,
                _ => {}
            }
            out.extend(self.item()?);
            if !self.sep() {
                self.ws_all();
                match self.peek() {
                    None => break,
                    Some(')') if nested => break,
                    _ => return Err(self.err("Expected a comma or new line")),
                }
            }
        }
        Ok(out)
    }

    fn try_repeat(&mut self) -> Option<u32> {
        let save = self.i;
        if let Some(n) = self.number() {
            self.ws();
            if matches!(self.peek(), Some('x') | Some('X') | Some('×')) {
                self.i += 1;
                if n >= 1.0 && n.fract() == 0.0 {
                    return Some(n as u32);
                }
            }
        }
        self.i = save;
        None
    }

    fn item(&mut self) -> Result<Vec<Block>, String> {
        if let Some(n) = self.try_repeat() {
            self.ws();
            if self.peek() == Some('(') {
                self.i += 1;
                let inner = self.plan(true)?;
                self.ws_all();
                if self.peek() != Some(')') {
                    return Err(self.err("Missing ')'"));
                }
                self.i += 1;
                if inner.is_empty() {
                    return Err("Empty repeat group".into());
                }
                return Ok(vec![Block::Set { repeats: n.min(100), blocks: inner }]);
            }
            let (d1, s1) = self.segment()?;
            self.ws();
            if self.peek() == Some('/') {
                self.i += 1;
                self.ws();
                let (d2, s2) = self.segment()?;
                return Ok(vec![match (s1, s2) {
                    (Spec::Power { from: a, to: a2, cadence }, Spec::Power { from: b, to: b2, .. }) if (a - a2).abs() < 1e-9 && (b - b2).abs() < 1e-9 => {
                        Block::Intervals { repeats: n.min(100), on_s: d1, on_power: a, off_s: d2, off_power: b, on_cadence: cadence }
                    }
                    _ => Block::Set { repeats: n.min(100), blocks: vec![to_block(d1, s1), to_block(d2, s2)] },
                }]);
            }
            return Ok(vec![Block::Set { repeats: n.min(100), blocks: vec![to_block(d1, s1)] }]);
        }
        let (d, s) = self.segment()?;
        Ok(vec![to_block(d, s)])
    }

    fn segment(&mut self) -> Result<(u32, Spec), String> {
        let d = self.duration()?;
        self.ws();
        let s = self.spec()?;
        Ok((d, s))
    }

    fn duration(&mut self) -> Result<u32, String> {
        let a = self.number().ok_or_else(|| self.err("Expected a duration like 30s, 5m or 1:30"))?;
        if self.peek() == Some(':') {
            self.i += 1;
            let b = self.number().ok_or_else(|| self.err("Expected seconds after ':'"))?;
            if self.peek() == Some(':') {
                self.i += 1;
                let c = self.number().ok_or_else(|| self.err("Expected seconds"))?;
                return Ok((a * 3600.0 + b * 60.0 + c).round() as u32);
            }
            return Ok((a * 60.0 + b).round() as u32);
        }
        self.ws();
        let unit = self.word();
        let secs = match unit.as_str() {
            "s" | "sec" | "secs" | "second" | "seconds" => a,
            "m" | "min" | "mins" | "minute" | "minutes" => a * 60.0,
            "h" | "hr" | "hrs" | "hour" | "hours" => a * 3600.0,
            _ => return Err(self.err("Add a unit to the duration (s, m or h)")),
        };
        if secs < 1.0 {
            return Err("Durations must be at least 1 second".into());
        }
        Ok(secs.round() as u32)
    }

    fn power(&mut self) -> Result<f64, String> {
        let n = self.number().ok_or_else(|| self.err("Expected a power like 95% or 250w"))?;
        self.ws();
        match self.peek() {
            Some('%') => {
                self.i += 1;
                Ok(n / 100.0)
            }
            Some('w') | Some('W') => {
                self.i += 1;
                Ok(n / self.ftp.max(50.0))
            }
            _ => Ok(if n <= 3.0 { n } else { n / 100.0 }),
        }
    }

    fn spec(&mut self) -> Result<Spec, String> {
        if self.peek() == Some('@') {
            self.i += 1;
            self.ws();
        }
        let save = self.i;
        let w = self.word();
        match w.as_str() {
            "free" | "freeride" | "effort" | "max" | "allout" => {
                let effort = w != "free" && w != "freeride";
                self.ws();
                let mut grade = if effort { 1.0 } else { 0.0 };
                if self.peek() == Some('@') {
                    self.i += 1;
                    self.ws();
                    grade = self.number().ok_or_else(|| self.err("Expected a gradient"))?;
                    if self.peek() == Some('%') {
                        self.i += 1;
                    }
                }
                return Ok(Spec::Free { effort, grade });
            }
            "at" => self.ws(),
            "" => {}
            _ => {
                self.i = save;
                return Err(self.err("Expected a power (e.g. 90%), 'free' or 'effort'"));
            }
        }
        let from = self.power()?;
        self.ws();
        let mut to = from;
        let save2 = self.i;
        let mut ramp = false;
        match self.peek() {
            Some('-') | Some('→') => {
                self.i += 1;
                ramp = true;
            }
            Some('.') if self.s.get(self.i + 1) == Some(&'.') => {
                self.i += 2;
                ramp = true;
            }
            Some('t') => {
                if self.word() == "to" {
                    ramp = true;
                } else {
                    self.i = save2;
                }
            }
            _ => {}
        }
        if ramp {
            self.ws();
            to = self.power()?;
        }
        self.ws();
        let save3 = self.i;
        let mut cadence = None;
        if let Some(c) = self.number() {
            self.ws();
            if self.word() == "rpm" {
                cadence = Some(c.round() as u32);
            } else {
                self.i = save3;
            }
        }
        if !(0.0..=4.0).contains(&from) || !(0.0..=4.0).contains(&to) {
            return Err("Power must be between 0 and 400% FTP".into());
        }
        Ok(Spec::Power { from, to, cadence })
    }
}

fn to_block(d: u32, s: Spec) -> Block {
    match s {
        Spec::Power { from, to, cadence } if (from - to).abs() < 1e-9 => Block::Steady { duration_s: d, power: from, cadence, label: None },
        Spec::Power { from, to, .. } => Block::Ramp { duration_s: d, from, to, label: None },
        Spec::Free { effort, grade } => Block::Free { duration_s: d, grade_pct: grade, label: None, effort },
    }
}

/// Parse the compact text format into blocks. `ftp` converts absolute watts.
pub fn parse_text(src: &str, ftp: f64) -> Result<Vec<Block>, String> {
    let mut p = Parser { s: src.chars().collect(), i: 0, ftp };
    let blocks = p.plan(false)?;
    p.ws_all();
    if p.i < p.s.len() {
        return Err(p.err("Unexpected text"));
    }
    if blocks.is_empty() {
        return Err("Nothing to add".into());
    }
    Ok(blocks)
}

fn fmt_dur_text(s: u32) -> String {
    if s % 3600 == 0 && s >= 3600 {
        format!("{}h", s / 3600)
    } else if s % 60 == 0 {
        format!("{}m", s / 60)
    } else if s > 60 {
        format!("{}:{:02}", s / 60, s % 60)
    } else {
        format!("{s}s")
    }
}

fn pct(f: f64) -> String {
    format!("{}%", (f * 100.0).round())
}

/// Inverse of [`parse_text`] (round-trips).
pub fn to_text(blocks: &[Block]) -> String {
    let one = |b: &Block| -> String {
        match b {
            Block::Steady { duration_s, power, cadence, .. } => {
                format!("{} @{}{}", fmt_dur_text(*duration_s), pct(*power), cadence.map(|c| format!(" {c}rpm")).unwrap_or_default())
            }
            Block::Ramp { duration_s, from, to, .. } => format!("{} {}-{}", fmt_dur_text(*duration_s), pct(*from), pct(*to)),
            Block::Intervals { repeats, on_s, on_power, off_s, off_power, on_cadence } => format!(
                "{}x {} @{}{} / {} @{}",
                repeats,
                fmt_dur_text(*on_s),
                pct(*on_power),
                on_cadence.map(|c| format!(" {c}rpm")).unwrap_or_default(),
                fmt_dur_text((*off_s).max(1)),
                pct(*off_power)
            ),
            Block::Free { duration_s, grade_pct, effort, .. } => {
                format!("{} {} @{}%", fmt_dur_text(*duration_s), if *effort { "effort" } else { "free" }, grade_pct)
            }
            Block::Set { repeats, blocks } => format!("{}x ({})", repeats, to_text(blocks).replace('\n', ", ")),
        }
    };
    blocks.iter().map(one).collect::<Vec<_>>().join("\n")
}

// ===========================================================================
// File formats
// ===========================================================================

fn attr_f(n: &roxmltree::Node, names: &[&str]) -> Option<f64> {
    names.iter().find_map(|k| n.attribute(*k).and_then(|v| v.trim().parse::<f64>().ok()))
}

/// Zwift workout (.zwo).
pub fn from_zwo(text: &str) -> Result<Workout, String> {
    let doc = roxmltree::Document::parse(text).map_err(|e| format!("Invalid ZWO/XML: {e}"))?;
    let root = doc.root_element();
    let child_text = |tag: &str| {
        root.children().find(|c| c.tag_name().name().eq_ignore_ascii_case(tag)).and_then(|c| c.text()).map(|s| s.trim().to_string())
    };
    let wk = root
        .descendants()
        .find(|n| n.tag_name().name().eq_ignore_ascii_case("workout"))
        .ok_or("No <workout> section found")?;
    let mut blocks = Vec::new();
    for n in wk.children().filter(|n| n.is_element()) {
        let dur = attr_f(&n, &["Duration", "duration"]).unwrap_or(0.0).round() as u32;
        let cad = attr_f(&n, &["Cadence", "cadence"]).map(|c| c.round() as u32);
        let tag = n.tag_name().name().to_ascii_lowercase();
        let b = match tag.as_str() {
            "warmup" | "cooldown" | "ramp" => {
                let lo = attr_f(&n, &["PowerLow", "powerlow"]).unwrap_or(0.5);
                let hi = attr_f(&n, &["PowerHigh", "powerhigh"]).unwrap_or(lo);
                let label = match tag.as_str() {
                    "warmup" => Some("Warm-up".into()),
                    "cooldown" => Some("Cool-down".into()),
                    _ => None,
                };
                Block::Ramp { duration_s: dur, from: lo, to: hi, label }
            }
            "steadystate" | "solidstate" => {
                let p = attr_f(&n, &["Power", "power"])
                    .or_else(|| Some((attr_f(&n, &["PowerLow"])? + attr_f(&n, &["PowerHigh"])?) / 2.0))
                    .unwrap_or(0.6);
                Block::Steady { duration_s: dur, power: p, cadence: cad, label: None }
            }
            "intervalst" => Block::Intervals {
                repeats: attr_f(&n, &["Repeat", "repeat"]).unwrap_or(1.0).round().max(1.0) as u32,
                on_s: attr_f(&n, &["OnDuration"]).unwrap_or(60.0).round() as u32,
                off_s: attr_f(&n, &["OffDuration"]).unwrap_or(60.0).round() as u32,
                on_power: attr_f(&n, &["OnPower", "PowerOnHigh", "PowerOnLow"]).unwrap_or(1.0),
                off_power: attr_f(&n, &["OffPower", "PowerOffLow", "PowerOffHigh"]).unwrap_or(0.5),
                on_cadence: cad,
            },
            "freeride" => Block::Free { duration_s: dur, grade_pct: 0.0, label: None, effort: false },
            "maxeffort" => Block::Free { duration_s: dur, grade_pct: 1.0, label: Some("Max effort".into()), effort: true },
            _ => continue,
        };
        blocks.push(b);
    }
    if blocks.is_empty() {
        return Err("The workout has no supported blocks".into());
    }
    Ok(Workout {
        id: String::new(),
        name: child_text("name").filter(|s| !s.is_empty()).unwrap_or_else(|| "Imported workout".into()),
        description: child_text("description").unwrap_or_default(),
        category: child_text("category").filter(|s| !s.is_empty()).unwrap_or_else(|| "Imported".into()),
        blocks,
        builtin: false,
        created_at: String::new(),
    })
}

fn xml_esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn zwo_blocks(blocks: &[Block], out: &mut String) {
    for b in blocks {
        match b {
            Block::Steady { duration_s, power, cadence, .. } => out.push_str(&format!(
                "    <SteadyState Duration=\"{duration_s}\" Power=\"{power:.3}\"{}/>\n",
                cadence.map(|c| format!(" Cadence=\"{c}\"")).unwrap_or_default()
            )),
            Block::Ramp { duration_s, from, to, .. } => {
                out.push_str(&format!("    <Ramp Duration=\"{duration_s}\" PowerLow=\"{from:.3}\" PowerHigh=\"{to:.3}\"/>\n"))
            }
            Block::Intervals { repeats, on_s, on_power, off_s, off_power, on_cadence } => out.push_str(&format!(
                "    <IntervalsT Repeat=\"{repeats}\" OnDuration=\"{on_s}\" OffDuration=\"{off_s}\" OnPower=\"{on_power:.3}\" OffPower=\"{off_power:.3}\"{}/>\n",
                on_cadence.map(|c| format!(" Cadence=\"{c}\"")).unwrap_or_default()
            )),
            Block::Free { duration_s, effort, .. } => {
                if *effort {
                    out.push_str(&format!("    <MaxEffort Duration=\"{duration_s}\"/>\n"))
                } else {
                    out.push_str(&format!("    <FreeRide Duration=\"{duration_s}\" FlatRoad=\"1\"/>\n"))
                }
            }
            // ZWO has no nesting: unroll sets.
            Block::Set { repeats, blocks } => {
                for _ in 0..*repeats {
                    zwo_blocks(blocks, out);
                }
            }
        }
    }
}

pub fn to_zwo(w: &Workout) -> String {
    let mut out = String::from("<workout_file>\n");
    out.push_str("  <author>CoreKick</author>\n");
    out.push_str(&format!("  <name>{}</name>\n", xml_esc(&w.name)));
    out.push_str(&format!("  <description>{}</description>\n", xml_esc(&w.description)));
    out.push_str("  <sportType>bike</sportType>\n");
    out.push_str(&format!("  <category>{}</category>\n", xml_esc(&w.category)));
    out.push_str("  <workout>\n");
    zwo_blocks(&w.blocks, &mut out);
    out.push_str("  </workout>\n</workout_file>\n");
    out
}

/// `.erg` (watts) or `.mrc` (percent) course files: piecewise-linear profiles.
pub fn from_erg_mrc(text: &str, ftp: f64) -> Result<Workout, String> {
    let mut name = None;
    let mut desc = String::new();
    let mut file_ftp = None;
    let mut percent = false;
    let mut in_data = false;
    let mut pts: Vec<(f64, f64)> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        let up = line.to_ascii_uppercase();
        if up.starts_with("[COURSE DATA]") {
            in_data = true;
            continue;
        }
        if up.starts_with("[END COURSE DATA]") {
            in_data = false;
            continue;
        }
        if up.starts_with('[') {
            continue;
        }
        if !in_data {
            if let Some((k, v)) = line.split_once('=') {
                match k.trim().to_ascii_uppercase().as_str() {
                    "FILE NAME" => name = Some(v.trim().to_string()),
                    "DESCRIPTION" => desc = v.trim().to_string(),
                    "FTP" => file_ftp = v.trim().parse::<f64>().ok(),
                    _ => {}
                }
            } else if up.contains("PERCENT") {
                percent = true;
            }
            continue;
        }
        let mut it = line.split_whitespace();
        if let (Some(a), Some(b)) = (it.next(), it.next()) {
            if let (Ok(t), Ok(v)) = (a.parse::<f64>(), b.parse::<f64>()) {
                pts.push((t * 60.0, v));
            }
        }
    }
    if pts.len() < 2 {
        return Err("No [COURSE DATA] found – is this an .erg or .mrc file?".into());
    }
    let base = file_ftp.unwrap_or(ftp).max(50.0);
    let frac = |v: f64| if percent { v / 100.0 } else { v / base };
    let mut blocks = Vec::new();
    for w in pts.windows(2) {
        let (t0, v0) = w[0];
        let (t1, v1) = w[1];
        let d = (t1 - t0).round();
        if d < 1.0 {
            continue;
        }
        let (a, b) = (frac(v0), frac(v1));
        blocks.push(if (a - b).abs() < 1e-6 {
            Block::Steady { duration_s: d as u32, power: a, cadence: None, label: None }
        } else {
            Block::Ramp { duration_s: d as u32, from: a, to: b, label: None }
        });
    }
    // Merge consecutive identical steady blocks.
    let mut merged: Vec<Block> = Vec::new();
    for b in blocks {
        if let (Some(Block::Steady { duration_s, power, .. }), Block::Steady { duration_s: d2, power: p2, .. }) = (merged.last_mut(), &b) {
            if (*power - *p2).abs() < 1e-6 {
                *duration_s += d2;
                continue;
            }
        }
        merged.push(b);
    }
    Ok(Workout {
        id: String::new(),
        name: name.unwrap_or_else(|| "Imported workout".into()),
        description: desc,
        category: "Imported".into(),
        blocks: merged,
        builtin: false,
        created_at: String::new(),
    })
}

/// Detect the format from the file name / content and import.
pub fn import_file(file_name: &str, text: &str, ftp: f64) -> Result<Workout, String> {
    let lower = file_name.to_ascii_lowercase();
    if lower.ends_with(".zwo") || text.trim_start().starts_with('<') {
        from_zwo(text)
    } else if lower.ends_with(".erg") || lower.ends_with(".mrc") || text.to_ascii_uppercase().contains("[COURSE DATA]") {
        from_erg_mrc(text, ftp)
    } else {
        // Fall back to the text format.
        parse_text(text, ftp).map(|blocks| Workout {
            id: String::new(),
            name: file_name.rsplit('/').next().unwrap_or("Imported").trim_end_matches(".txt").to_string(),
            description: String::new(),
            category: "Imported".into(),
            blocks,
            builtin: false,
            created_at: String::new(),
        })
    }
}

// ===========================================================================
// Built-in library
// ===========================================================================

fn w(id: &str, name: &str, cat: &str, desc: &str, blocks: Vec<Block>) -> Workout {
    Workout {
        id: format!("builtin-{id}"),
        name: name.into(),
        description: desc.into(),
        category: cat.into(),
        blocks,
        builtin: true,
        created_at: String::new(),
    }
}

fn steady(d: u32, p: f64) -> Block {
    Block::Steady { duration_s: d, power: p, cadence: None, label: None }
}
fn steady_c(d: u32, p: f64, c: u32, label: &str) -> Block {
    Block::Steady { duration_s: d, power: p, cadence: Some(c), label: Some(label.into()) }
}
fn ramp(d: u32, a: f64, b: f64) -> Block {
    Block::Ramp { duration_s: d, from: a, to: b, label: None }
}
fn reps(n: u32, on: u32, onp: f64, off: u32, offp: f64) -> Block {
    Block::Intervals { repeats: n, on_s: on, on_power: onp, off_s: off, off_power: offp, on_cadence: None }
}
fn set(n: u32, blocks: Vec<Block>) -> Block {
    Block::Set { repeats: n, blocks }
}
fn warmup() -> Block {
    Block::Ramp { duration_s: 600, from: 0.45, to: 0.75, label: Some("Warm-up".into()) }
}
fn cooldown() -> Block {
    Block::Ramp { duration_s: 300, from: 0.6, to: 0.4, label: Some("Cool-down".into()) }
}

pub fn builtin_workouts() -> Vec<Workout> {
    vec![
        // ---- Recovery ----
        w("recovery-spin", "Recovery Spin", "Recovery", "Easy, high-cadence spin to flush the legs.",
          vec![ramp(300, 0.4, 0.5), Block::Steady { duration_s: 1500, power: 0.5, cadence: Some(95), label: Some("Easy spin".into()) }, ramp(300, 0.5, 0.4)]),
        w("recovery-45", "Active Recovery 45", "Recovery", "45 minutes barely above coffee-ride pace, with a few cadence lifts to keep the legs supple.",
          vec![ramp(300, 0.4, 0.5), set(4, vec![steady(450, 0.5), steady_c(90, 0.52, 105, "Spin-up")]), ramp(240, 0.5, 0.4)]),
        // ---- Endurance ----
        w("endurance-60", "Endurance 60", "Endurance", "An hour of steady Zone 2 aerobic work.",
          vec![warmup(), steady(2700, 0.68), cooldown()]),
        w("endurance-90", "Endurance 90", "Endurance", "90 minutes of Zone 2 with gentle variation to stay engaged.",
          vec![warmup(), set(4, vec![steady(780, 0.68), steady(240, 0.74)]), steady(420, 0.65), cooldown()]),
        w("long-ride-2h", "Long Ride 2h", "Endurance", "Two hours of aerobic base. Fuel it like an outdoor ride.",
          vec![warmup(), set(3, vec![steady(1600, 0.68), steady(300, 0.75)]), steady(600, 0.65), cooldown()]),
        w("endurance-bursts", "Endurance + Bursts", "Endurance", "Zone 2 hour with a 10-second burst every 5 minutes to recruit fast-twitch fibres.",
          vec![warmup(), set(9, vec![steady(290, 0.68), Block::Steady { duration_s: 10, power: 1.6, cadence: Some(100), label: Some("Burst".into()) }]), cooldown()]),
        w("cadence-pyramid", "Cadence Pyramid", "Endurance", "Zone 2 power, climbing cadence targets 85 → 120 rpm and back. Smoothness drill.",
          vec![warmup(), set(2, vec![steady_c(180, 0.65, 85, "85 rpm"), steady_c(180, 0.65, 95, "95 rpm"), steady_c(120, 0.65, 105, "105 rpm"), steady_c(60, 0.65, 115, "115 rpm"), steady_c(60, 0.62, 120, "120 rpm"), steady_c(120, 0.65, 105, "105 rpm"), steady_c(180, 0.65, 90, "90 rpm")]), cooldown()]),
        // ---- Tempo ----
        w("tempo-3x12", "Tempo 3×12", "Tempo", "Three 12-minute tempo blocks to build muscular endurance.",
          vec![warmup(), reps(3, 720, 0.85, 240, 0.55), cooldown()]),
        w("tempo-climb", "Low-Cadence Tempo", "Tempo", "Two 20-minute tempo efforts at 65–70 rpm – strength-endurance for long climbs.",
          vec![warmup(), steady_c(1200, 0.83, 68, "Big-gear tempo"), steady(300, 0.55), steady_c(1200, 0.83, 68, "Big-gear tempo"), cooldown()]),
        w("tempo-surges", "Tempo with Surges", "Tempo", "Tempo blocks broken by 30 s surges – like holding a wheel in a group.",
          vec![warmup(), set(3, vec![set(4, vec![steady(150, 0.82), Block::Steady { duration_s: 30, power: 1.2, cadence: None, label: Some("Surge".into()) }]), steady(240, 0.55)]), cooldown()]),
        // ---- Sweet spot ----
        w("sweet-spot-2x20", "Sweet Spot 2×20", "Sweet Spot", "The classic: two 20-minute blocks at 88–93 % FTP.",
          vec![warmup(), reps(2, 1200, 0.9, 300, 0.5), cooldown()]),
        w("sweet-spot-3x15", "Sweet Spot 3×15", "Sweet Spot", "Three 15-minute sweet-spot intervals with 5-minute recoveries.",
          vec![warmup(), reps(3, 900, 0.9, 300, 0.5), cooldown()]),
        w("sweet-spot-4x12", "Sweet Spot 4×12", "Sweet Spot", "Four 12-minute blocks, each one a touch harder: 88 → 94 %.",
          vec![warmup(), steady(720, 0.88), steady(240, 0.5), steady(720, 0.9), steady(240, 0.5), steady(720, 0.92), steady(240, 0.5), steady(720, 0.94), cooldown()]),
        w("hour-of-power", "Hour of Power", "Sweet Spot", "A continuous 40 minutes at 85–90 % FTP. Mental toughness included.",
          vec![warmup(), ramp(2400, 0.85, 0.9), cooldown()]),
        // ---- Threshold ----
        w("threshold-3x10", "Threshold 3×10", "Threshold", "Three 10-minute efforts right at FTP.",
          vec![warmup(), reps(3, 600, 1.0, 300, 0.5), cooldown()]),
        w("threshold-2x20", "Threshold 2×20", "Threshold", "Two 20-minute efforts at 95–100 % FTP. The benchmark FTP builder.",
          vec![warmup(), reps(2, 1200, 0.97, 360, 0.5), cooldown()]),
        w("over-unders", "Over-Unders", "Threshold", "Alternate 2 min at 95 % and 1 min at 110 % to teach lactate clearance.",
          vec![warmup(),
               Block::Intervals { repeats: 3, on_s: 120, on_power: 0.95, off_s: 60, off_power: 1.1, on_cadence: None },
               steady(300, 0.5),
               Block::Intervals { repeats: 3, on_s: 120, on_power: 0.95, off_s: 60, off_power: 1.1, on_cadence: None },
               cooldown()]),
        w("under-over-sets", "Under-Over Sets", "Threshold", "3 sets of (4 × 2 min @ 95 % / 1 min @ 105 %) with 5-minute recoveries.",
          vec![warmup(), set(3, vec![set(4, vec![steady(120, 0.95), steady(60, 1.05)]), steady(300, 0.5)]), cooldown()]),
        w("threshold-ladder", "Threshold Ladder", "Threshold", "4-6-8-6-4 minutes at threshold with short recoveries.",
          vec![warmup(), steady(240, 1.0), steady(120, 0.5), steady(360, 1.0), steady(180, 0.5), steady(480, 0.98), steady(240, 0.5), steady(360, 1.0), steady(180, 0.5), steady(240, 1.02), cooldown()]),
        // ---- VO2 max ----
        w("vo2-5x3", "VO2 Max 5×3", "VO2 Max", "Five 3-minute efforts at 120 % FTP with equal recovery.",
          vec![warmup(), steady(180, 0.95), steady(180, 0.5), reps(5, 180, 1.2, 180, 0.5), cooldown()]),
        w("norwegian-4x4", "Norwegian 4×4", "VO2 Max", "4 × 4 minutes at 112–115 % with 3-minute recoveries. Simple, brutal, effective.",
          vec![warmup(), reps(4, 240, 1.13, 180, 0.55), cooldown()]),
        w("micro-30-30", "30/30s", "VO2 Max", "Two sets of 10 × 30 s on / 30 s off at 130 %.",
          vec![warmup(), reps(10, 30, 1.3, 30, 0.5), steady(300, 0.5), reps(10, 30, 1.3, 30, 0.5), cooldown()]),
        w("ronnestad-30-15", "Rønnestad 30/15s", "VO2 Max", "3 sets of 13 × 30 s @ 120 % / 15 s @ 50 % – the research-backed VO2 session.",
          vec![warmup(), set(3, vec![reps(13, 30, 1.2, 15, 0.5), steady(180, 0.5)]), cooldown()]),
        w("40-20s", "40/20s", "VO2 Max", "3 sets of 8 × 40 s hard / 20 s easy.",
          vec![warmup(), set(3, vec![reps(8, 40, 1.2, 20, 0.5), steady(300, 0.5)]), cooldown()]),
        // ---- Anaerobic / sprint ----
        w("tabata", "Tabata Blast", "Anaerobic", "8 × 20 s all-out (170 %) / 10 s rest. Short and brutal.",
          vec![warmup(), steady(120, 0.5), reps(8, 20, 1.7, 10, 0.4), steady(300, 0.5), cooldown()]),
        w("anaerobic-6x1", "Anaerobic 6×1", "Anaerobic", "Six 1-minute efforts at 140 % with full recovery.",
          vec![warmup(), reps(6, 60, 1.4, 240, 0.5), cooldown()]),
        w("sprint-primer", "Sprint Primer", "Anaerobic", "Six seated/standing 15 s sprints with full recovery.",
          vec![warmup(), reps(6, 15, 2.0, 285, 0.5), cooldown()]),
        w("sprint-power", "Sprint Power", "Anaerobic", "8 × 10-second max sprints in resistance mode – you choose the gear.",
          vec![warmup(), set(8, vec![Block::Free { duration_s: 10, grade_pct: 2.0, label: Some("Sprint!".into()), effort: true }, steady(170, 0.5)]), cooldown()]),
        // ---- Mixed / race ----
        w("pyramid", "Pyramid", "Mixed", "1-2-3-4-3-2-1 minute steps climbing to VO2 and back.",
          vec![warmup(), steady(60, 0.9), steady(60, 0.5), steady(120, 1.0), steady(60, 0.5), steady(180, 1.05), steady(90, 0.5),
               steady(240, 1.1), steady(120, 0.5), steady(180, 1.05), steady(90, 0.5), steady(120, 1.0), steady(60, 0.5), steady(60, 0.9), cooldown()]),
        w("race-sim", "Race Simulation", "Mixed", "Unpredictable surges, a 5-minute break attempt and a sprint finish.",
          vec![warmup(), steady(300, 0.75), set(4, vec![steady(20, 1.5), steady(100, 0.8)]), steady(300, 1.05), steady(180, 0.7),
               set(6, vec![steady(30, 1.3), steady(60, 0.85)]), steady(240, 0.95), Block::Free { duration_s: 20, grade_pct: 1.0, label: Some("Sprint finish".into()), effort: true }, cooldown()]),
        w("openers", "Pre-Race Openers", "Mixed", "Short day-before session: Zone 2 with 3 × 1 min at 110 % and 3 short sprints.",
          vec![warmup(), steady(300, 0.65), reps(3, 60, 1.1, 120, 0.55), reps(3, 10, 1.8, 110, 0.55), steady(300, 0.6), cooldown()]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_are_contiguous() {
        for wk in builtin_workouts() {
            let s = wk.steps();
            assert!(!s.is_empty(), "{}", wk.name);
            for pair in s.windows(2) {
                assert_eq!(pair[0].end_s(), pair[1].start_s, "{}", wk.name);
            }
        }
        assert!(builtin_workouts().len() >= 30);
    }

    #[test]
    fn names_match_durations() {
        let d = |id: &str| builtin_workouts().into_iter().find(|w| w.id == id).unwrap().duration_s();
        assert_eq!(d("builtin-recovery-45"), 45 * 60);
        assert_eq!(d("builtin-endurance-60"), 60 * 60);
        assert_eq!(d("builtin-endurance-90"), 90 * 60);
        assert_eq!(d("builtin-long-ride-2h"), 120 * 60);
    }

    #[test]
    fn builtin_ids_unique() {
        let mut ids: Vec<String> = builtin_workouts().into_iter().map(|w| w.id).collect();
        ids.sort();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n);
    }

    #[test]
    fn sweet_spot_duration_and_tss() {
        let wk = builtin_workouts().into_iter().find(|w| w.id == "builtin-sweet-spot-2x20").unwrap();
        assert_eq!(wk.duration_s(), 600 + 2 * 1200 + 2 * 300 + 300);
        let e = wk.estimate(250.0);
        assert!(e.tss > 55.0 && e.tss < 85.0, "{}", e.tss);
        assert!(e.intensity_factor > 0.7 && e.intensity_factor < 0.9);
    }

    #[test]
    fn step_lookup_and_interpolation() {
        let steps = blocks_to_steps(&[ramp(100, 0.5, 1.0), steady(50, 1.2)]);
        let (i, into) = step_at(&steps, 50.0).unwrap();
        assert_eq!(i, 0);
        assert!((steps[i].fraction_at(into).unwrap() - 0.75).abs() < 1e-9);
        assert_eq!(step_at(&steps, 100.0).unwrap().0, 1);
        assert!(step_at(&steps, 150.0).is_none());
    }

    #[test]
    fn intervals_and_sets_expand() {
        let s = blocks_to_steps(&[reps(3, 60, 1.2, 30, 0.5)]);
        assert_eq!(s.len(), 6);
        assert_eq!(s[0].label, "Interval 1/3");
        assert!(s[0].effort);
        let s = blocks_to_steps(&[set(2, vec![reps(2, 30, 1.3, 30, 0.5), steady(60, 0.5)])]);
        assert_eq!(s.len(), 10);
        assert_eq!(s[0].label, "Set 1/2 · Interval 1/2");
        assert_eq!(s[9].label, "Set 2/2 · Recovery");
        assert_eq!(s.last().unwrap().end_s(), 2 * (30 + 30 + 30 + 30 + 60));
    }

    #[test]
    fn serde_roundtrip_with_sets() {
        let wk = builtin_workouts().into_iter().find(|w| w.id == "builtin-under-over-sets").unwrap();
        let j = serde_json::to_string(&wk).unwrap();
        let back: Workout = serde_json::from_str(&j).unwrap();
        assert_eq!(wk, back);
    }

    #[test]
    fn text_parser_basics() {
        let b = parse_text("10m 50-75%, 3x(8m @95%, 4m @55%)\n5x 1m @120% 100rpm / 1m @50%\n2m free, 30s effort @2%\n1:30 @200w", 200.0).unwrap();
        assert_eq!(b.len(), 6);
        assert_eq!(b[0], Block::Ramp { duration_s: 600, from: 0.5, to: 0.75, label: None });
        match &b[1] {
            Block::Set { repeats, blocks } => {
                assert_eq!(*repeats, 3);
                assert_eq!(blocks.len(), 2);
            }
            x => panic!("{x:?}"),
        }
        assert_eq!(b[2], Block::Intervals { repeats: 5, on_s: 60, on_power: 1.2, off_s: 60, off_power: 0.5, on_cadence: Some(100) });
        assert_eq!(b[3], Block::Free { duration_s: 120, grade_pct: 0.0, label: None, effort: false });
        assert_eq!(b[4], Block::Free { duration_s: 30, grade_pct: 2.0, label: None, effort: true });
        assert_eq!(b[5], Block::Steady { duration_s: 90, power: 1.0, cadence: None, label: None });
    }

    #[test]
    fn text_parser_errors_are_helpful() {
        assert!(parse_text("10 @50%", 200.0).unwrap_err().contains("unit"));
        assert!(parse_text("3x(5m @90%", 200.0).unwrap_err().contains(")"));
        assert!(parse_text("5m banana", 200.0).is_err());
        assert!(parse_text("", 200.0).is_err());
    }

    #[test]
    fn text_roundtrip() {
        for wk in builtin_workouts() {
            let t = to_text(&wk.blocks);
            let back = parse_text(&t, 250.0).unwrap_or_else(|e| panic!("{}: {e}\n{t}", wk.name));
            let a = estimate_steps(&blocks_to_steps(&back), 250.0);
            let b = wk.estimate(250.0);
            assert_eq!(a.duration_s, b.duration_s, "{}", wk.name);
            assert!((a.tss - b.tss).abs() < 0.6, "{} {} {}", wk.name, a.tss, b.tss);
        }
    }

    #[test]
    fn zwo_roundtrip() {
        let wk = builtin_workouts().into_iter().find(|w| w.id == "builtin-over-unders").unwrap();
        let x = to_zwo(&wk);
        let back = from_zwo(&x).unwrap();
        assert_eq!(back.name, wk.name);
        assert_eq!(back.duration_s(), wk.duration_s());
        let z = r#"<workout_file><name>Test</name><workout>
            <Warmup Duration="600" PowerLow="0.25" PowerHigh="0.75"/>
            <IntervalsT Repeat="5" OnDuration="60" OffDuration="60" OnPower="1.2" OffPower="0.5" Cadence="100"/>
            <FreeRide Duration="300" FlatRoad="1"/>
            <MaxEffort Duration="20"/>
            <Cooldown Duration="300" PowerLow="0.6" PowerHigh="0.3"/>
          </workout></workout_file>"#;
        let w2 = from_zwo(z).unwrap();
        assert_eq!(w2.duration_s(), 600 + 600 + 300 + 20 + 300);
        assert_eq!(w2.blocks.len(), 5);
    }

    #[test]
    fn erg_and_mrc_import() {
        let erg = "[COURSE HEADER]\nFTP = 250\nMINUTES WATTS\n[END COURSE HEADER]\n[COURSE DATA]\n0 125\n10 200\n10 250\n20 250\n20 125\n25 125\n[END COURSE DATA]\n";
        let w = from_erg_mrc(erg, 200.0).unwrap();
        assert_eq!(w.duration_s(), 1500);
        assert_eq!(w.blocks[0], Block::Ramp { duration_s: 600, from: 0.5, to: 0.8, label: None });
        assert_eq!(w.blocks[1], Block::Steady { duration_s: 600, power: 1.0, cadence: None, label: None });
        let mrc = "[COURSE HEADER]\nMINUTES PERCENT\n[END COURSE HEADER]\n[COURSE DATA]\n0 50\n5 50\n5 100\n8 100\n[END COURSE DATA]";
        let w = from_erg_mrc(mrc, 200.0).unwrap();
        assert_eq!(w.blocks.len(), 2);
        assert!(import_file("a.mrc", mrc, 200.0).is_ok());
        assert!(import_file("plan.txt", "10m @60%", 200.0).is_ok());
    }
}
