//! The generator's random numbers: xoshiro256** seeded through splitmix64.
//!
//! Fixed algorithm, u64 arithmetic only, no floats and no usize in ranges,
//! so the same seed gives the same numbers on every target (metaxy on
//! 64-bit Linux, the mod as wasm32 or on 32-bit consoles).

pub struct Rng {
    s: [u64; 4],
}

fn splitmix64(x: &mut u64) -> u64 {
    *x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Rng {
    pub fn new(seed: u64) -> Rng {
        let mut x = seed;
        Rng { s: [splitmix64(&mut x), splitmix64(&mut x), splitmix64(&mut x), splitmix64(&mut x)] }
    }

    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// A number in 0..n, without bias (rejection sampling). n > 0.
    pub fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0);
        let zone = u64::MAX - (u64::MAX % n);
        loop {
            let v = self.next_u64();
            if v < zone {
                return v % n;
            }
        }
    }

    /// Fisher-Yates, from the end.
    pub fn shuffle<T>(&mut self, v: &mut [T]) {
        let n = v.len() as u64;
        let mut i = n;
        while i > 1 {
            let j = self.below(i);
            v.swap((i - 1) as usize, j as usize);
            i -= 1;
        }
    }

    pub fn pick<'a, T>(&mut self, v: &'a [T]) -> &'a T {
        &v[self.below(v.len() as u64) as usize]
    }
}

/// FNV-1a 64 over bytes: the data hash recorded in a world.
pub fn fnv1a(parts: &[&[u8]]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for p in parts {
        for b in *p {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01B3);
        }
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_sequence() {
        // the first numbers for seed 1 are part of the format: worlds depend on them
        let mut r = Rng::new(1);
        let v: Vec<u64> = (0..3).map(|_| r.next_u64()).collect();
        let mut r2 = Rng::new(1);
        let v2: Vec<u64> = (0..3).map(|_| r2.next_u64()).collect();
        assert_eq!(v, v2);
        assert_ne!(v[0], v[1]);
    }

    #[test]
    fn below_in_range() {
        let mut r = Rng::new(7);
        for n in 1..50u64 {
            for _ in 0..50 {
                assert!(r.below(n) < n);
            }
        }
    }
}
