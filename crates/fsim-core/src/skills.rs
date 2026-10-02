//! Эффективные навыки: композиты из атрибутов через кривые отклика (`docs/01-data-model.md`
//! §1, §3). Модификаторы формы дня, уверенности, усталости и контекста появятся на M6.

use crate::attrs::{Attr, Attrs};
use crate::config::Composites;
use crate::curves::CurveSet;

/// Навыки одного игрока, 0–1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Skills {
    pub short_pass: f32,
    pub long_pass: f32,
    pub cross: f32,
    pub control: f32,
    pub dribble: f32,
    pub finish: f32,
    pub long_shot: f32,
    pub tackle: f32,
    pub aerial: f32,
    pub shield: f32,
    pub awareness: f32,
    pub press_resist: f32,
    pub gk_shot_stop: f32,
}

/// Ментальные значения и черты, которые читают решения (после кривых отклика).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mental {
    pub vision: f32,
    pub decisions: f32,
    pub composure: f32,
    pub anticipation: f32,
    pub aggression: f32,
    pub bravery: f32,
    pub flair: f32,
    pub balance: f32,
    pub strength: f32,
}

impl Skills {
    pub fn from_attrs(attrs: &Attrs, curves: &CurveSet, composites: &Composites) -> Skills {
        let comp = |name: &str| -> f32 {
            composites.0[name]
                .iter()
                .map(|(attr, w)| {
                    let a = Attr::from_name(attr).expect("атрибуты проверены при загрузке");
                    w * curves.eval(a, attrs[a as usize])
                })
                .sum()
        };
        Skills {
            short_pass: comp("short_pass"),
            long_pass: comp("long_pass"),
            cross: comp("cross"),
            control: comp("control"),
            dribble: comp("dribble"),
            finish: comp("finish"),
            long_shot: comp("long_shot"),
            tackle: comp("tackle"),
            aerial: comp("aerial"),
            shield: comp("shield"),
            awareness: comp("awareness"),
            press_resist: comp("press_resist"),
            gk_shot_stop: comp("gk_shot_stop"),
        }
    }
}

impl Mental {
    pub fn from_attrs(attrs: &Attrs, curves: &CurveSet) -> Mental {
        let e = |a: Attr| curves.eval(a, attrs[a as usize]);
        Mental {
            vision: e(Attr::vision),
            decisions: e(Attr::decisions),
            composure: e(Attr::composure),
            anticipation: e(Attr::anticipation),
            aggression: e(Attr::aggression),
            bravery: e(Attr::bravery),
            flair: e(Attr::flair),
            balance: e(Attr::balance),
            strength: e(Attr::strength),
        }
    }
}
