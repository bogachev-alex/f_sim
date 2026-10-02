//! Атрибуты игрока. Порядок и имена совпадают с `config/attributes.json`
//! (проверяется тестом), значения хранятся как `f32` в [0, 1].

macro_rules! attrs {
    ($($group:ident: [$($name:ident),* $(,)?]),* $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
        #[serde(rename_all = "snake_case")]
        #[repr(u8)]
        #[allow(non_camel_case_types)]
        pub enum Attr { $($($name),*),* }

        impl Attr {
            pub const ALL: &'static [Attr] = &[$($(Attr::$name),*),*];
            pub const NAMES: &'static [&'static str] = &[$($(stringify!($name)),*),*];
            /// Группа атрибута: ключ в `attributes.json` и в `response_curves.json`.
            pub fn group(self) -> &'static str {
                match self { $($(Attr::$name => stringify!($group)),*),* }
            }
            pub fn name(self) -> &'static str { Self::NAMES[self as usize] }
            pub fn from_name(s: &str) -> Option<Attr> {
                Self::NAMES.iter().position(|n| *n == s).map(|i| Self::ALL[i])
            }
        }
    };
}

attrs! {
    physical: [pace, acceleration, agility, balance, strength, stamina, natural_fitness,
               jumping_reach, recovery],
    technical: [passing, crossing, long_shots, finishing, first_touch, technique, dribbling,
                heading, tackling, marking, free_kicks, corners, penalties, long_throws],
    mental: [vision, decisions, anticipation, positioning, off_the_ball, composure,
             concentration, work_rate, teamwork, aggression, bravery, flair],
    goalkeeping: [reflexes, handling, one_on_ones, aerial_command, kicking, throwing,
                  rushing_out, gk_positioning],
    traits: [consistency, big_matches, pressure_handling, temperament, leadership],
}

pub const ATTR_COUNT: usize = 48;

/// Значения всех атрибутов игрока.
pub type Attrs = [f32; ATTR_COUNT];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_matches() {
        assert_eq!(Attr::ALL.len(), ATTR_COUNT);
        assert_eq!(Attr::NAMES.len(), ATTR_COUNT);
    }
}
