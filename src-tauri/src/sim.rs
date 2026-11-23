//! A virtual trainer + rider so the whole app can be used without hardware.
//!
//! The "rider" pushes `effort_w` (set from the UI). In ERG mode the trainer
//! drags power towards the target with a short lag; if the target is far above
//! what the rider can do, they "fail" (cadence and power collapse), which lets
//! the ramp test run end-to-end in the simulator.

use corekick_core::engine::{TrainerCommand, TrainerInput};
use corekick_core::model::Profile;
use corekick_core::rng::Rng;

pub struct SimTrainer {
    pub effort_w: f64,
    power: f64,
    cadence: f64,
    hr: f64,
    mode: Option<TrainerCommand>,
    rng: Rng,
}

impl SimTrainer {
    pub fn new(profile: &Profile) -> Self {
        SimTrainer {
            effort_w: (profile.ftp * 0.7).round(),
            power: 0.0,
            cadence: 0.0,
            hr: profile.rest_hr as f64,
            mode: None,
            rng: Rng::new(0xC0FFEE),
        }
    }

    pub fn command(&mut self, c: TrainerCommand) {
        self.mode = Some(c);
    }

    pub fn update(&mut self, dt: f64, profile: &Profile) -> TrainerInput {
        let ftp = profile.ftp.max(80.0);
        let effort = self.effort_w.max(0.0);
        let (target, cad_target) = match self.mode {
            Some(TrainerCommand::TargetPower { watts }) => {
                if effort > 0.0 && watts > effort * 1.35 {
                    (watts * 0.35, 38.0) // rider can't hold it
                } else if effort <= 0.0 {
                    (0.0, 0.0)
                } else {
                    (watts, 90.0)
                }
            }
            Some(TrainerCommand::Resistance { level_pct }) => {
                // Fixed torque: heavier levels mean more watts at the same cadence.
                if effort <= 0.0 { (0.0, 0.0) } else { (effort * (0.55 + level_pct / 100.0), 88.0) }
            }
            Some(TrainerCommand::Simulation { grade_pct, .. }) => {
                let cad = if effort <= 0.0 { 0.0 } else { (92.0 - grade_pct.max(0.0) * 2.0).clamp(65.0, 100.0) };
                (effort, cad)
            }
            None => (effort, if effort > 0.0 { 88.0 } else { 0.0 }),
        };
        let a = (dt / 1.5).min(1.0);
        self.power += (target - self.power) * a;
        self.cadence += (cad_target - self.cadence) * (dt / 1.0).min(1.0);
        let noise = 1.0 + self.rng.normal() * 0.025;
        let power = (self.power * noise).max(0.0);
        let cadence = (self.cadence + self.rng.normal() * 1.2).max(0.0);

        let rest = profile.rest_hr as f64;
        let max = profile.max_hr.max(profile.rest_hr + 40) as f64;
        let frac = (0.30 + 0.62 * self.power / ftp).clamp(0.0, 1.0);
        let hr_target = if self.power < 5.0 { rest + 15.0 } else { rest + (max - rest) * frac };
        self.hr += (hr_target - self.hr) * (dt / 25.0).min(1.0);

        TrainerInput {
            power_w: if self.power < 1.0 { 0.0 } else { power.round() },
            cadence_rpm: if self.cadence < 5.0 { 0.0 } else { cadence.round() },
            heart_rate: Some(self.hr.round()),
            connected: true,
        }
    }
}
