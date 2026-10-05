//! A small deterministic generator, so the corpus is a fixed artefact.
//!
//! # Why not a dependency
//!
//! `rand` would be a distributed-graph dependency requiring a licence record
//! (docs/THIRD-PARTY-POLICY.md) for about forty lines of arithmetic used only
//! to lay out benchmark fixtures. Nothing here is cryptographic and nothing
//! depends on statistical quality beyond "spreads values without an obvious
//! pattern": the requirement is REPRODUCIBILITY, which a fixed algorithm with
//! a fixed seed gives and a library upgrade could silently take away.
//!
//! PCG-XSH-RR 64/32 (O'Neill, 2014), published in "PCG: A Family of Simple
//! Fast Space-Efficient Statistically Good Algorithms for Random Number
//! Generation", HMC-CS-2014-0905. Implemented from the published algorithm
//! description, not adapted from source (docs/CLEAN-ROOM-POLICY.md).

/// A 64-bit LCG with an output permutation. Deterministic for a given seed.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
    increment: u64,
}

const MULTIPLIER: u64 = 6_364_136_223_846_793_005;

impl Rng {
    /// Seeds the generator. The same seed always yields the same sequence.
    pub fn new(seed: u64) -> Self {
        // The increment must be odd for the LCG to have full period.
        let mut rng = Self {
            state: 0,
            increment: (seed << 1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }

    /// The next 32 bits.
    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(MULTIPLIER).wrapping_add(self.increment);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// A value in `0..n`. `n` must be non-zero.
    ///
    /// Uses rejection sampling rather than a modulo, so the distribution is
    /// exactly uniform and a corpus does not inherit a bias from its size.
    pub fn below(&mut self, n: u32) -> u32 {
        assert!(n > 0, "below(0) has no value to return");
        let threshold = n.wrapping_neg() % n;
        loop {
            let v = self.next_u32();
            if v >= threshold {
                return v % n;
            }
        }
    }

    /// True with probability `numerator / denominator`.
    pub fn chance(&mut self, numerator: u32, denominator: u32) -> bool {
        self.below(denominator) < numerator
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_gives_the_same_sequence() {
        let a: Vec<u32> = (0..64).map(|_| Rng::new(7).next_u32()).collect();
        let mut r = Rng::new(7);
        let b: Vec<u32> = (0..64).map(|_| r.next_u32()).collect();
        assert_eq!(a[0], b[0]);

        let mut x = Rng::new(1234);
        let mut y = Rng::new(1234);
        for _ in 0..1000 {
            assert_eq!(x.next_u32(), y.next_u32());
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut x = Rng::new(1);
        let mut y = Rng::new(2);
        assert_ne!(
            (0..8).map(|_| x.next_u32()).collect::<Vec<_>>(),
            (0..8).map(|_| y.next_u32()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn below_stays_in_range_and_covers_it() {
        let mut r = Rng::new(99);
        let mut seen = [false; 5];
        for _ in 0..1000 {
            let v = r.below(5);
            assert!(v < 5);
            seen[v as usize] = true;
        }
        assert!(seen.iter().all(|&s| s), "every value in 0..5 occurs");
    }
}
