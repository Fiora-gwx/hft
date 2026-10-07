//! Reference xorshift64* mask-key sequence (README §2 step 1): state ^= state >> 12;
//! state ^= state << 25; state ^= state >> 27; key = high 32 bits of state × 0x2545F4914F6CDD1D.

pub struct XorShift64Star(pub u64);

impl XorShift64Star {
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }
}
