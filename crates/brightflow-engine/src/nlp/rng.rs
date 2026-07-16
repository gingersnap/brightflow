//! Seeded PRNG shared by the NLP primitives.
//!
//! Deliberately tiny: avoids a `rand` dependency and guarantees reproducible
//! clustering, shuffles and splits across builds and platforms.

/// SplitMix64 — tiny, fast, statistically fine for seeding.
///
/// Reproducibility is the point: every consumer that seeds from a constant
/// gets identical output on every build, platform and run.
#[derive(Debug, Clone)]
pub struct SplitMix64(u64);

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform f32 in [0, 1).
    pub fn next_f32(&mut self) -> f32 {
        ((self.next_u64() >> 40) as f32) / ((1u32 << 24) as f32)
    }

    /// Uniform usize in [0, bound). Returns 0 for an empty bound.
    pub fn next_bounded(&mut self, bound: usize) -> usize {
        if bound == 0 {
            return 0;
        }
        (self.next_u64() % (bound as u64)) as usize
    }

    /// In-place Fisher-Yates shuffle.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.next_bounded(i + 1);
            items.swap(i, j);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp, clippy::indexing_slicing)]
mod tests {
    use super::SplitMix64;

    #[test]
    fn deterministic_across_instances() {
        let mut a = SplitMix64::new(42);
        let mut b = SplitMix64::new(42);
        for _ in 0..64 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn next_f32_in_unit_interval() {
        let mut rng = SplitMix64::new(7);
        for _ in 0..1_000 {
            let v = rng.next_f32();
            assert!((0.0..1.0).contains(&v), "out of range: {v}");
        }
    }

    #[test]
    fn next_bounded_respects_bound_and_handles_zero() {
        let mut rng = SplitMix64::new(9);
        for _ in 0..1_000 {
            assert!(rng.next_bounded(5) < 5);
        }
        assert_eq!(rng.next_bounded(0), 0);
    }

    #[test]
    fn shuffle_is_a_permutation_and_deterministic() {
        let mut a: Vec<usize> = (0..50).collect();
        let mut b: Vec<usize> = (0..50).collect();
        SplitMix64::new(123).shuffle(&mut a);
        SplitMix64::new(123).shuffle(&mut b);
        assert_eq!(a, b, "same seed must give the same permutation");

        let mut sorted = a.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..50).collect::<Vec<_>>(), "must be a permutation");
        assert_ne!(a, (0..50).collect::<Vec<_>>(), "must actually shuffle");
    }
}
