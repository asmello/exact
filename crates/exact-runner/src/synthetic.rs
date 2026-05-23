// Synthetic cycle counts for QEMU dev mode.
//
// QEMU's `lm3s6965evb` machine doesn't model DWT, so `userlib::time_cycles`
// returns 0 on the device side. To exercise the leaderboard / scoring UI
// without real hardware, the runner fabricates deterministic cycle values
// per `(bin, case_input)` pair: siphash-2-4 keyed by the bin's SHA-256
// (first 16 bytes), seeded with a domain string and the case input.
//
// 5_000..55_000 range is wide enough to surface ordering differences
// across submissions but tight enough to feel "cycle-y."

use sha2::{Digest, Sha256};
use siphasher::sip::SipHasher24;
use std::hash::Hasher;

const DOMAIN: &[u8] = b"exact-synthetic";
const MIN_CYCLES: u64 = 5_000;
const SPREAD: u64 = 50_000;

pub fn synthetic_cycles(bin_sha: &[u8; 32], case_input: &[u8]) -> u64 {
    let k0 = u64::from_le_bytes(bin_sha[0..8].try_into().unwrap());
    let k1 = u64::from_le_bytes(bin_sha[8..16].try_into().unwrap());
    let mut h = SipHasher24::new_with_keys(k0, k1);
    h.write(DOMAIN);
    h.write(case_input);
    MIN_CYCLES + (h.finish() % SPREAD)
}

pub fn sha256(bin: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(bin);
    let out = h.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}
