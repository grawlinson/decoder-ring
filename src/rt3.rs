//! Port of the RT3Lite licence primitives (`rt3_license.c` / `rt3_utilities.c`)
//! recovered from `DecoderRing.app` (x86_64 slice).
//!
//! Every function notes the original symbol it was decompiled from.

/// `_gRT3_FiveBitToEightBitChars` @ 0x1000142a0 — 32-symbol alphabet
/// (no 0, 1, I or O).
pub const FIVE_BIT_CHARS: &[u8; 32] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";

const CODE_MASK: u64 = 0x0fff_ffff_ffff_ffff; // 60 bits
const SMALL_VALUE_TABLE: [u8; 3] = [0x49, 0x71, 0xad];

/// `__RT3_RandomSeed` / `__RT3_RandomNumber` — Park–Miller "minimal standard"
/// generator (16807) implemented with 16-bit halves, exactly as the original.
#[derive(Debug, Clone)]
pub struct Rng {
    pub seed: u32,
}

impl Rng {
    pub fn new(seed: u32) -> Self {
        Rng { seed }
    }

    fn step(&mut self) -> u32 {
        let s = self.seed;
        let lo = (s & 0xffff).wrapping_mul(0x41a7);
        let hi = (s >> 16).wrapping_mul(0x41a7);
        let t = (lo >> 16).wrapping_add(hi);
        let edx = (t.wrapping_add(t) >> 16).wrapping_add((t & 0x7fff) << 16);
        let new = (lo & 0xffff).wrapping_add(edx).wrapping_sub(0x7fff_ffff);
        self.seed = new;
        if new & 0xffff == 0x8000 {
            0
        } else {
            new & 0xffff
        }
    }

    /// Returns a value in `[min, max)` (max is exclusive in practice).
    pub fn number(&mut self, min: i32, max: i32) -> i32 {
        let range = (max as u32).wrapping_sub(min as u32);
        let iters = if range < 0x10000 { 1 } else { 2 };
        let mut acc: u32 = 0;
        for _ in 0..iters {
            acc = (acc << 16) | self.step();
        }
        let r = if range < 0x10000 {
            range.wrapping_mul(acc) >> 16
        } else {
            acc.wrapping_mul(range)
        };
        (min as u32).wrapping_add(r) as i32
    }
}

/// `__RG_CalcScrambleSeed`
pub fn scramble_seed(s: &[u8]) -> u32 {
    s.iter()
        .fold(0u32, |acc, &c| acc.rotate_right(28) ^ (c as i8 as i32 as u32))
}

/// `__RG_CalcRetailHash`
pub fn retail_hash(product: &[u8]) -> u8 {
    let h = scramble_seed(product) as u8;
    if h < 3 {
        SMALL_VALUE_TABLE[h as usize]
    } else if h < 100 {
        !h
    } else {
        h
    }
}

/// Maps a byte that is < 3 to the fixed table, otherwise returns it.
pub fn avoid_small(b: u8) -> u8 {
    if b < 3 {
        SMALL_VALUE_TABLE[b as usize]
    } else {
        b
    }
}

/// `_RT3_FixLicenseName` — keeps only ASCII letters, upper-cased, max 255.
///
/// The original has a branch that keeps a bare 6-digit retail ID when the
/// global product version equals 0x8003; DecoderRing never sets that value
/// (it passes 1/2/3/999 to `RT3_Open`), so that branch is reproduced behind
/// `allow_retail_digits` for completeness only.
pub fn fix_license_name(name: &[u8], allow_retail_digits: bool) -> Vec<u8> {
    if allow_retail_digits {
        if let Some(d) = retail_digits(name) {
            return d;
        }
    }
    let mut out = Vec::new();
    for &c in name {
        if out.len() >= 255 {
            break;
        }
        if c.is_ascii_lowercase() {
            out.push(c - 0x20);
        } else if c.is_ascii_uppercase() {
            out.push(c);
        }
    }
    out
}

/// `_IsRetailCode` — returns the digits if the name is exactly six digits
/// (any non-letter separators are ignored, any letter disqualifies it).
pub fn retail_digits(name: &[u8]) -> Option<Vec<u8>> {
    let mut digits = Vec::new();
    for &c in name {
        if c.is_ascii_alphabetic() || digits.len() >= 0xff {
            return None;
        }
        if c.is_ascii_digit() {
            digits.push(c);
        }
    }
    (digits.len() == 6).then_some(digits)
}

/// `_RT3_CalcMainHash`
pub fn main_hash(product: &[u8], name: &[u8], copies: i32) -> u64 {
    let copies = copies as u32;
    let mut h: u64 = 0;
    for (i, &c) in product.iter().enumerate() {
        h = h.rotate_left(4) ^ (c as i8 as i64 as u64);
        h = h.rotate_left(1) ^ ((i as u32).wrapping_add(7).wrapping_mul(copies) as u64);
    }
    let mut k: u32 = 0x27;
    for &c in name {
        h = h.rotate_left(3) ^ (c as i8 as i64 as u64);
        h = h.rotate_left(1) ^ (k.wrapping_mul(copies) as u64);
        k = k.wrapping_add(3);
    }
    if h == 0 {
        h = 1;
    }
    h & CODE_MASK
}

fn checksum3(v: u64) -> u32 {
    (0..57).step_by(3).map(|s| ((v >> s) & 7) as u32).sum()
}

/// `_RT3_LicenseCodeEnswizzle` — interleaves three 16-bit blocks and an
/// 8-bit timestamp into a 60-bit code with a 3-bit checksum and parity bit.
pub fn enswizzle(a: u16, b: u16, c: u16, p: u8) -> u64 {
    let (a, b, c, p) = (a as u64, b as u64, c as u64, p as u64);
    let n = |x: u64, s: u32| (x >> s) & 0xf;
    let bit = |s: u32| (p >> s) & 1;
    let mut v = n(a, 12);
    v = (v << 1) | bit(0);
    v = (v << 4) | n(b, 8);
    v = (v << 1) | bit(1);
    v = (v << 4) | n(c, 4);
    v = (v << 4) | n(a, 0);
    v = (v << 1) | bit(2);
    v = (v << 4) | n(b, 12);
    v = (v << 1) | bit(3);
    v = (v << 4) | n(c, 8);
    v = (v << 4) | n(a, 4);
    v = (v << 1) | bit(4);
    v = (v << 4) | n(b, 0);
    v = (v << 1) | bit(5);
    v = (v << 4) | n(c, 12);
    v = (v << 4) | n(a, 8);
    v = (v << 1) | bit(6);
    v = (v << 4) | n(b, 4);
    v = (v << 1) | bit(7);
    v = (v << 4) | n(c, 0);

    let sum = checksum3(v);
    let shifted = v << 3;
    let out = shifted | (sum & 7) as u64;
    let parity_src = ((shifted >> 32) as u32) ^ (out as u32);
    if parity_src.count_ones() % 2 == 0 {
        out | 0x0800_0000_0000_0000
    } else {
        out
    }
}

/// `_RT3_LicenseCodeDeswizzle` — inverse of [`enswizzle`]. Returns `None`
/// when the embedded checksum does not match.
pub fn deswizzle(code: u64) -> Option<(u16, u16, u16, u8)> {
    let v = code & 0x07ff_ffff_ffff_ffff;
    if (code & 7) as u32 != checksum3(v >> 3) & 7 {
        return None;
    }
    Some((
        block1(code),
        block2(code),
        block3(code),
        timestamp(code),
    ))
}

/// `_RT3_ExtractLicenseBlock1`
pub fn block1(c: u64) -> u16 {
    let hi = (c >> 32) as u32;
    ((((c >> 13) as u32 & 0xf) << 8)
        | (((c >> 27) as u32 & 0xf) << 4)
        | (hi >> 9 & 0xf)
        | ((hi >> 23 & 0xf) << 12)) as u16
}

/// `_RT3_ExtractLicenseBlock2`
pub fn block2(c: u64) -> u16 {
    let hi = (c >> 32) as u32;
    ((((c >> 8) as u32 & 0xf) << 4)
        | ((c >> 22) as u32 & 0xf)
        | ((hi >> 4 & 0xf) << 12)
        | ((hi >> 18 & 0xf) << 8)) as u16
}

/// `_RT3_ExtractLicenseBlock3`
pub fn block3(c: u64) -> u16 {
    (((c >> 3) as u32 & 0xf)
        | (((c >> 17) as u32 & 0xf) << 12)
        | (((c >> 31) as u32 & 0xf) << 8)
        | (((c >> 45) as u32 & 0xf) << 4)) as u16
}

/// `_RT3_ExtractLicenseTimestamp`
pub fn timestamp(c: u64) -> u8 {
    let hi = (c >> 32) as u32;
    let lo = c as u32;
    (((lo >> 7 & 1) << 7)
        | ((lo >> 12 & 1) << 6)
        | ((lo >> 21 & 1) << 5)
        | ((lo >> 26 & 1) << 4)
        | ((hi >> 3 & 1) << 3)
        | ((hi >> 8 & 1) << 2)
        | ((hi >> 17 & 1) << 1)
        | (hi >> 22 & 1)) as u8
}

/// `_RT3_LicenseCodeToText` — `XXXX-XXXX-XXXX`.
pub fn code_to_text(code: u64) -> String {
    let code = code & CODE_MASK;
    let mut s = String::with_capacity(14);
    for i in 0..12 {
        let shift = 55 - 5 * i;
        s.push(FIVE_BIT_CHARS[((code >> shift) & 0x1f) as usize] as char);
        if i % 4 == 3 && i < 11 {
            s.push('-');
        }
    }
    s
}

/// `_RT3_LicenseTextToCode` — ignores characters outside the alphabet,
/// case-insensitive, reads at most 12 symbols.
pub fn text_to_code(text: &str) -> u64 {
    let mut v: u64 = 0;
    let mut n = 0;
    for c in text.bytes() {
        if n >= 12 {
            break;
        }
        let c = c.to_ascii_uppercase();
        if let Some(idx) = FIVE_BIT_CHARS.iter().position(|&x| x == c) {
            v = (v << 5) | idx as u64;
            n += 1;
        }
    }
    v
}

/// `_RT3_CalcTimePeriod` — 14-day periods since 2000-12-25, truncated to a byte.
pub fn time_period(t: u32) -> u8 {
    if t > 0x3a50_0ecf {
        (t.wrapping_add(0xc5b9_2bb0) / 0x0012_7500) as u8
    } else {
        0
    }
}

/// Expiry timestamp shown by DecoderRing for a given period
/// (`period * 0x127500 + 0x3a74f8d0`, computed in 32 bits).
pub fn period_expiry(period: u8) -> u32 {
    (period as u32).wrapping_mul(0x0012_7500).wrapping_add(0x3a74_f8d0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swizzle_roundtrip() {
        for &(a, b, c, p) in &[
            (0u16, 0u16, 0u16, 0u8),
            (0xffff, 0xffff, 0xffff, 0xff),
            (0x1234, 0xabcd, 0x5a5a, 0x81),
            (0x0f0f, 0xf0f0, 0x1111, 0x42),
        ] {
            let code = enswizzle(a, b, c, p);
            assert!(code <= CODE_MASK);
            assert_eq!(deswizzle(code), Some((a, b, c, p)));
        }
    }

    #[test]
    fn text_roundtrip() {
        let code = enswizzle(0x1234, 0xabcd, 0x5a5a, 0x81);
        let t = code_to_text(code);
        assert_eq!(t.len(), 14);
        assert_eq!(text_to_code(&t), code);
        assert_eq!(text_to_code(&t.to_lowercase()), code);
    }

    #[test]
    fn fix_name() {
        assert_eq!(fix_license_name(b"John Q. Smith 3rd", false), b"JOHNQSMITHRD");
        assert_eq!(retail_digits(b"123-456"), Some(b"123456".to_vec()));
        assert_eq!(retail_digits(b"12345"), None);
        assert_eq!(retail_digits(b"A123456"), None);
    }

    #[test]
    fn rng_range() {
        let mut r = Rng::new(12345);
        for _ in 0..1000 {
            let v = r.number(-127, 127);
            assert!((-127..127).contains(&v));
            let v = r.number(0, 100);
            assert!((0..100).contains(&v));
        }
    }
}
