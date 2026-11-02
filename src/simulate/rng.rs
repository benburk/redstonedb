//! The simple generator a test draws its sampled cases from, so every build
//! of a kind is tried, timed, and measured on the same cases, run after run.

/// A xorshift generator from one fixed seed: not random, only spread out
/// enough that its numbers cover every bit.
pub(super) struct Rng(u64);

impl Default for Rng {
    fn default() -> Self {
        Rng(0x9e37_79b9_7f4a_7c15)
    }
}

impl Rng {
    /// The next number, every bit of it drawn.
    pub fn draw(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// The next number below `n`, to pick one of `n` things; `n` is not 0.
    pub fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.draw() % n as u64).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stream_is_the_same_every_run() {
        let mut rng = Rng::default();
        let first: Vec<u64> = (0..3).map(|_| rng.draw()).collect();
        assert_eq!(
            first,
            [
                0xdc1b_77ae_0bf3_4dad,
                0x64f0_eeb9_026e_6076,
                0x7b07_ce91_e590_6136
            ]
        );
    }
}
