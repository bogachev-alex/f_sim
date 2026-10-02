//! Согласование линии обороны: индивидуальные отклонения защитников от общей линии.
//! Отклонение — процесс Орнштейна—Уленбека: у защитников с хорошей концентрацией, сыгранностью
//! и тактической слаженностью линия ровнее.

use super::team::{Group, TeamSetup};
use crate::config::LineScatter;
use crate::data::STARTERS;
use crate::rng::Rng;

#[derive(Clone, Debug)]
pub struct LineState {
    pub dev: [f32; STARTERS],
    /// Стационарное стандартное отклонение каждого игрока, м.
    sd: [f32; STARTERS],
}

impl LineState {
    pub fn new(team: &TeamSetup, cfg: &LineScatter) -> LineState {
        let mut sd = [0.0; STARTERS];
        for (i, s) in team.slots.iter().enumerate() {
            if s.group == Group::Back {
                sd[i] = cfg.scatter_sd_m
                    * (1.0 - cfg.concentration_weight * team.concentration[i])
                    * (1.0 - cfg.cohesion_weight * team.cohesion)
                    * (1.0 - cfg.familiarity_weight * team.familiarity);
            }
        }
        LineState {
            dev: [0.0; STARTERS],
            sd,
        }
    }

    /// Шаг процесса на `dt` секунд. Без аллокаций.
    pub fn step(&mut self, cfg: &LineScatter, rng: &mut Rng, dt: f32) {
        let decay = libm::expf(-dt / cfg.scatter_tau_s);
        // Точное обновление OU: x' = x e^{-dt/τ} + σ √(1 − e^{-2dt/τ}) ξ.
        let noise = libm::sqrtf(1.0 - decay * decay);
        for i in 0..STARTERS {
            if self.sd[i] > 0.0 {
                self.dev[i] = self.dev[i] * decay + self.sd[i] * noise * rng.next_normal();
            }
        }
    }
}
