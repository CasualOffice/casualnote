//! A seeded generator, so any failure replays exactly (docs/24, Seeds).

#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// In `0..n`. `n` of 0 gives 0.
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }

    /// In `lo..=hi`.
    pub fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + self.below(hi - lo + 1)
    }

    /// True `percent` times in a hundred.
    pub fn chance(&mut self, percent: f64) -> bool {
        (self.next_u64() % 1_000_000) as f64 / 10_000.0 < percent
    }

    /// A rough power law: small values are much more likely, as with word and page popularity.
    pub fn skewed(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        let u = (self.next_u64() % 1_000_000) as f64 / 1_000_000.0;
        ((u * u * u * n as f64) as usize).min(n - 1)
    }
}
