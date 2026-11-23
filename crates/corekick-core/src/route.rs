//! Routes: elevation profiles, statistics, climb detection, a seeded generator,
//! a segment builder and GPX import.

use serde::{Deserialize, Serialize};

use crate::physics::{steady_speed, PhysicsParams};
use crate::rng::Rng;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct RoutePoint {
    /// Cumulative distance from start (m)
    pub d: f64,
    /// Elevation (m)
    pub ele: f64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteSource {
    Generated,
    Custom,
    Gpx,
    Builtin,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Terrain {
    Flat,
    Rolling,
    Hilly,
    Mountainous,
    Alpine,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Finish {
    Free,
    Loop,
    Summit,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GenParams {
    pub distance_km: f64,
    pub target_gain_m: f64,
    pub max_grade_pct: f64,
    pub terrain: Terrain,
    pub finish: Finish,
    pub seed: u64,
    #[serde(default = "default_start_ele")]
    pub start_ele_m: f64,
    /// 0 = choppy, 1 = very smooth
    #[serde(default = "default_smooth")]
    pub smoothness: f64,
}

fn default_start_ele() -> f64 {
    120.0
}
fn default_smooth() -> f64 {
    0.5
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct SegmentSpec {
    pub length_m: f64,
    pub grade_pct: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Route {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub source: RouteSource,
    #[serde(default)]
    pub created_at: String,
    pub points: Vec<RoutePoint>,
    #[serde(default)]
    pub is_loop: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub gen_params: Option<GenParams>,
    #[serde(default)]
    pub segments: Option<Vec<SegmentSpec>>,
    #[serde(default)]
    pub favourite: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Climb {
    pub start_d: f64,
    pub end_d: f64,
    pub length_m: f64,
    pub gain_m: f64,
    pub avg_grade_pct: f64,
    pub max_grade_pct: f64,
    pub start_ele: f64,
    pub top_ele: f64,
    /// Strava-style score: length (m) × average grade (%)
    pub score: f64,
    pub category: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RouteStats {
    pub distance_m: f64,
    pub gain_m: f64,
    pub loss_m: f64,
    pub min_ele: f64,
    pub max_ele: f64,
    /// Steepest 100 m section (%)
    pub max_grade_pct: f64,
    /// Steepest 100 m descent (%), negative
    pub min_grade_pct: f64,
    /// Climbing metres per km
    pub climb_density: f64,
    /// Share of distance at ≥ 3 % (0..1)
    pub climbing_share: f64,
    pub climbs: Vec<Climb>,
    /// 0..10
    pub difficulty: f64,
    pub difficulty_label: String,
}

impl Route {
    pub fn distance(&self) -> f64 {
        self.points.last().map(|p| p.d).unwrap_or(0.0)
    }

    pub fn elevation_at(&self, d: f64) -> f64 {
        let pts = &self.points;
        match pts.len() {
            0 => 0.0,
            1 => pts[0].ele,
            n => {
                if d <= pts[0].d {
                    return pts[0].ele;
                }
                if d >= pts[n - 1].d {
                    return pts[n - 1].ele;
                }
                let i = pts.partition_point(|p| p.d <= d);
                let (a, b) = (pts[i - 1], pts[i]);
                let f = if b.d > a.d { (d - a.d) / (b.d - a.d) } else { 0.0 };
                a.ele + f * (b.ele - a.ele)
            }
        }
    }

    /// Gradient (fraction) at `d` measured over ±`half_window` metres.
    pub fn grade_at(&self, d: f64, half_window: f64) -> f64 {
        let total = self.distance();
        if total <= 0.0 {
            return 0.0;
        }
        let h = half_window.max(1.0);
        let a = (d - h).max(0.0);
        let b = (d + h).min(total);
        if b - a < 1.0 {
            return 0.0;
        }
        (self.elevation_at(b) - self.elevation_at(a)) / (b - a)
    }

    /// Downsample to ~n points for thumbnails.
    pub fn thumbnail(&self, n: usize) -> Vec<[f64; 2]> {
        let total = self.distance();
        if n < 2 || total <= 0.0 {
            return self.points.iter().map(|p| [p.d, p.ele]).collect();
        }
        (0..n)
            .map(|i| {
                let d = total * i as f64 / (n - 1) as f64;
                [d, self.elevation_at(d)]
            })
            .collect()
    }

    /// Elevation series on a uniform grid (step metres).
    fn uniform(&self, step: f64) -> Vec<(f64, f64)> {
        let total = self.distance();
        let n = (total / step).ceil() as usize;
        let mut v: Vec<(f64, f64)> = (0..=n)
            .map(|i| {
                let d = (i as f64 * step).min(total);
                (d, self.elevation_at(d))
            })
            .collect();
        v.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9);
        v
    }

    pub fn stats(&self) -> RouteStats {
        let total = self.distance();
        let (mut gain, mut loss) = (0.0, 0.0);
        let mut min_ele = f64::INFINITY;
        let mut max_ele = f64::NEG_INFINITY;
        for w in self.points.windows(2) {
            let de = w[1].ele - w[0].ele;
            if de > 0.0 {
                gain += de
            } else {
                loss -= de
            }
        }
        for p in &self.points {
            min_ele = min_ele.min(p.ele);
            max_ele = max_ele.max(p.ele);
        }
        if self.points.is_empty() {
            min_ele = 0.0;
            max_ele = 0.0;
        }

        // Grades over 100 m windows
        let grid = self.uniform(25.0);
        let (mut max_g, mut min_g) = (0.0_f64, 0.0_f64);
        let mut climbing_m = 0.0;
        for i in 0..grid.len() {
            let j = (i + 4).min(grid.len() - 1);
            if j == i {
                continue;
            }
            let dd = grid[j].0 - grid[i].0;
            if dd < 50.0 {
                continue;
            }
            let g = (grid[j].1 - grid[i].1) / dd * 100.0;
            max_g = max_g.max(g);
            min_g = min_g.min(g);
        }
        for w in grid.windows(2) {
            let dd = w[1].0 - w[0].0;
            if dd > 0.0 && (w[1].1 - w[0].1) / dd >= 0.03 {
                climbing_m += dd;
            }
        }

        let climbs = detect_climbs(&grid);
        let km = (total / 1000.0).max(0.001);
        let density = gain / km;
        let difficulty = difficulty_score(gain, density, max_g);
        RouteStats {
            distance_m: total,
            gain_m: gain,
            loss_m: loss,
            min_ele,
            max_ele,
            max_grade_pct: max_g,
            min_grade_pct: min_g,
            climb_density: density,
            climbing_share: if total > 0.0 { climbing_m / total } else { 0.0 },
            climbs,
            difficulty,
            difficulty_label: difficulty_label(difficulty).into(),
        }
    }

    /// Estimated riding time (s) holding `watts` throughout.
    /// Descents are capped at 65 km/h (real riders brake).
    pub fn estimate_time(&self, watts: f64, params: &PhysicsParams) -> f64 {
        let grid = self.uniform(50.0);
        let mut t = 0.0;
        for w in grid.windows(2) {
            let dd = w[1].0 - w[0].0;
            if dd <= 0.0 {
                continue;
            }
            let g = (w[1].1 - w[0].1) / dd;
            let v = steady_speed(watts, g, w[0].1, params).clamp(1.0, 18.0);
            t += dd / v;
        }
        t
    }
}

fn detect_climbs(grid: &[(f64, f64)]) -> Vec<Climb> {
    let mut out = Vec::new();
    if grid.len() < 3 {
        return out;
    }
    let e: Vec<f64> = grid.iter().map(|p| p.1).collect();
    let mut start = 0usize;
    let mut peak = 0usize;
    for i in 1..e.len() {
        if e[i] <= e[start] && peak == start {
            start = i;
            peak = i;
            continue;
        }
        if e[i] >= e[peak] {
            peak = i;
        } else {
            let drop = e[peak] - e[i];
            let gain = e[peak] - e[start];
            if drop > (10.0_f64).max(0.25 * gain) {
                if let Some(c) = make_climb(grid, start, peak) {
                    out.push(c);
                }
                start = i;
                peak = i;
            }
        }
        if e[i] < e[start] {
            start = i;
            peak = i;
        }
    }
    if let Some(c) = make_climb(grid, start, peak) {
        out.push(c);
    }
    out
}

fn make_climb(grid: &[(f64, f64)], s: usize, p: usize) -> Option<Climb> {
    let slope = |i: usize| {
        let dd = grid[i + 1].0 - grid[i].0;
        if dd > 0.0 { (grid[i + 1].1 - grid[i].1) / dd } else { 0.0 }
    };
    // Trim flat lead-in and run-out (< 1 %).
    let (mut s, mut p) = (s, p);
    while s < p && slope(s) < 0.01 {
        s += 1;
    }
    while p > s && slope(p - 1) < 0.01 {
        p -= 1;
    }
    if p <= s {
        return None;
    }
    let length = grid[p].0 - grid[s].0;
    let gain = grid[p].1 - grid[s].1;
    if length < 300.0 || gain < 15.0 {
        return None;
    }
    let avg = gain / length * 100.0;
    if avg < 2.0 {
        return None;
    }
    let mut max_g: f64 = 0.0;
    for i in s..p {
        let j = (i + 4).min(p);
        let dd = grid[j].0 - grid[i].0;
        if dd >= 50.0 {
            max_g = max_g.max((grid[j].1 - grid[i].1) / dd * 100.0);
        }
    }
    let score = length * avg;
    Some(Climb {
        start_d: grid[s].0,
        end_d: grid[p].0,
        length_m: length,
        gain_m: gain,
        avg_grade_pct: avg,
        max_grade_pct: max_g.max(avg),
        start_ele: grid[s].1,
        top_ele: grid[p].1,
        score,
        category: climb_category(score).into(),
    })
}

pub fn climb_category(score: f64) -> &'static str {
    match score {
        s if s >= 80_000.0 => "HC",
        s if s >= 64_000.0 => "Cat 1",
        s if s >= 32_000.0 => "Cat 2",
        s if s >= 16_000.0 => "Cat 3",
        s if s >= 8_000.0 => "Cat 4",
        _ => "Uncategorised",
    }
}

pub fn difficulty_score(gain: f64, density: f64, max_grade: f64) -> f64 {
    let s = 1.6 * (1.0 + gain.max(0.0) / 200.0).log2() + density / 10.0 + (max_grade - 6.0).max(0.0) / 3.0;
    (s.clamp(0.0, 10.0) * 10.0).round() / 10.0
}

pub fn difficulty_label(score: f64) -> &'static str {
    match score {
        s if s < 2.5 => "Easy",
        s if s < 4.5 => "Moderate",
        s if s < 6.5 => "Hard",
        s if s < 8.5 => "Very hard",
        _ => "Extreme",
    }
}

fn moving_average(x: &[f64], window: usize) -> Vec<f64> {
    if window <= 1 || x.len() < 2 {
        return x.to_vec();
    }
    let half = window / 2;
    let n = x.len();
    let mut prefix = vec![0.0; n + 1];
    for i in 0..n {
        prefix[i + 1] = prefix[i] + x[i];
    }
    (0..n)
        .map(|i| {
            let a = i.saturating_sub(half);
            let b = (i + half + 1).min(n);
            (prefix[b] - prefix[a]) / (b - a) as f64
        })
        .collect()
}

fn points_from_grades(grades: &[f64], step: f64, start_ele: f64) -> Vec<RoutePoint> {
    let mut pts = Vec::with_capacity(grades.len() + 1);
    let mut ele = start_ele;
    pts.push(RoutePoint { d: 0.0, ele });
    for (i, g) in grades.iter().enumerate() {
        ele += g * step;
        pts.push(RoutePoint { d: (i + 1) as f64 * step, ele });
    }
    let min = pts.iter().map(|p| p.ele).fold(f64::INFINITY, f64::min);
    if min < 0.0 {
        for p in pts.iter_mut() {
            p.ele -= min;
        }
    }
    for p in pts.iter_mut() {
        p.ele = (p.ele * 100.0).round() / 100.0;
    }
    pts
}

struct TerrainSpec {
    seg_len: (f64, f64),
    grade: (f64, f64),
    jitter: f64,
    flat_p: f64,
}

fn terrain_spec(t: Terrain) -> TerrainSpec {
    match t {
        Terrain::Flat => TerrainSpec { seg_len: (800.0, 4000.0), grade: (0.5, 2.0), jitter: 0.4, flat_p: 0.5 },
        Terrain::Rolling => TerrainSpec { seg_len: (250.0, 1200.0), grade: (1.5, 5.0), jitter: 0.8, flat_p: 0.2 },
        Terrain::Hilly => TerrainSpec { seg_len: (500.0, 2500.0), grade: (3.0, 8.0), jitter: 1.2, flat_p: 0.15 },
        Terrain::Mountainous => {
            TerrainSpec { seg_len: (1500.0, 7000.0), grade: (4.5, 9.0), jitter: 1.5, flat_p: 0.1 }
        }
        Terrain::Alpine => TerrainSpec { seg_len: (4000.0, 14000.0), grade: (6.0, 10.0), jitter: 1.5, flat_p: 0.05 },
    }
}

/// Suggested climbing for a terrain and distance (m) – used as UI defaults.
pub fn suggested_gain(t: Terrain, distance_km: f64) -> f64 {
    let per_km = match t {
        Terrain::Flat => 3.0,
        Terrain::Rolling => 10.0,
        Terrain::Hilly => 18.0,
        Terrain::Mountainous => 28.0,
        Terrain::Alpine => 40.0,
    };
    (per_km * distance_km / 10.0).round() * 10.0
}

/// Scale positive (or negative) grades so their integral hits `target` metres,
/// respecting `limit` (absolute, %/100). Returns achieved metres.
fn scale_component(g: &mut [f64], step: f64, target: f64, limit: f64, positive: bool) -> f64 {
    let sel = |x: f64| if positive { x > 0.0 } else { x < 0.0 };
    let total = |g: &[f64]| g.iter().filter(|&&x| sel(x)).map(|x| x.abs()).sum::<f64>() * step;
    if target <= 0.0 {
        for x in g.iter_mut() {
            if sel(*x) {
                *x = 0.0;
            }
        }
        return 0.0;
    }
    for _ in 0..12 {
        let have = total(g);
        if have < 1e-6 {
            break;
        }
        let f = target / have;
        if (f - 1.0).abs() < 0.005 {
            break;
        }
        for x in g.iter_mut() {
            if sel(*x) {
                *x = (*x * f).clamp(-limit, limit);
            }
        }
    }
    total(g)
}

/// Generate a route procedurally. Deterministic for a given `GenParams`.
pub fn generate(params: &GenParams, name: &str) -> Route {
    let step = 20.0;
    let total = (params.distance_km * 1000.0).clamp(1000.0, 400_000.0);
    let n = (total / step).round().max(10.0) as usize;
    let spec = terrain_spec(params.terrain);
    let mut rng = Rng::new(params.seed);
    let max_g = (params.max_grade_pct.clamp(1.0, 25.0)) / 100.0;

    let mut raw = vec![0.0_f64; n];
    // Reserve a final climb for summit finishes.
    let summit_len = if params.finish == Finish::Summit {
        ((total * 0.25).clamp(1500.0, spec.seg_len.1.max(4000.0)) / step) as usize
    } else {
        0
    };
    let body = n.saturating_sub(summit_len);

    // If lots of climbing is requested, make climbs longer than descents so the
    // target can be met without exceeding the maximum gradient.
    let typical_up = (spec.grade.1 / 100.0).min(max_g) * 0.85;
    let needed_share = params.target_gain_m.max(0.0) / (total * typical_up).max(1.0);
    let bias = (needed_share / 0.45).clamp(0.6, 2.5);
    let flat_p = if needed_share > 0.5 { spec.flat_p * 0.3 } else { spec.flat_p };

    let mut i = 0usize;
    let mut up = rng.chance(0.6);
    while i < body {
        let flat = rng.chance(flat_p);
        let mult = if flat { 1.0 } else if up { bias } else { 1.0 / bias };
        let len_m = (rng.range(spec.seg_len.0, spec.seg_len.1) * mult).min(total * 0.35).max(step * 5.0);
        let len = ((len_m / step) as usize).max(1);
        let base = if flat {
            rng.normal() * 0.3
        } else {
            let mag = rng.range(spec.grade.0, spec.grade.1);
            let s = if up { mag } else { -mag * rng.range(0.9, 1.25) };
            up = !up;
            s
        };
        // vary every ~200 m inside the segment
        let mut chunk_g = base;
        for k in 0..len {
            if i >= body {
                break;
            }
            if k % 10 == 0 {
                chunk_g = base + rng.normal() * spec.jitter * if flat { 0.5 } else { 1.0 };
            }
            raw[i] = chunk_g / 100.0;
            i += 1;
        }
    }
    for (k, x) in raw.iter_mut().enumerate().skip(body) {
        let prog = (k - body) as f64 / summit_len.max(1) as f64;
        let base = spec.grade.1 * (0.85 + 0.3 * prog);
        *x = (base + rng.normal() * spec.jitter * 0.6) / 100.0;
        if *x < 0.01 {
            *x = 0.01;
        }
    }

    let w = 3 + (params.smoothness.clamp(0.0, 1.0) * 12.0) as usize;
    let mut g = moving_average(&raw, w);

    let target_gain = params.target_gain_m.max(0.0);
    let gain = scale_component(&mut g, step, target_gain, max_g, true);
    let target_loss = match params.finish {
        Finish::Loop => gain,
        Finish::Free => gain * 0.85,
        Finish::Summit => gain * 0.45,
    };
    scale_component(&mut g, step, target_loss, max_g + 0.03, false);

    g = moving_average(&g, 3);
    for x in g.iter_mut() {
        *x = x.clamp(-(max_g + 0.03), max_g);
    }
    // Smoothing blends ups into downs; re-hit the targets precisely.
    let gain = scale_component(&mut g, step, target_gain, max_g, true);
    let target_loss = match params.finish {
        Finish::Loop => gain,
        Finish::Free => gain * 0.85,
        Finish::Summit => gain * 0.45,
    };
    scale_component(&mut g, step, target_loss, max_g + 0.03, false);
    if params.finish == Finish::Loop {
        // Close the loop exactly: spread the residual over the whole route.
        let net: f64 = g.iter().sum::<f64>() * step;
        let per = net / (n as f64 * step);
        for x in g.iter_mut() {
            *x -= per;
        }
    }

    let points = points_from_grades(&g, step, params.start_ele_m);
    let mut r = Route {
        id: crate::new_id(),
        name: name.to_string(),
        description: String::new(),
        source: RouteSource::Generated,
        created_at: crate::now_rfc3339(),
        points,
        is_loop: params.finish == Finish::Loop,
        tags: vec![format!("{:?}", params.terrain).to_lowercase()],
        gen_params: Some(params.clone()),
        segments: None,
        favourite: false,
    };
    if params.finish == Finish::Summit {
        r.tags.push("summit finish".into());
    }
    if r.is_loop {
        r.tags.push("loop".into());
    }
    r
}

/// Build a route from user-defined segments (length + gradient).
pub fn build_from_segments(name: &str, segs: &[SegmentSpec], start_ele: f64, smooth: bool) -> Route {
    let step = 10.0;
    let mut g = Vec::new();
    for s in segs {
        let n = (s.length_m.max(0.0) / step).round() as usize;
        g.extend(std::iter::repeat(s.grade_pct.clamp(-25.0, 25.0) / 100.0).take(n));
    }
    if g.is_empty() {
        g.push(0.0);
    }
    if smooth {
        g = moving_average(&g, 7);
    }
    let points = points_from_grades(&g, step, start_ele);
    let first = points.first().map(|p| p.ele).unwrap_or(0.0);
    let last = points.last().map(|p| p.ele).unwrap_or(0.0);
    Route {
        id: crate::new_id(),
        name: name.to_string(),
        description: String::new(),
        source: RouteSource::Custom,
        created_at: crate::now_rfc3339(),
        is_loop: (first - last).abs() < 1.0,
        points,
        tags: vec!["custom".into()],
        gen_params: None,
        segments: Some(segs.to_vec()),
        favourite: false,
    }
}

fn haversine(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6_371_000.0;
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

pub struct GpxTrack {
    pub name: Option<String>,
    pub points: Vec<(f64, f64, Option<f64>)>,
}

pub fn parse_gpx(text: &str) -> Result<GpxTrack, String> {
    let doc = roxmltree::Document::parse(text).map_err(|e| format!("Invalid GPX/XML: {e}"))?;
    let mut name = None;
    let mut points = Vec::new();
    for node in doc.descendants() {
        let tag = node.tag_name().name();
        if name.is_none() && tag == "name" {
            if let Some(t) = node.text() {
                let t = t.trim();
                if !t.is_empty() {
                    name = Some(t.to_string());
                }
            }
        }
        if tag == "trkpt" || tag == "rtept" {
            let lat = node.attribute("lat").and_then(|v| v.parse::<f64>().ok());
            let lon = node.attribute("lon").and_then(|v| v.parse::<f64>().ok());
            if let (Some(lat), Some(lon)) = (lat, lon) {
                let ele = node
                    .children()
                    .find(|c| c.tag_name().name() == "ele")
                    .and_then(|c| c.text())
                    .and_then(|t| t.trim().parse::<f64>().ok());
                points.push((lat, lon, ele));
            }
        }
    }
    if points.len() < 2 {
        return Err("GPX file contains fewer than two track/route points".into());
    }
    Ok(GpxTrack { name, points })
}

pub fn route_from_gpx(text: &str, name_override: Option<&str>) -> Result<Route, String> {
    let trk = parse_gpx(text)?;
    let has_ele = trk.points.iter().any(|p| p.2.is_some());
    let mut dist = vec![0.0];
    let mut ele = vec![trk.points[0].2.unwrap_or(0.0)];
    for w in trk.points.windows(2) {
        let dd = haversine(w[0].0, w[0].1, w[1].0, w[1].1);
        if dd < 0.5 {
            continue;
        }
        dist.push(dist.last().unwrap() + dd);
        ele.push(w[1].2.unwrap_or(*ele.last().unwrap()));
    }
    let total = *dist.last().unwrap();
    if total < 200.0 {
        return Err("GPX track is shorter than 200 m".into());
    }
    // Resample to 10 m and smooth GPS/DEM noise over ~70 m.
    let step = 10.0;
    let n = (total / step).floor() as usize;
    let mut res = Vec::with_capacity(n + 2);
    let mut j = 0usize;
    for i in 0..=n {
        let d = i as f64 * step;
        while j + 1 < dist.len() && dist[j + 1] < d {
            j += 1;
        }
        let e = if j + 1 < dist.len() {
            let f = (d - dist[j]) / (dist[j + 1] - dist[j]).max(1e-9);
            ele[j] + f.clamp(0.0, 1.0) * (ele[j + 1] - ele[j])
        } else {
            ele[j]
        };
        res.push(e);
    }
    let sm = moving_average(&res, 7);
    let mut points: Vec<RoutePoint> =
        sm.iter().enumerate().map(|(i, &e)| RoutePoint { d: i as f64 * step, ele: (e * 100.0).round() / 100.0 }).collect();
    if total - points.last().unwrap().d > 1.0 {
        points.push(RoutePoint { d: total, ele: *ele.last().unwrap() });
    }
    let (a, b) = (trk.points.first().unwrap(), trk.points.last().unwrap());
    let is_loop = haversine(a.0, a.1, b.0, b.1) < 250.0;
    let mut tags = vec!["gpx".to_string()];
    if !has_ele {
        tags.push("no elevation".into());
    }
    if is_loop {
        tags.push("loop".into());
    }
    Ok(Route {
        id: crate::new_id(),
        name: name_override.map(|s| s.to_string()).or(trk.name).unwrap_or_else(|| "Imported route".into()),
        description: if has_ele { String::new() } else { "GPX had no elevation data – ridden flat.".into() },
        source: RouteSource::Gpx,
        created_at: crate::now_rfc3339(),
        points,
        is_loop,
        tags,
        gen_params: None,
        segments: None,
        favourite: false,
    })
}

// ===========================================================================
// In-ride sectors: splice new road into a route
// ===========================================================================

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SectorKind {
    Climb,
    Descent,
    Flat,
    Rollers,
    Sprint,
    Custom,
    Terrain,
}

fn d_in() -> f64 {
    300.0
}
fn yes() -> bool {
    true
}

/// A piece of road to add ahead of the rider (or at the end of the route).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SectorSpec {
    pub kind: SectorKind,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub length_m: f64,
    #[serde(default)]
    pub grade_pct: f64,
    /// Custom sectors: explicit segments.
    #[serde(default)]
    pub segments: Vec<SegmentSpec>,
    /// Terrain sectors: generate a chunk of this terrain type.
    #[serde(default)]
    pub terrain: Option<Terrain>,
    /// How far ahead of the rider the sector starts (m).
    #[serde(default = "d_in")]
    pub in_m: f64,
    /// Append at the end of the route instead.
    #[serde(default)]
    pub at_end: bool,
    /// Add a return section afterwards so the rest of the route keeps its elevation.
    #[serde(default = "yes")]
    pub rejoin: bool,
    /// Time the sector and record a split.
    #[serde(default = "yes")]
    pub timed: bool,
}

impl SectorSpec {
    pub fn display_name(&self) -> String {
        if let Some(n) = self.name.as_ref().filter(|n| !n.trim().is_empty()) {
            return n.trim().to_string();
        }
        let km = |m: f64| if m >= 1000.0 { format!("{:.1} km", m / 1000.0) } else { format!("{:.0} m", m) };
        match self.kind {
            SectorKind::Climb => format!("Climb {} @ {:.1}%", km(self.length_m), self.grade_pct.abs()),
            SectorKind::Descent => format!("Descent {} @ {:.1}%", km(self.length_m), self.grade_pct.abs()),
            SectorKind::Flat => format!("Flat {}", km(self.length_m)),
            SectorKind::Rollers => format!("Rollers {}", km(self.length_m)),
            SectorKind::Sprint => format!("Sprint {}", km(self.length_m)),
            SectorKind::Custom => "Custom sector".into(),
            SectorKind::Terrain => format!("{} {}", self.terrain.map(|t| format!("{t:?}")).unwrap_or_else(|| "Hilly".into()), km(self.length_m)),
        }
    }

    /// The sector's own segments (without any rejoin section).
    pub fn segments(&self, seed: u64) -> Vec<SegmentSpec> {
        let len = self.length_m.clamp(50.0, 50_000.0);
        let g = self.grade_pct.clamp(-20.0, 20.0);
        match self.kind {
            SectorKind::Climb => vec![SegmentSpec { length_m: len, grade_pct: g.abs().max(0.5) }],
            SectorKind::Descent => vec![SegmentSpec { length_m: len, grade_pct: -g.abs().max(0.5) }],
            SectorKind::Flat => vec![SegmentSpec { length_m: len, grade_pct: g.clamp(-1.0, 1.0) }],
            SectorKind::Sprint => vec![SegmentSpec { length_m: len.min(1000.0), grade_pct: 0.0 }],
            SectorKind::Rollers => {
                let n = ((len / 400.0).round() as usize).max(2) & !1; // even → ends level
                let n = n.max(2);
                let amp = if g.abs() < 0.5 { 4.0 } else { g.abs() };
                (0..n).map(|i| SegmentSpec { length_m: len / n as f64, grade_pct: if i % 2 == 0 { amp } else { -amp } }).collect()
            }
            SectorKind::Custom => self.segments.iter().filter(|s| s.length_m > 0.0).cloned().collect(),
            SectorKind::Terrain => {
                let t = self.terrain.unwrap_or(Terrain::Hilly);
                let r = generate(
                    &GenParams {
                        distance_km: (len / 1000.0).max(1.0),
                        target_gain_m: suggested_gain(t, len / 1000.0).max(10.0),
                        max_grade_pct: match t {
                            Terrain::Flat => 2.5,
                            Terrain::Rolling => 7.0,
                            Terrain::Hilly => 10.0,
                            _ => 12.0,
                        },
                        terrain: t,
                        finish: Finish::Loop,
                        seed,
                        start_ele_m: 0.0,
                        smoothness: 0.5,
                    },
                    "chunk",
                );
                r.points
                    .windows(2)
                    .map(|w| SegmentSpec { length_m: w[1].d - w[0].d, grade_pct: (w[1].ele - w[0].ele) / (w[1].d - w[0].d).max(1e-6) * 100.0 })
                    .collect()
            }
        }
    }
}

/// Total length and net elevation change of a set of segments.
pub fn segments_summary(segs: &[SegmentSpec]) -> (f64, f64) {
    segs.iter().fold((0.0, 0.0), |(l, e), s| (l + s.length_m, e + s.length_m * s.grade_pct / 100.0))
}

/// A section that returns to the original elevation after a sector (≤ 5 %).
pub fn rejoin_segment(net_m: f64) -> Option<SegmentSpec> {
    if net_m.abs() < 2.0 {
        return None;
    }
    let len = (net_m.abs() / 0.05).max(200.0);
    Some(SegmentSpec { length_m: len, grade_pct: -net_m / len * 100.0 })
}

/// Splice `segs` into the route at distance `at` (clamped to the route).
/// Everything after `at` moves along by the inserted length and up/down by
/// the inserted net elevation so the profile stays continuous.
/// Returns (inserted length, net elevation change).
pub fn insert_segments(route: &mut Route, at: f64, segs: &[SegmentSpec]) -> (f64, f64) {
    let step = 10.0;
    let total = route.distance();
    let at = at.clamp(0.0, total);
    let e0 = route.elevation_at(at);
    let mut grades = Vec::new();
    for s in segs {
        let n = ((s.length_m / step).round() as usize).max(1);
        grades.extend(std::iter::repeat(s.grade_pct.clamp(-25.0, 25.0) / 100.0).take(n));
    }
    if grades.is_empty() {
        return (0.0, 0.0);
    }
    let grades = moving_average(&grades, 5);
    let mut pts: Vec<RoutePoint> = route.points.iter().filter(|p| p.d < at).cloned().collect();
    pts.push(RoutePoint { d: at, ele: e0 });
    let (mut d, mut e) = (at, e0);
    for g in grades {
        d += step;
        e += g * step;
        pts.push(RoutePoint { d, ele: (e * 100.0).round() / 100.0 });
    }
    let ins = d - at;
    let de = e - e0;
    for p in route.points.iter().filter(|p| p.d > at) {
        pts.push(RoutePoint { d: p.d + ins, ele: p.ele + de });
    }
    let min = pts.iter().map(|p| p.ele).fold(f64::INFINITY, f64::min);
    if min < 0.0 {
        for p in pts.iter_mut() {
            p.ele -= min;
        }
    }
    route.points = pts;
    (ins, de)
}

/// A long, closed loop of generated terrain for endless free rides.
pub fn endless_route(terrain: Terrain, seed: u64) -> Route {
    let km = 150.0;
    let mut r = generate(
        &GenParams {
            distance_km: km,
            target_gain_m: suggested_gain(terrain, km),
            max_grade_pct: match terrain {
                Terrain::Flat => 2.0,
                Terrain::Rolling => 6.0,
                Terrain::Hilly => 9.0,
                _ => 11.0,
            },
            terrain,
            finish: Finish::Loop,
            seed,
            start_ele_m: 200.0,
            smoothness: 0.6,
        },
        &format!("Endless {}", format!("{terrain:?}").to_lowercase()),
    );
    r.description = "Procedurally generated road that never ends.".into();
    r
}

/// A starter library so a fresh install has something to ride.
pub fn seed_routes() -> Vec<Route> {
    let mk = |name: &str, desc: &str, km: f64, gain: f64, maxg: f64, t: Terrain, f: Finish, seed: u64, start: f64| {
        let mut r = generate(
            &GenParams {
                distance_km: km,
                target_gain_m: gain,
                max_grade_pct: maxg,
                terrain: t,
                finish: f,
                seed,
                start_ele_m: start,
                smoothness: 0.55,
            },
            name,
        );
        r.description = desc.into();
        r.source = RouteSource::Builtin;
        r
    };
    vec![
        mk("Coastal Flats", "Fast, flat seafront loop. Perfect for recovery or tempo.", 24.0, 60.0, 2.5, Terrain::Flat, Finish::Loop, 11, 8.0),
        mk("Rolling Downs", "Punchy rollers that reward momentum.", 32.0, 380.0, 7.0, Terrain::Rolling, Finish::Loop, 23, 90.0),
        mk("Valley Classic", "A hilly one-day-classic profile with a few sharp kickers.", 58.0, 900.0, 11.0, Terrain::Hilly, Finish::Free, 37, 140.0),
        mk("Col de Kickr", "A long Alpine-style summit finish. Pace it.", 21.0, 1050.0, 10.5, Terrain::Alpine, Finish::Summit, 51, 620.0),
        mk("Gran Fondo Sierra", "Big day out: three major climbs and fast descents.", 96.0, 2300.0, 12.0, Terrain::Mountainous, Finish::Loop, 77, 400.0),
        mk("Lunchtime Hill Reps", "Short loop with one solid climb – ideal for laps.", 8.0, 140.0, 8.0, Terrain::Hilly, Finish::Loop, 5, 60.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(t: Terrain, f: Finish, km: f64, gain: f64, maxg: f64) -> GenParams {
        GenParams { distance_km: km, target_gain_m: gain, max_grade_pct: maxg, terrain: t, finish: f, seed: 7, start_ele_m: 100.0, smoothness: 0.5 }
    }

    #[test]
    fn generator_hits_distance_and_gain() {
        for (t, km, gain) in [
            (Terrain::Rolling, 30.0, 300.0),
            (Terrain::Hilly, 50.0, 800.0),
            (Terrain::Mountainous, 80.0, 1800.0),
            (Terrain::Alpine, 20.0, 1000.0),
        ] {
            let r = generate(&params(t, Finish::Free, km, gain, 14.0), "t");
            let s = r.stats();
            assert!((s.distance_m - km * 1000.0).abs() < 25.0, "{t:?} dist {}", s.distance_m);
            let err = (s.gain_m - gain).abs() / gain;
            assert!(err < 0.12, "{t:?} gain {} vs {gain}", s.gain_m);
            assert!(s.max_grade_pct <= 14.5, "{t:?} max grade {}", s.max_grade_pct);
        }
    }

    #[test]
    fn generator_is_deterministic() {
        let p = params(Terrain::Hilly, Finish::Loop, 20.0, 300.0, 10.0);
        let a = generate(&p, "a");
        let b = generate(&p, "b");
        assert_eq!(a.points, b.points);
    }

    #[test]
    fn loop_closes() {
        let r = generate(&params(Terrain::Hilly, Finish::Loop, 40.0, 600.0, 10.0), "loop");
        let first = r.points.first().unwrap().ele;
        let last = r.points.last().unwrap().ele;
        assert!((first - last).abs() < 2.0, "{first} {last}");
    }

    #[test]
    fn summit_finish_ends_high() {
        let r = generate(&params(Terrain::Alpine, Finish::Summit, 25.0, 1200.0, 11.0), "summit");
        let s = r.stats();
        let last = r.points.last().unwrap().ele;
        assert!(last > s.max_ele - 15.0, "last {last} max {}", s.max_ele);
        assert!(!s.climbs.is_empty());
    }

    #[test]
    fn respects_max_grade() {
        let r = generate(&params(Terrain::Alpine, Finish::Free, 30.0, 3000.0, 6.0), "cap");
        assert!(r.stats().max_grade_pct <= 6.3);
    }

    #[test]
    fn flat_route_with_zero_gain() {
        let r = generate(&params(Terrain::Flat, Finish::Loop, 10.0, 0.0, 5.0), "flat");
        assert!(r.stats().gain_m < 2.0);
    }

    #[test]
    fn interpolation_and_grade() {
        let r = build_from_segments("s", &[SegmentSpec { length_m: 1000.0, grade_pct: 0.0 }, SegmentSpec { length_m: 1000.0, grade_pct: 5.0 }], 100.0, false);
        assert!((r.distance() - 2000.0).abs() < 1e-6);
        assert!((r.elevation_at(2000.0) - 150.0).abs() < 0.1);
        assert!((r.grade_at(1500.0, 20.0) - 0.05).abs() < 1e-6);
        assert!(r.grade_at(500.0, 20.0).abs() < 1e-9);
        let s = r.stats();
        assert_eq!(s.climbs.len(), 1);
        assert!((s.climbs[0].avg_grade_pct - 5.0).abs() < 0.3);
        assert_eq!(s.climbs[0].category, "Uncategorised"); // 1000 m × 5 % = 5000
    }

    #[test]
    fn climb_categories() {
        assert_eq!(climb_category(85_000.0), "HC");
        assert_eq!(climb_category(20_000.0), "Cat 3");
    }

    #[test]
    fn gpx_import() {
        let mut gpx = String::from(r#"<?xml version="1.0"?><gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1"><trk><name>Test Hill</name><trkseg>"#);
        for i in 0..200 {
            let lat = 51.0 + i as f64 * 0.0001; // ~11 m per step
            let ele = 50.0 + i as f64 * 0.55;
            gpx.push_str(&format!(r#"<trkpt lat="{lat}" lon="-0.1"><ele>{ele}</ele></trkpt>"#));
        }
        gpx.push_str("</trkseg></trk></gpx>");
        let r = route_from_gpx(&gpx, None).unwrap();
        assert_eq!(r.name, "Test Hill");
        let s = r.stats();
        assert!((s.distance_m - 2213.0).abs() < 30.0, "{}", s.distance_m);
        assert!((s.gain_m - 109.0).abs() < 6.0, "{}", s.gain_m);
        assert!(parse_gpx("<gpx></gpx>").is_err());
    }

    #[test]
    fn estimate_time_reasonable() {
        let r = build_from_segments("flat", &[SegmentSpec { length_m: 36_000.0, grade_pct: 0.0 }], 0.0, false);
        let t = r.estimate_time(250.0, &PhysicsParams { mass_kg: 80.0, ..Default::default() });
        assert!((3300.0..3800.0).contains(&t), "{t}");
    }

    #[test]
    fn sector_insertion_keeps_profile_continuous() {
        let base = build_from_segments("flat", &[SegmentSpec { length_m: 2000.0, grade_pct: 0.0 }], 100.0, false);
        // Without rejoin: everything after is lifted by the climb's gain.
        let mut r = base.clone();
        let spec = SectorSpec { kind: SectorKind::Climb, name: None, length_m: 1000.0, grade_pct: 6.0, segments: vec![], terrain: None, in_m: 300.0, at_end: false, rejoin: false, timed: true };
        let (len, de) = insert_segments(&mut r, 500.0, &spec.segments(1));
        assert!((len - 1000.0).abs() < 1e-6);
        assert!((de - 60.0).abs() < 1.0, "{de}");
        assert!((r.distance() - 3000.0).abs() < 1e-6);
        assert!((r.elevation_at(400.0) - 100.0).abs() < 0.5);
        assert!((r.elevation_at(2900.0) - 160.0).abs() < 1.0);
        assert!((r.grade_at(1000.0, 50.0) - 0.06).abs() < 0.003);
        // With rejoin the end of the route is back at its original elevation.
        let mut r2 = base.clone();
        let mut segs = spec.segments(1);
        let (_, net) = segments_summary(&segs);
        segs.extend(rejoin_segment(net));
        insert_segments(&mut r2, 500.0, &segs);
        assert!((r2.elevation_at(r2.distance()) - 100.0).abs() < 1.0);
        assert!((r2.distance() - (3000.0 + 1200.0)).abs() < 1.0, "{}", r2.distance());
        // No jumps anywhere.
        for w in r2.points.windows(2) {
            assert!(((w[1].ele - w[0].ele) / (w[1].d - w[0].d)).abs() < 0.08);
        }
    }

    #[test]
    fn sector_shapes() {
        let mk = |kind, len, g| SectorSpec { kind, name: None, length_m: len, grade_pct: g, segments: vec![], terrain: Some(Terrain::Hilly), in_m: 0.0, at_end: false, rejoin: true, timed: true };
        let (l, e) = segments_summary(&mk(SectorKind::Rollers, 2000.0, 5.0).segments(1));
        assert!((l - 2000.0).abs() < 1e-6 && e.abs() < 1e-6);
        let (l, _) = segments_summary(&mk(SectorKind::Terrain, 4000.0, 0.0).segments(3));
        assert!((l - 4000.0).abs() < 30.0, "{l}");
        assert_eq!(mk(SectorKind::Descent, 1000.0, 6.0).segments(1)[0].grade_pct, -6.0);
        assert_eq!(mk(SectorKind::Climb, 800.0, 9.0).display_name(), "Climb 800 m @ 9.0%");
        let e = endless_route(Terrain::Rolling, 9);
        assert!((e.elevation_at(0.0) - e.elevation_at(e.distance())).abs() < 2.0);
    }

    #[test]
    fn seed_routes_are_valid() {
        let rs = seed_routes();
        assert_eq!(rs.len(), 6);
        for r in rs {
            let s = r.stats();
            assert!(s.distance_m > 5000.0);
            assert!(!s.difficulty_label.is_empty());
        }
    }
}
