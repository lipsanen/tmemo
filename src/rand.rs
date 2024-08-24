use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn from_seed(seed: u64) -> SplitMix64 {
        SplitMix64 { state: seed }
    }

    pub fn next_rand(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }

    pub fn next_float(&mut self, low: f64, high: f64) -> f64 {
        let frac = self.next_rand() as f64 / (2.0_f64).powf(64.0);
        frac * (high - low) + low
    }
}
