/// The mulberry32 generator: a tiny, fast 32-bit PRNG whose output is pinned by the
/// engine version, unlike the `rand` generators. Matches the common JavaScript
/// implementation bit for bit.
#[derive(Debug, Clone)]
pub(super) struct Mulberry32 {
    state: u32,
}

impl Mulberry32 {
    pub(super) fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    /// The next number in `[0, 1)`, a multiple of 2^-32.
    pub(super) fn next_f64(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x6D2B_79F5);
        let mut t = self.state;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }

    /// A uniform index below `len`, which must not be zero.
    pub(super) fn index(&mut self, len: usize) -> usize {
        debug_assert!(len > 0, "an index needs a non-empty range");
        // next_f64 is below 1 by at least 2^-32, so the product floors to at most len - 1.
        (self.next_f64() * len as f64) as usize
    }
}
