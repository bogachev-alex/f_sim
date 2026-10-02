//! Потоки PRNG. Один поток на подсистему, чтобы изменение одной подсистемы
//! не сдвигало случайные числа другой.

use rand_core::{RngCore, SeedableRng};
use rand_xoshiro::Xoshiro256PlusPlus;

/// SplitMix64: один шаг. Состояние увеличивается на золотую константу, результат смешивается.
#[inline]
pub fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Seed матча в пакете: выводится из мастер-seed и индекса матча.
pub fn match_seed(master: u64, index: u64) -> u64 {
    let mut s = master ^ index.wrapping_mul(0xD1B5_4A32_D192_ED03);
    splitmix64(&mut s)
}

/// Подсистемы со своими потоками. Порядок вариантов входит в формат seed, не переставлять.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Stream {
    Init,
    Perception,
    Decision,
    Execution,
    Duels,
    SetPieces,
    Referee,
    Dynamics,
    Physics,
}

impl Stream {
    pub const COUNT: usize = 9;
}

pub struct Rng(Xoshiro256PlusPlus);

impl Rng {
    /// Независимый поток по seed и номеру потока.
    pub fn new(seed: u64, stream: u64) -> Self {
        let mut sm = seed ^ stream.wrapping_mul(0xA24B_AED4_963E_E407);
        let mut bytes = [0u8; 32];
        for i in 0..4 {
            bytes[i * 8..(i + 1) * 8].copy_from_slice(&splitmix64(&mut sm).to_le_bytes());
        }
        Rng(Xoshiro256PlusPlus::from_seed(bytes))
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }

    /// Равномерное число в [0, 1). Верхние 24 бита, поэтому результат точно представим в `f32`.
    #[inline]
    pub fn next_f32(&mut self) -> f32 {
        (self.0.next_u32() >> 8) as f32 * (1.0 / 16_777_216.0)
    }

    /// Стандартное нормальное число (Box–Muller через `libm`, второе значение пары отбрасывается).
    #[inline]
    pub fn next_normal(&mut self) -> f32 {
        // 1 - u лежит в (0, 1], логарифм определён.
        let u1 = 1.0 - self.next_f32();
        let u2 = self.next_f32();
        libm::sqrtf(-2.0 * libm::logf(u1)) * libm::cosf(core::f32::consts::TAU * u2)
    }

    #[inline]
    pub fn range_f32(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next_f32()
    }
}

/// Набор потоков одного матча.
pub struct MatchRng {
    streams: [Rng; Stream::COUNT],
}

impl MatchRng {
    pub fn new(seed: u64) -> Self {
        let streams = core::array::from_fn(|i| Rng::new(seed, i as u64 + 1));
        MatchRng { streams }
    }

    #[inline]
    pub fn get(&mut self, s: Stream) -> &mut Rng {
        &mut self.streams[s as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitmix_reference_vector() {
        // Эталон SplitMix64 для seed 0 (из референсной реализации Vigna).
        let mut s = 0u64;
        assert_eq!(splitmix64(&mut s), 0xE220_A839_7B1D_CDAF);
        assert_eq!(splitmix64(&mut s), 0x6E78_9E6A_A1B9_65F4);
    }

    #[test]
    fn same_seed_same_sequence() {
        let mut a = MatchRng::new(7);
        let mut b = MatchRng::new(7);
        for _ in 0..100 {
            assert_eq!(
                a.get(Stream::Decision).next_u64(),
                b.get(Stream::Decision).next_u64()
            );
        }
    }

    #[test]
    fn streams_are_independent() {
        let mut a = MatchRng::new(7);
        let mut b = MatchRng::new(7);
        // Расход потока Duels в `a` не должен менять поток Decision.
        for _ in 0..50 {
            a.get(Stream::Duels).next_u64();
        }
        assert_eq!(
            a.get(Stream::Decision).next_u64(),
            b.get(Stream::Decision).next_u64()
        );
        assert_ne!(
            a.get(Stream::Referee).next_u64(),
            a.get(Stream::Dynamics).next_u64()
        );
    }

    #[test]
    fn match_seeds_differ_by_index() {
        assert_ne!(match_seed(1, 0), match_seed(1, 1));
        assert_ne!(match_seed(1, 0), match_seed(2, 0));
        assert_eq!(match_seed(1, 5), match_seed(1, 5));
    }

    #[test]
    fn f32_in_unit_interval() {
        let mut r = MatchRng::new(3);
        for _ in 0..10_000 {
            let x = r.get(Stream::Init).next_f32();
            assert!((0.0..1.0).contains(&x));
        }
    }
}
