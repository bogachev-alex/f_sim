//! Якорные точки игроков команды в системе команды.

use super::lerp;
use super::team::{Group, TeamSetup};
use crate::config::{PhaseOffset, PositioningConfig};
use crate::data::STARTERS;
use glam::Vec2;

/// Входные данные формы: положение мяча в системе команды и вес фазы атаки (0 оборона, 1 атака).
#[derive(Clone, Copy, Debug)]
pub struct ShapeInput {
    pub ball: Vec2,
    pub attack_w: f32,
}

/// Размеры поля.
#[derive(Clone, Copy, Debug)]
pub struct Pitch {
    pub length: f32,
    pub width: f32,
}

/// Смещение к центру на `inward` метров по знаку базового положения; не переходит через ось.
#[inline]
fn apply_inward(y: f32, base_side: f32, inward: f32) -> f32 {
    if base_side == 0.0 || inward == 0.0 {
        return y;
    }
    if inward > 0.0 {
        // Ближе к центру, но не дальше оси, если игрок был на своей стороне.
        let moved = y - base_side * inward;
        if moved * base_side < 0.0 {
            0.0
        } else {
            moved
        }
    } else {
        y - base_side * inward
    }
}

/// Положение линии обороны (общая координата `X`) в обороне и в атаке.
#[derive(Clone, Copy, Debug)]
pub struct LineX {
    pub defense: f32,
    pub attack: f32,
}

/// Считает якоря всех 11 игроков. `line_dev`: индивидуальные отклонения защитников от
/// общей линии (`positioning::line`). Возвращает положение линии обороны в двух фазах.
pub fn compute(
    team: &TeamSetup,
    cfg: &PositioningConfig,
    pitch: Pitch,
    input: ShapeInput,
    line_dev: &[f32; STARTERS],
    out: &mut [Vec2; STARTERS],
) -> LineX {
    let st = &team.style;
    let (d, a, k) = (&cfg.defense, &cfg.attack, &cfg.keeper);
    let half_len = pitch.length * 0.5;
    let ball = input.ball;

    // ---- оборона ----
    let def_follow =
        (d.line_follow_k * (ball.x - half_len)).clamp(-d.line_follow_max_m, d.line_follow_max_m);
    let def_back = (lerp(d.back_line_min_m, d.back_line_max_m, st.line_height) + def_follow)
        .min((ball.x - d.line_behind_ball_m).max(d.back_line_min_m));
    let front = lerp(d.front_line_min_m, d.front_line_max_m, st.block_height);
    let depth_max = lerp(d.depth_loose_m, d.depth_compact_m, st.compactness);
    let def_depth = (front - (def_back - def_follow))
        .max(d.depth_floor_m)
        .min(depth_max);
    let def_width = lerp(d.width_scale_loose, d.width_scale_compact, st.compactness);
    let def_shift = lerp(
        d.ball_lateral_k_loose,
        d.ball_lateral_k_compact,
        st.compactness,
    ) * ball.y;

    // ---- атака ----
    let att_follow =
        (a.line_follow_k * (ball.x - half_len)).clamp(-a.line_follow_max_m, a.line_follow_max_m);
    let att_back = lerp(a.back_line_min_m, a.back_line_max_m, st.line_height) + att_follow;
    let att_front = lerp(a.front_line_min_m, a.front_line_max_m, st.block_height) + att_follow;
    let att_depth = (att_front - att_back).max(d.depth_floor_m);
    let att_width = lerp(a.width_scale_min, a.width_scale_max, st.width_in);
    let att_shift = a.ball_lateral_k * ball.y;

    let w = input.attack_w.clamp(0.0, 1.0);
    let x_lo = cfg.margin_m;
    let x_hi = pitch.length - cfg.margin_m;
    let y_lim = pitch.width * 0.5 - cfg.margin_m;

    for (i, s) in team.slots.iter().enumerate() {
        let side = if s.y > 0.0 {
            1.0
        } else if s.y < 0.0 {
            -1.0
        } else {
            0.0
        };
        let (def_x, def_y, att_x, att_y);
        if s.group == Group::Keeper {
            def_x = k.base_m + s.defense.dx_m + k.line_follow_k * (def_back - d.back_line_min_m);
            att_x = k.base_m + s.attack.dx_m + k.line_follow_k * (att_back - a.back_line_min_m);
            let lat = (k.lateral_k * ball.y).clamp(-k.max_lateral_m, k.max_lateral_m);
            def_y = lat;
            att_y = lat;
        } else {
            let u_def = if s.group == Group::Back {
                0.0
            } else {
                s.depth.max(0.0)
            };
            let dev = if s.group == Group::Back {
                line_dev[i]
            } else {
                0.0
            };
            def_x = def_back + u_def * def_depth + s.defense.dx_m + dev;
            att_x = att_back + s.depth * att_depth + s.attack.dx_m;
            def_y = apply_inward(s.y * def_width + def_shift, side, s.defense.inward_m);
            att_y = apply_inward(s.y * att_width + att_shift, side, s.attack.inward_m);
        }
        out[i] = Vec2::new(
            lerp(def_x, att_x, w).clamp(x_lo, x_hi),
            lerp(def_y, att_y, w).clamp(-y_lim, y_lim),
        );
    }
    LineX {
        defense: def_back,
        attack: att_back,
    }
}

/// Смещение роли для проверок и отладки.
pub fn role_offset(team: &TeamSetup, i: usize, attack: bool) -> PhaseOffset {
    if attack {
        team.slots[i].attack
    } else {
        team.slots[i].defense
    }
}
