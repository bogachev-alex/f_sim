//! Настройка команды для позиционирования: слоты схемы, роли, стиль, личные параметры игроков.

use crate::attrs::{Attr, ATTR_COUNT};
use crate::config::{
    Config, FullbackRole, PhaseOffset, Style, FULLBACK_ROLE_INVERTED, FULLBACK_ROLE_OVERLAPPING,
    FULLBACK_ROLE_STAY,
};
use crate::curves::CurveSet;
use crate::data::{Position, Squad, STARTERS};
use crate::physics::body::BodyParams;
use crate::skills::{Mental, Skills};

/// Линия, к которой относится игрок.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Keeper,
    /// Центральные и фланговые защитники и «крылья»: согласуют линию обороны.
    Back,
    Other,
}

#[derive(Clone, Debug)]
pub struct SlotShape {
    pub pos: Position,
    pub group: Group,
    /// Базовая точка в метрах в системе команды.
    pub x: f32,
    pub y: f32,
    /// Глубина слота между линией защитников (0) и самым передним игроком (1).
    pub depth: f32,
    pub attack: PhaseOffset,
    pub defense: PhaseOffset,
    pub role: String,
}

#[derive(Clone, Debug)]
pub struct TeamSetup {
    pub style: Style,
    pub formation: String,
    pub slots: Vec<SlotShape>,
    pub body: [BodyParams; STARTERS],
    /// Эффективные (после кривых) значения 0–1.
    pub concentration: [f32; STARTERS],
    pub work_rate: [f32; STARTERS],
    pub teamwork: [f32; STARTERS],
    pub skills: [Skills; STARTERS],
    pub mental: [Mental; STARTERS],
    pub cohesion: f32,
    pub familiarity: f32,
}

fn group_of(pos: Position) -> Group {
    match pos {
        Position::GK => Group::Keeper,
        Position::CB | Position::FB | Position::WB => Group::Back,
        _ => Group::Other,
    }
}

/// Роль слота: фланговые защитники берут роль из стиля, остальные играют свою.
fn resolve_role<'a>(style: &Style, pos: Position, slot_role: &'a str) -> &'a str {
    if pos != Position::FB {
        return slot_role;
    }
    match style.fullback_role {
        FullbackRole::Overlapping => FULLBACK_ROLE_OVERLAPPING,
        FullbackRole::Inverted => FULLBACK_ROLE_INVERTED,
        FullbackRole::Stay => FULLBACK_ROLE_STAY,
    }
}

impl TeamSetup {
    /// Коммуникация линии 0–1: среднее `teamwork` игроков и сыгранность команды
    /// (`docs/04-off-ball.md` §2, передача опеки).
    pub fn communication(&self) -> f32 {
        let tw = self.teamwork.iter().sum::<f32>() / STARTERS as f32;
        0.5 * (tw + self.cohesion)
    }

    /// Команда из состава: схема и роли из конфига, параметры игроков из атрибутов.
    pub fn from_squad(cfg: &Config, squad: &Squad, style: &Style) -> Result<TeamSetup, String> {
        let attrs: Vec<[f32; ATTR_COUNT]> = squad
            .players
            .iter()
            .take(STARTERS)
            .map(|p| p.attrs_array())
            .collect();
        Self::build(cfg, &squad.formation, style, &attrs, Some(squad))
    }

    /// Команда из одинаковых игроков: для тестов, CLI и сценариев без составов.
    pub fn uniform(
        cfg: &Config,
        formation: &str,
        style: &Style,
        value: f32,
    ) -> Result<TeamSetup, String> {
        let attrs = vec![[value; ATTR_COUNT]; STARTERS];
        Self::build(cfg, formation, style, &attrs, None)
    }

    fn build(
        cfg: &Config,
        formation: &str,
        style: &Style,
        attrs: &[[f32; ATTR_COUNT]],
        squad: Option<&Squad>,
    ) -> Result<TeamSetup, String> {
        let slots = cfg
            .formations
            .0
            .get(formation)
            .ok_or_else(|| format!("схема `{formation}` не найдена"))?;
        if attrs.len() != STARTERS {
            return Err(format!(
                "нужно {STARTERS} стартовых игроков, получено {}",
                attrs.len()
            ));
        }
        let curves = CurveSet::build(&cfg.curves).map_err(|e| e.to_string())?;
        let (len, wid) = (cfg.physics.pitch.length_m, cfg.physics.pitch.width_m);

        // Глубина слотов: линия защитников (центральные и фланговые) и самый передний игрок.
        let back_xs: Vec<f32> = slots
            .iter()
            .filter(|s| matches!(s.pos, Position::CB | Position::FB))
            .map(|s| s.x * len)
            .collect();
        let x_ref = back_xs.iter().sum::<f32>() / back_xs.len().max(1) as f32;
        let x_front = slots
            .iter()
            .filter(|s| s.pos != Position::GK)
            .map(|s| s.x * len)
            .fold(f32::MIN, f32::max);
        let span = (x_front - x_ref).max(1e-3);

        let mut shapes = Vec::with_capacity(STARTERS);
        for (i, s) in slots.iter().enumerate() {
            let role_id = match squad {
                Some(sq) => resolve_role(style, s.pos, &sq.players[i].role).to_string(),
                None => resolve_role(style, s.pos, &s.role).to_string(),
            };
            let role = cfg
                .roles
                .get(&role_id)
                .ok_or_else(|| format!("роль `{role_id}` не найдена"))?;
            let x = s.x * len;
            shapes.push(SlotShape {
                pos: s.pos,
                group: group_of(s.pos),
                x,
                y: (s.y - 0.5) * wid,
                depth: (x - x_ref) / span,
                attack: role.attack,
                defense: role.defense,
                role: role_id,
            });
        }

        let mut body = [BodyParams {
            max_speed: 0.0,
            accel: 0.0,
            decel: 0.0,
            lat_accel: 0.0,
        }; STARTERS];
        let mut concentration = [0.0; STARTERS];
        let mut work_rate = [0.0; STARTERS];
        let mut teamwork = [0.0; STARTERS];
        let mut skills = [Skills::default(); STARTERS];
        let mut mental = [Mental::default(); STARTERS];
        for i in 0..STARTERS {
            body[i] = BodyParams::from_attrs(&attrs[i], &curves, &cfg.physics);
            concentration[i] =
                curves.eval(Attr::concentration, attrs[i][Attr::concentration as usize]);
            work_rate[i] = curves.eval(Attr::work_rate, attrs[i][Attr::work_rate as usize]);
            teamwork[i] = curves.eval(Attr::teamwork, attrs[i][Attr::teamwork as usize]);
            skills[i] = Skills::from_attrs(&attrs[i], &curves, &cfg.composites);
            mental[i] = Mental::from_attrs(&attrs[i], &curves);
        }
        Ok(TeamSetup {
            style: style.clone(),
            formation: formation.to_string(),
            slots: shapes,
            body,
            concentration,
            work_rate,
            teamwork,
            skills,
            mental,
            cohesion: cfg.positioning.line.default_cohesion,
            familiarity: cfg.positioning.line.default_familiarity,
        })
    }
}
