use crate::info::FrameLayout;
use fsim_core::sink::FrameSink;
use fsim_core::world::{World, PLAYERS};

/// Копит кадры физики в плоском буфере для отправки в визуализатор.
pub struct RecordingSink {
    buf: Vec<f32>,
}

impl RecordingSink {
    pub fn new() -> Self {
        RecordingSink { buf: Vec::new() }
    }

    pub fn take(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.buf)
    }
}

impl Default for RecordingSink {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameSink for RecordingSink {
    fn frame(&mut self, tick: u32, w: &World) {
        let l = FrameLayout::new();
        self.buf.reserve(l.stride as usize);
        // Порядок полей совпадает с `FrameLayout`.
        self.buf.push(tick as f32);
        let p = &w.players;
        for i in 0..PLAYERS {
            self.buf.extend_from_slice(&[
                p.pos_x[i],
                p.pos_y[i],
                p.vel_x[i],
                p.vel_y[i],
                p.facing[i],
                p.target_x[i],
                p.target_y[i],
                p.speed_cap[i],
                p.anchor_x[i],
                p.anchor_y[i],
                f32::from(p.duty[i]),
                f32::from(p.duty_ref[i]),
            ]);
        }
        for t in &w.teams {
            self.buf
                .extend_from_slice(&[t.attack_w, t.line_x, t.offside_x, f32::from(t.has_ball)]);
        }
        let (bp, bv) = (w.ball_pos, w.ball_vel);
        self.buf.extend_from_slice(&[
            bp.x,
            bp.y,
            bp.z,
            bv.x,
            bv.y,
            bv.z,
            f32::from(w.ball_mode),
            f32::from(w.ball_actor),
        ]);
    }
}
