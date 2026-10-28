//! Port of the RT3Lite code generator (`rt3_generator.c`) recovered from
//! `DecoderRing.app`. Function names mirror the original `RG_*` symbols.

use crate::rt3::{self, Rng};

const BLOCKS: usize = 6;
const OPS_PER_BLOCK: usize = 0x90; // generated
const OPS_EXECUTED: usize = 0x80; // only the first 128 are ever run

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Op {
    pub kind: i32,
    pub a: i32,
    pub b: i32,
}

/// The per-product "algorithm": six blocks of byte-mangling operations
/// derived deterministically from the product name (`RG_GenerateAlgorithm`).
#[derive(Debug, Clone)]
pub struct Algorithm {
    pub blocks: Vec<[Op; OPS_PER_BLOCK]>,
}

/// `_RG_GenerateAlgorithm(product, product, depth)`
///
/// `depth` is the number of indented sub-items between the selected menu
/// entry and its parent product (e.g. "Snapz Pro X / w/ Movie Capture 2.x" = 2).
pub fn generate_algorithm(product: &[u8], depth: i32) -> Algorithm {
    let mut seed = rt3::scramble_seed(product);
    let mut k = depth;
    loop {
        k -= 1;
        if k == -1 {
            break;
        }
        let u = ((0x10 - k) as u32).wrapping_mul(seed);
        seed ^= u.rotate_right((k & 0x1f) as u32);
    }

    let mut blocks = Vec::with_capacity(BLOCKS);
    for i in 0..BLOCKS as u32 {
        let mut rng = Rng::new((seed >> 5).wrapping_mul(i * 0x11 + 0x11));
        let mut ops = [Op::default(); OPS_PER_BLOCK];
        for (j, op) in ops.iter_mut().enumerate() {
            let kind = if rng.number(0, 100) < 0x29 && j > 3 {
                if rng.number(0, 100) < 0x1f {
                    if rng.number(0, 100) < 0x33 { 5 } else { 4 }
                } else if rng.number(0, 100) < 0x33 {
                    1
                } else {
                    0
                }
            } else if rng.number(0, 100) < 0x29 {
                3
            } else {
                2
            };
            let (a, b) = match kind {
                0 | 1 => (rng.number(0, 8), 0),
                2 => {
                    let a = rng.number(0, 3);
                    (a, if a != 2 { rng.number(0, 0x10) } else { 0 })
                }
                3 => (rng.number(-0x7f, 0x7f), 0),
                4 => {
                    let a = rng.number(0, 6);
                    let b = if a == 0 || a == 1 { rng.number(10, 0xf5) } else { rng.number(0, 0x10) };
                    (a, b)
                }
                5 => (rng.number(0, 10), 0),
                _ => unreachable!(),
            };
            *op = Op { kind, a, b };
        }
        blocks.push(ops);
    }
    Algorithm { blocks }
}

fn idx(b: i32, len: usize) -> usize {
    if len == 0 { 0 } else { (b % len as i32) as usize }
}

/// `__RG_RunAlgorithmBlock` — a tiny conditional byte VM.
pub fn run_block(alg: &Algorithm, block: usize, product: &[u8], name: &[u8], copies: i32) -> u8 {
    assert!(copies != 0, "copies must be non-zero");
    let plen = product.len();
    let nlen = name.len();
    let pc = |i: usize| product.get(i).copied().unwrap_or(0) as i8 as i32;
    let nc = |i: usize| name.get(i).copied().unwrap_or(0) as i8 as i32;

    let mut active = true;
    let mut x: u8 = 0;
    for op in &alg.blocks[block][..OPS_EXECUTED] {
        let n = op.a as u32;
        match op.kind {
            0 => {
                if active {
                    x = ((x as u32) << (n & 31) | (x as u32) >> ((8 - n) & 31)) as u8;
                }
                active = true;
            }
            1 => {
                if active {
                    x = ((x as u32) << ((8 - n) & 31) | (x as u32) >> (n & 31)) as u8;
                }
                active = true;
            }
            2 => {
                if active {
                    match op.a {
                        1 if nlen != 0 => x ^= name[idx(op.b, nlen)],
                        2 => x ^= copies as u8,
                        0 if plen != 0 => x ^= product[idx(op.b, plen)],
                        _ => {}
                    }
                }
                active = true;
            }
            3 => {
                if active {
                    x = x.wrapping_add(op.a as u8);
                }
                active = true;
            }
            4 => {
                let xi = x as i32;
                let cond = active
                    && match op.a {
                        0 => xi > op.b,
                        1 => xi < op.b,
                        2 => xi > pc(idx(op.b, plen)),
                        3 => xi < pc(idx(op.b, plen)),
                        4 => xi > nc(idx(op.b, nlen)),
                        5 => xi < nc(idx(op.b, nlen)),
                        _ => false,
                    };
                if !cond {
                    active = false;
                }
            }
            5 => active = true,
            _ => {}
        }
    }
    rt3::avoid_small(x)
}

/// `__RG_RunAlgorithm` — produces the three 16-bit code blocks.
pub fn run_algorithm(alg: &Algorithm, product: &[u8], name: &[u8], copies: i32) -> [u16; 3] {
    let mut out = [0u16; 3];
    for (i, v) in out.iter_mut().enumerate() {
        let a = run_block(alg, 2 * i, product, name, copies) as u16;
        let b = run_block(alg, 2 * i + 1, product, name, copies) as u16;
        *v = a.wrapping_mul(b);
    }
    let mut rng = Rng::new(rt3::scramble_seed(name).wrapping_mul(0x25));
    for v in out.iter_mut() {
        if *v < 0x100 {
            *v = v.wrapping_mul((rng.number(3, 0xff) as u16) | 1);
        }
        if *v < 0x1111 {
            *v = v.wrapping_mul((rng.number(3, 0xf) as u16) | 1);
        }
    }
    out
}

/// `_RG_GenerateLicenseCode` — returns the raw 60-bit code.
pub fn generate_license_code(alg: &Algorithm, product: &[u8], licensee: &[u8], copies: i32, period: u8) -> u64 {
    let name = rt3::fix_license_name(licensee, false);
    let [a, b, c] = run_algorithm(alg, product, &name, copies);
    rt3::enswizzle(a, b, c, period) ^ rt3::main_hash(product, &name, copies)
}

/// `_RG_GenerateRetailCodes` (single code, step 1) — for 6-digit retail IDs.
pub fn generate_retail_code(alg: &Algorithm, product: &[u8], retail_id: u32) -> u64 {
    assert!(retail_id <= 999_999);
    let hash = rt3::retail_hash(product) as u32;
    let id = format!("{:06}", retail_id);
    let [a, b, c] = run_algorithm(alg, product, id.as_bytes(), 1);
    rt3::enswizzle(a, b, c, (retail_id % hash) as u8) ^ rt3::main_hash(product, id.as_bytes(), 1)
}

/// Unnamed validator @ 0x100006c02 (the inverse check used by RT3 clients).
/// Returns `Some(timestamp_byte)` if the code is structurally valid for the
/// given product/name/copies.
pub fn validate_license_code(alg: &Algorithm, product: &[u8], licensee: &[u8], copies: i32, code: u64) -> Option<u8> {
    validate_with_name(alg, product, &rt3::fix_license_name(licensee, false), copies, code)
}

fn validate_with_name(alg: &Algorithm, product: &[u8], name: &[u8], copies: i32, code: u64) -> Option<u8> {
    let raw = code ^ rt3::main_hash(product, name, copies);
    let (b1, b2, b3, ts) = rt3::deswizzle(raw)?;
    for (i, v) in [b1, b2, b3].into_iter().enumerate() {
        let x = run_block(alg, 2 * i, product, name, copies) as u32;
        let y = run_block(alg, 2 * i + 1, product, name, copies) as u32;
        if v as u32 % (x * y) != 0 {
            return None;
        }
    }
    Some(ts)
}

/// Validates a retail code (name is the zero-padded 6-digit ID, copies = 1).
pub fn validate_retail_code(alg: &Algorithm, product: &[u8], retail_id: u32, code: u64) -> bool {
    let id = format!("{:06}", retail_id);
    validate_with_name(alg, product, id.as_bytes(), 1, code)
        .map(|ts| ts as u32 == retail_id % rt3::retail_hash(product) as u32)
        .unwrap_or(false)
}

/// `__RG_OldStyleChecksum`
fn old_style_checksum(mut sum: u32, s: &[u8], copies: u16, style: u8) -> u32 {
    for &c in s {
        let b = (sum as u8).wrapping_add(c) as u32;
        sum = (sum & 0xffff_ff00) | b;
        match style {
            1 => {
                sum = sum.rotate_right(27);
                sum = (sum & 0xffff_0000) | ((sum as u16).wrapping_add(copies) as u32);
            }
            2 => {
                sum = sum.rotate_right(26);
                sum = (sum & 0xffff_0000) | ((sum as u16).wrapping_add(copies) as u32);
                sum = (sum ^ 0xdead_beef).rotate_right(1);
            }
            _ => unreachable!(),
        }
    }
    sum
}

/// `_RG_GenerateOldStyleCode` — 8 letters `A`..`P`.
/// `style` 1 = classic (no suffix in the menu), 2 = "!" products.
pub fn generate_old_style_code(product: &[u8], licensee: &[u8], copies: u16, style: u8) -> String {
    assert!(style == 1 || style == 2);
    assert!(copies != 0);
    let upper: Vec<u8> = licensee.iter().take(255).map(|c| c.to_ascii_uppercase()).collect();
    let mut sum = old_style_checksum(0, &upper, copies, style);
    sum = old_style_checksum(sum, product, copies, style);
    let mut out = String::with_capacity(8);
    for _ in 0..8 {
        out.push((b'A' + (sum & 0xf) as u8) as char);
        sum = sum.rotate_right(4);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rt3;

    #[test]
    fn generated_codes_validate() {
        for (product, depth) in [("EV Nova", 0), ("Snapz Pro X", 1), ("Snapz Pro X", 2), ("WireTap Studio", 0)] {
            let alg = generate_algorithm(product.as_bytes(), depth);
            for (name, copies) in [("John Doe", 1), ("A. N. Other", 5), ("x", 300)] {
                for period in [0u8, 7, 162, 255] {
                    let code = generate_license_code(&alg, product.as_bytes(), name.as_bytes(), copies, period);
                    let parsed = rt3::text_to_code(&rt3::code_to_text(code));
                    assert_eq!(parsed, code);
                    assert_eq!(
                        validate_license_code(&alg, product.as_bytes(), name.as_bytes(), copies, parsed),
                        Some(period)
                    );
                    // wrong name / copies / product must fail
                    assert_eq!(validate_license_code(&alg, product.as_bytes(), b"Someone Else", copies, parsed), None);
                    assert_eq!(validate_license_code(&alg, product.as_bytes(), name.as_bytes(), copies + 1, parsed), None);
                }
            }
        }
    }

    #[test]
    fn retail_codes_validate() {
        let alg = generate_algorithm(b"EV Nova", 0);
        for id in [0u32, 1, 123456, 999999] {
            let code = generate_retail_code(&alg, b"EV Nova", id);
            assert!(validate_retail_code(&alg, b"EV Nova", id, code));
            assert!(!validate_retail_code(&alg, b"EV Nova", (id + 1) % 1_000_000, code));
        }
    }

    #[test]
    fn depth_changes_algorithm() {
        let a0 = generate_algorithm(b"Snapz Pro X", 0);
        let a1 = generate_algorithm(b"Snapz Pro X", 1);
        assert_ne!(a0.blocks[0], a1.blocks[0]);
    }

    #[test]
    fn old_style_shape() {
        let c = generate_old_style_code(b"Escape Velocity", b"John", 1, 1);
        assert_eq!(c.len(), 8);
        assert!(c.bytes().all(|b| (b'A'..=b'P').contains(&b)));
        assert_ne!(c, generate_old_style_code(b"Escape Velocity", b"John", 1, 2));
        assert_eq!(c, generate_old_style_code(b"Escape Velocity", b"john", 1, 1));
    }
}
