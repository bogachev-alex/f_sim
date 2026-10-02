//! Венгерский алгоритм для прямоугольной матрицы стоимости (строк не больше столбцов).
//! Вариант с потенциалами, O(n² m). Массивы фиксированного размера: без аллокаций.

pub const MAX_ROWS: usize = 10;
pub const MAX_COLS: usize = 20;

/// Матрица `n × m` стоимостей, заполняется строка за строкой.
pub struct CostMatrix {
    pub n: usize,
    pub m: usize,
    cost: [[f32; MAX_COLS]; MAX_ROWS],
}

impl CostMatrix {
    pub fn new(n: usize, m: usize) -> CostMatrix {
        assert!(
            n <= m && n <= MAX_ROWS && m <= MAX_COLS,
            "матрица {n}×{m} вне допустимых размеров"
        );
        CostMatrix {
            n,
            m,
            cost: [[0.0; MAX_COLS]; MAX_ROWS],
        }
    }

    #[inline]
    pub fn set(&mut self, row: usize, col: usize, v: f32) {
        self.cost[row][col] = v;
    }

    #[inline]
    pub fn get(&self, row: usize, col: usize) -> f32 {
        self.cost[row][col]
    }
}

/// Оптимальное назначение: для каждой строки номер столбца, суммарная стоимость минимальна.
/// Каждый столбец достаётся не больше чем одной строке. Вычисления в `f64`.
pub fn solve(c: &CostMatrix, out: &mut [usize; MAX_ROWS]) {
    let (n, m) = (c.n, c.m);
    let mut u = [0.0f64; MAX_ROWS + 1];
    let mut v = [0.0f64; MAX_COLS + 1];
    let mut p = [0usize; MAX_COLS + 1];
    let mut way = [0usize; MAX_COLS + 1];
    for i in 1..=n {
        p[0] = i;
        let mut j0 = 0usize;
        let mut minv = [f64::INFINITY; MAX_COLS + 1];
        let mut used = [false; MAX_COLS + 1];
        loop {
            used[j0] = true;
            let i0 = p[j0];
            let mut delta = f64::INFINITY;
            let mut j1 = 0usize;
            for j in 1..=m {
                if !used[j] {
                    let cur = c.cost[i0 - 1][j - 1] as f64 - u[i0] - v[j];
                    if cur < minv[j] {
                        minv[j] = cur;
                        way[j] = j0;
                    }
                    if minv[j] < delta {
                        delta = minv[j];
                        j1 = j;
                    }
                }
            }
            for j in 0..=m {
                if used[j] {
                    u[p[j]] += delta;
                    v[j] -= delta;
                } else {
                    minv[j] -= delta;
                }
            }
            j0 = j1;
            if p[j0] == 0 {
                break;
            }
        }
        loop {
            let j1 = way[j0];
            p[j0] = p[j1];
            j0 = j1;
            if j0 == 0 {
                break;
            }
        }
    }
    for j in 1..=m {
        if p[j] != 0 {
            out[p[j] - 1] = j - 1;
        }
    }
}

/// Суммарная стоимость назначения.
pub fn total(c: &CostMatrix, assign: &[usize; MAX_ROWS]) -> f64 {
    (0..c.n).map(|i| c.get(i, assign[i]) as f64).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Rng;

    /// Полный перебор для сверки.
    fn brute(c: &CostMatrix) -> f64 {
        fn rec(c: &CostMatrix, row: usize, used: &mut [bool; MAX_COLS], acc: f64, best: &mut f64) {
            if row == c.n {
                *best = best.min(acc);
                return;
            }
            for j in 0..c.m {
                if !used[j] {
                    used[j] = true;
                    rec(c, row + 1, used, acc + c.get(row, j) as f64, best);
                    used[j] = false;
                }
            }
        }
        let mut best = f64::INFINITY;
        rec(c, 0, &mut [false; MAX_COLS], 0.0, &mut best);
        best
    }

    #[test]
    fn matches_brute_force_on_random_matrices() {
        let mut rng = Rng::new(7, 0);
        for case in 0..400 {
            let n = 1 + (rng.next_u64() % 6) as usize;
            let m = n + (rng.next_u64() % 4) as usize;
            let mut c = CostMatrix::new(n, m);
            for i in 0..n {
                for j in 0..m {
                    c.set(i, j, rng.range_f32(-5.0, 20.0));
                }
            }
            let mut a = [0usize; MAX_ROWS];
            solve(&c, &mut a);
            // Все столбцы разные.
            for i in 0..n {
                for k in i + 1..n {
                    assert_ne!(a[i], a[k], "случай {case}: один столбец двум строкам");
                }
            }
            let (got, want) = (total(&c, &a), brute(&c));
            assert!(
                (got - want).abs() < 1e-4,
                "случай {case}: {got} против {want}"
            );
        }
    }

    #[test]
    fn known_assignment() {
        let mut c = CostMatrix::new(3, 3);
        let rows = [[4.0, 1.0, 3.0], [2.0, 0.0, 5.0], [3.0, 2.0, 2.0]];
        for (i, r) in rows.iter().enumerate() {
            for (j, v) in r.iter().enumerate() {
                c.set(i, j, *v);
            }
        }
        let mut a = [0usize; MAX_ROWS];
        solve(&c, &mut a);
        assert_eq!(&a[..3], &[1, 0, 2]);
        assert!((total(&c, &a) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn bonus_columns_are_always_filled() {
        // Два «особых» столбца со стоимостью -1000 заполняются, даже если игроки далеко.
        let mut c = CostMatrix::new(4, 6);
        for i in 0..4 {
            for j in 0..6 {
                c.set(i, j, if j < 2 { 50.0 - 1000.0 } else { (i + j) as f32 });
            }
        }
        let mut a = [0usize; MAX_ROWS];
        solve(&c, &mut a);
        let filled = |col: usize| (0..4).any(|i| a[i] == col);
        assert!(filled(0) && filled(1));
    }

    #[test]
    fn full_size_is_fast_and_valid() {
        let mut rng = Rng::new(3, 0);
        let mut c = CostMatrix::new(MAX_ROWS, MAX_COLS);
        for i in 0..MAX_ROWS {
            for j in 0..MAX_COLS {
                c.set(i, j, rng.range_f32(0.0, 10.0));
            }
        }
        let mut a = [0usize; MAX_ROWS];
        solve(&c, &mut a);
        let mut seen = [false; MAX_COLS];
        for &j in &a[..MAX_ROWS] {
            assert!(!seen[j]);
            seen[j] = true;
        }
    }
}
