# CoreKick

A fast, offline indoor-cycling app for the **Wahoo KICKR CORE** (and any Bluetooth **FTMS** smart trainer), built with **Tauri 2**, Rust and React.

- **Routes**: a seeded generator (terrain, distance, climbing, max gradient, loop / point-to-point / summit finish), a segment-by-segment builder and **GPX import**. Every route gets automatic **climb detection and categories** (Cat 4 → HC), a difficulty score and an estimated time.
- **Riding**: SIM mode sends the real gradient to the trainer (with look-ahead smoothing, a trainer-difficulty setting and clamps). Your virtual speed comes from a proper physics model: gravity, rolling resistance, aero drag with altitude-adjusted air density, and momentum. You can ride laps, and you can **race a ghost** of your best attempt.
- **Workouts**: 31 built-in sessions, from recovery through Norwegian 4×4s and Rønnestad 30/15s to race simulations. You can filter them by length and sort by duration or difficulty. The editor supports nested **repeat sets**, cadence targets and a **text mode**. Workouts **import** from Zwift `.zwo`, `.erg` and `.mrc` files and **export** to `.zwo`.
- **Free ride**: four modes – gradient (SIM), power (ERG), resistance level and heart-rate-controlled. Scenery can be a flat road, endless generated terrain (rolling, hilly, mountainous or alpine) or a loop of any saved route. You can set a goal by time, distance, kJ, kcal or climbing.
- **Edit the ride while riding** (press `E`):
  - *Road ahead*: splice climbs, walls, rollers, descents, sprints or a chunk of generated terrain into the road ahead. Sectors can be timed and recorded as splits. "Return to route elevation" keeps the rest of the course intact, and you can undo a sector until you reach it.
  - *Workout*: lengthen or shorten the current step, nudge its target, add one more rep, remove upcoming steps, insert sets (from presets or text), skip to the cool-down, or end the plan and ride free. Inserting intervals into a free ride or a route ride works too: ERG takes over for the set, then hands back.
  - *Ride*: switch free-ride mode, set or clear a goal, change trainer difficulty, and add or remove laps on route rides.
  - **Laps** (`L`): each press records a split, and route laps and timed sectors are recorded automatically. All splits appear on the ride's page.
- **Tests**: Ramp (auto-detects failure), 20-minute and 2 × 8-minute FTP tests, with one-click "apply new FTP".
- **Automatic FTP (eFTP)**: after every saved ride, and once a day on launch, CoreKick re-assesses your FTP from your best 3, 5, 8, 10 and 20-minute efforts over the last 90 days.
  - It fits a **critical power** model (work = CP·t + W′) to those efforts. If that fit isn't reliable, it uses 95% of your best 20-minute power instead.
  - Only valid rides count: at least 20 minutes moving, with real power data, and not ridden on the simulator.
  - In *Rider → Auto FTP* you choose what happens. *Suggest* (the default) shows a banner with a one-tap "Set FTP". *Auto-raise* applies increases automatically. *Off* does nothing.
  - Drops are never applied automatically. They are only suggested when the fit is confident and you've ridden hard recently, so easy weeks won't lower your FTP.
  - The Tests page shows the estimate and the exact efforts it used.
- **Events**: recurring rides (once, daily, weekly on chosen days, every N weeks or every N days). Each event keeps an attempt history, a best time and a streak.
- **Accurate energy**: work (kJ) is integrated from measured power over real elapsed time. Calories are calculated as `kJ ÷ (4.184 × gross efficiency)`, and efficiency is adjustable (default 24%, so 1 kJ ≈ 1 kcal). A heart-rate-based estimate (Keytel formula) is shown alongside when a strap is connected.
- **Stats**: NP, IF, TSS, VI, power-duration bests, time in zones, fitness / fatigue / form (CTL / ATL / TSB), a power curve (all-time vs 90 days), weekly volume, records, streaks and FTP history.
- **Export**: FIT (validated against Garmin's official FIT SDK), TCX and CSV.
- **Strava**: one-click or automatic upload as a **Virtual Ride** – FIT with power, heart rate, cadence, laps, NP/TSS – and a "View on Strava" link.
- **Heart rate**: any Bluetooth heart-rate strap or a watch broadcasting HR (e.g. COROS PACE Pro). The ride screen shows **live zones**, using % of max HR, heart-rate reserve or 7 threshold-based zones. Each ride's summary includes time in each zone.
- **Heart-rate alerts**: choose a zone range and get tones plus spoken prompts when you go too high or too low, with a warm-up grace period. Toggle them mid-ride with `H` or adjust them in the Edit panel.
- **Virtual shifting for the Zwift Cog**: 24 virtual gears for single-speed setups. CoreKick turns cadence × gear × wheel size into road speed, then holds the KICKR in ERG at exactly the power that speed needs on the current (difficulty-scaled) gradient. Shift with the on-screen buttons, the `[` / `]` keys, or any Bluetooth page-turner remote (Page Up / Page Down).
- **Simulator**: a virtual trainer and rider, so you can try everything without the bike.

## Run it (macOS)

Prerequisites: [Rust](https://rustup.rs), Node 18+, Xcode Command Line Tools (`xcode-select --install`).

```bash
cd CoreKick
npm install
npm run tauri dev          # development, hot reload
npm run tauri build        # release app → target/release/bundle/macos/CoreKick.app
```

**Bluetooth permission.** The bundled app ships `NSBluetoothAlwaysUsageDescription`, and macOS asks for permission the first time you scan. In `tauri dev` the request is made on behalf of your terminal, so allow Terminal / iTerm / VS Code under *System Settings → Privacy & Security → Bluetooth*. If scanning does nothing in dev, build the app and run the `.app` instead.

**Simulator.** Turn it on from *Devices*, or start with `COREKICK_SIMULATOR=1 npm run tauri dev`.

Windows and Linux builds work too: btleplug supports WinRT and BlueZ.

## Connecting the KICKR CORE

1. Update the trainer firmware in the Wahoo app. FTMS control arrived in the 2021 firmware.
2. Close any other app that might take control of the trainer (Zwift, the Wahoo app, etc.).
3. In *Devices*, choose **Scan for devices**, then **Use as trainer**. Pedal briefly first to wake the trainer.

If the trainer drops out, CoreKick reconnects and re-sends the current gradient or ERG target.

## Strava setup

Strava issues API access per *application*, so CoreKick uses your own (free) API app:

1. Go to <https://www.strava.com/settings/api> and create an app. Any name works; use `http://localhost` as the website and `localhost` as the **Authorization Callback Domain**.
2. In CoreKick → Profile → Strava, paste the **Client ID** and **Client Secret**, then press **Connect with Strava**. Your browser opens Strava's permission page; keep "Upload your activities" ticked.
3. Optionally switch on **Upload every saved ride automatically**.

Notes:
- A new Strava app covers one athlete (you), which is all you need.
- Since June 2026 Strava requires the developer of a standard-tier API app to hold a Strava subscription.
- Tokens are stored in the local database and refreshed automatically. Disconnect revokes them.
- Strava is moving its API to `https://api-v3.strava.com`. The new host is available from 4 Jan 2027 and required from 1 Jun 2027. CoreKick reads an optional `strava_api_base` setting so the switch is a one-line change.

## COROS PACE Pro as a heart-rate monitor

On the watch, long-press BACK/LAP → Toolbox → **Broadcast HR**. In CoreKick → Devices, press **Scan** and **Connect** it as the heart-rate device. While broadcasting, the watch allows only one Bluetooth connection, so disconnect it from your phone first.

## Workout text format

```
10m 50-75%                   ramp from 50 % to 75 % FTP
20m @90% 85rpm               steady with a cadence target
5x 1m @120% / 1m @50%        intervals
3x (30s @150%, 30s @50%)     a repeat set (nestable)
2m free · 30s effort @2%     rider-controlled (efforts count in tests)
1:30 @250w · 1h @65%         mm:ss, hours, absolute watts
```

Put each item on its own line, or separate items with commas.

**Resistance level mode** uses FTMS *Set Target Resistance Level* and maps 0–100 % onto the trainer's reported range. If your firmware ignores it, use gradient mode instead.

## Zwift Cog / single-speed setup

Turn on **Profile → Drivetrain → Virtual shifting**, then pick your wheel size and starting gear.

- **ERG workouts and the ramp test** ignore gears: the trainer sets the power, as normal.
- **Routes, free rides, test efforts, and workouts switched to resistance mode** use the virtual gears. When the cadence-implied speed needs no power (coasting or steep descents), resistance drops to zero, like freewheeling.
- **Zwift Click / Play** buttons use an undocumented, encrypted Zwift protocol, so they aren't supported. A cheap Bluetooth page-turner (it acts as a keyboard) or the keyboard works today.

## Project layout

```
crates/corekick-core/     Pure Rust, no UI or Bluetooth – everything here is unit-tested
  ftms.rs                 FTMS byte protocol: Indoor Bike Data parser, control point, HR
  physics.rs              forces, steady-state solver, momentum integration
  route.rs                profiles, stats, climb detection, generator, segment builder, GPX
  workout.rs              blocks → steps, estimates, built-in library
  gears.rs                virtual shifting (Zwift Cog): gear table, cadence → speed → watts
  fitness_test.rs         ramp / 20-min / 2×8 protocols + FTP evaluation
  ftp_estimate.rs         eFTP: critical-power fit on 90-day mean-max, suggestion rules
  engine.rs               4 Hz ride engine: SIM/ERG control, laps, ghost, recording, energy
  metrics.rs, stats.rs    NP/IF/TSS, bests, zones, CTL/ATL/TSB, records
  events.rs               recurrence expansion
  db.rs                   SQLite (bundled) persistence
  export.rs, fit.rs       TCX / CSV / FIT export
src-tauri/                Tauri shell
  ble.rs                  btleplug scanning, FTMS supervisor (reconnect, serialized control
                          point writes, coalescing), heart-rate monitor
  sim.rs                  virtual trainer + rider
  strava.rs               OAuth (loopback), token refresh, FIT upload + polling
  commands.rs             IPC commands
  lib.rs                  app state, ride loop, telemetry broadcast
src/                      React + TypeScript UI (no UI framework; custom SVG charts)
```

Data is stored in `~/Library/Application Support/com.corekick.app/corekick.db`.

## How it works

The ride loop runs at 4 Hz. Each tick:

1. Reads trainer telemetry (Bluetooth or simulator).
2. Runs the engine, which integrates speed from power on the *true* route gradient, advances distance and elevation, records 1 Hz samples and integrates energy.
3. If the target changed, sends a trainer command. SIM commands are throttled to 2 Hz with 0.1% resolution, and ERG to 1 W changes.
4. Emits a snapshot to the UI.

FTMS control-point writes are serialized: one outstanding request at a time, and the next is sent only after its indication arrives or a 2 s timeout passes.

## Tests

```bash
cargo test -p corekick-core     # 87 tests: protocol, physics, generator, engine, tests, stats…
npm run typecheck
```
