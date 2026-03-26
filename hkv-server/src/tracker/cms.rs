use std::num::Wrapping;

#[derive(Debug, Clone)]
pub(crate) struct CountMinSketch {
    width: usize,
    depth: usize,
    rows: Vec<Vec<u64>>,
}

impl CountMinSketch {
    pub(crate) fn new(width: usize, depth: usize) -> Self {
        assert!(width > 0, "cms width must be positive");
        assert!(depth > 0, "cms depth must be positive");

        Self {
            width,
            depth,
            rows: vec![vec![0; width]; depth],
        }
    }

    pub(crate) fn increment(&mut self, key: &[u8]) {
        for row in 0..self.depth {
            let idx = self.index_for(row, key);
            self.rows[row][idx] = self.rows[row][idx].saturating_add(1);
        }
    }

    pub(crate) fn estimate(&self, key: &[u8]) -> u64 {
        self.rows
            .iter()
            .enumerate()
            .map(|(row, values)| values[self.index_for(row, key)])
            .min()
            .unwrap_or(0)
    }

    pub(crate) fn reset(&mut self) {
        for row in &mut self.rows {
            row.fill(0);
        }
    }

    fn index_for(&self, row: usize, key: &[u8]) -> usize {
        (stable_hash(row as u64, key) % self.width as u64) as usize
    }
}

fn stable_hash(seed: u64, data: &[u8]) -> u64 {
    let mut hash = Wrapping(0xcbf29ce484222325u64 ^ seed);
    for byte in data {
        hash ^= Wrapping(*byte as u64);
        hash *= Wrapping(0x100000001b3);
    }
    hash.0
}

#[cfg(test)]
mod tests {
    use super::CountMinSketch;

    #[test]
    fn increment_and_estimate_single_key() {
        let mut sketch = CountMinSketch::new(32, 4);

        sketch.increment(b"alpha");
        sketch.increment(b"alpha");

        assert_eq!(sketch.estimate(b"alpha"), 2);
    }

    #[test]
    fn estimate_never_decreases_for_same_key_before_reset() {
        let mut sketch = CountMinSketch::new(32, 4);

        sketch.increment(b"alpha");
        let first = sketch.estimate(b"alpha");
        sketch.increment(b"alpha");
        let second = sketch.estimate(b"alpha");

        assert!(second >= first);
    }

    #[test]
    fn unrelated_keys_do_not_produce_broken_estimates() {
        let mut sketch = CountMinSketch::new(256, 4);

        for _ in 0..5 {
            sketch.increment(b"alpha");
        }
        sketch.increment(b"beta");

        assert!(sketch.estimate(b"alpha") >= 5);
        assert!(sketch.estimate(b"beta") >= 1);
        assert!(sketch.estimate(b"alpha") >= sketch.estimate(b"beta"));
    }

    #[test]
    fn reset_clears_current_state() {
        let mut sketch = CountMinSketch::new(32, 4);
        sketch.increment(b"alpha");
        sketch.increment(b"beta");

        sketch.reset();

        assert_eq!(sketch.estimate(b"alpha"), 0);
        assert_eq!(sketch.estimate(b"beta"), 0);
    }

    #[test]
    #[should_panic(expected = "cms width must be positive")]
    fn constructor_rejects_zero_width() {
        let _ = CountMinSketch::new(0, 4);
    }

    #[test]
    #[should_panic(expected = "cms depth must be positive")]
    fn constructor_rejects_zero_depth() {
        let _ = CountMinSketch::new(32, 0);
    }

    #[test]
    fn counters_saturate_at_u64_max() {
        let mut sketch = CountMinSketch::new(1, 1);
        sketch.rows[0][0] = u64::MAX;

        sketch.increment(b"alpha");

        assert_eq!(sketch.estimate(b"alpha"), u64::MAX);
    }

    #[test]
    fn collisions_only_overestimate() {
        let mut sketch = CountMinSketch::new(1, 2);
        sketch.increment(b"alpha");
        sketch.increment(b"beta");

        assert!(sketch.estimate(b"alpha") >= 1);
        assert!(sketch.estimate(b"beta") >= 1);
    }
}
