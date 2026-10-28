# decoder-ring

A Rust command-line port of **DecoderRing.app** (Ambrosia Software, v1.0.0,
SVN r15488, July 2011), the in-house tool Ambrosia used to issue registration
codes for its Mac products. The original is a Cocoa app (`ASWRegDecoderRingWC`)
that wraps the **RT3Lite** licensing library (`rt3_generator.c`, `rt3_license.c`).

The port was produced by decompiling the x86_64 slice of the original binary
with `rea` (Ghidra 12.1.4 backend), then checked against the original machine
code under emulation (see [Verification](#verification)).

## Usage

```sh
cargo build --release

decoder-ring list
decoder-ring generate -p "EV Nova" -n "Jane Doe" -c 1
decoder-ring generate -p "EV Nova" -n 123456             # 6-digit retail ID on a ** product
decoder-ring generate -p "Snapz Pro X w/ Movie Capture 2.x" -n "Jane Doe"
decoder-ring generate -p "Escape Velocity" -n "Jane Doe" # old-style 8-letter code
decoder-ring generate -p "EV Nova" -n "Jane Doe" --now 1300000000   # fixed clock
decoder-ring verify   -p "EV Nova" -n "Jane Doe" -k XXXX-XXXX-XXXX
decoder-ring generate --custom "Some Product" --scheme rt3 --depth 0 -n "Jane Doe"
```

`verify` exits 0 for valid, 1 for invalid, 2 on error.

## What was recovered

### Product menu (from `Decoder.nib`)

52 menu entries. The suffix on each title chooses the scheme
(`-[ASWRegDecoderRingWC generateCode:]`):

| Suffix | Scheme | Notes |
|---|---|---|
| none | old-style v1 | 8 letters `A`–`P` |
| `!` | old-style v2 | same as v1, with a different checksum |
| `*` | RT3 | `XXXX-XXXX-XXXX`, includes a 2-week time-period byte |
| `**` | RT3 + retail | as above; a licensee made up of 6 digits gets a non-expiring retail code |
| `?` | external | DEFCON: the app generates no code |

A title starting with a space is a sub-item (Snapz Pro X "w/ Movie Capture
1.x/2.x"). The app walks up the menu to the parent title, counting the steps,
and passes that count as `depth` to `RG_GenerateAlgorithm`, which gives each
sub-item its own algorithm.

### RT3 code pipeline

1. **`RG_GenerateAlgorithm(product, depth)`**: seeds a Park–Miller (16807) PRNG
   (`__RT3_RandomNumber`) from a rotate/XOR hash of the product name and builds
   6 blocks of 144 random byte operations (rotate, XOR with a name, product or
   copies byte, add, conditional skip, reset).
2. **`RT3_FixLicenseName`**: keeps only the ASCII letters of the name and
   upper-cases them.
3. **`__RG_RunAlgorithm`**: runs the first 128 operations of each block as a
   small byte VM. Each pair of blocks is multiplied into one 16-bit value, so
   there are three values. Small values are scaled by odd random factors so
   that a validator can check them by divisibility.
4. **`RT3_LicenseCodeEnswizzle`**: interleaves the three 16-bit values with the
   8-bit time period (`(t − 2000-12-25) / 14 days`), then adds a 3-bit checksum
   and a parity bit, for 60 bits in total.
5. XOR with **`RT3_CalcMainHash(product, name, copies)`**, then encode with the
   32-symbol alphabet `23456789ABCDEFGHJKLMNPQRSTUVWXYZ`.

`verify` ports the validator at `0x100006c02`: deswizzle the code, check the
checksum, and check that each block is divisible by its VM product.

### Old-style codes

`RG_GenerateOldStyleCode` checksums the upper-cased licensee and then the
product name, using a byte-add, a rotate and a 16-bit add of the copy count
(v2 also XORs with `0xDEADBEEF` and rotates once more). It outputs 8 nibbles as
`A`–`P`.

### Faithfully reproduced quirks

- **Expiry date wraps.** The time period is stored in one byte, so it wrapped
  around in 2010. Any code generated today shows an expiry date in the 2000s,
  for example 2007. The original app has the same behaviour, and the expiry
  date it displays is only informational.
- Strings are converted to **Mac OS Roman**, as `cStringFromString:` does. A
  name that can't be represented in Mac OS Roman is rejected.
- `RT3_FixLicenseName` has a branch that keeps a 6-digit retail ID. It only
  runs when the global product version is `0x8003`, and DecoderRing never sets
  that value, so the branch is unreachable. It is kept behind a flag that is
  never set.

### Not ported

These parts are GUI-only or have side effects:

- the e-mail template text and the pasteboard;
- the "movable" checkbox, which writes a licence into the local RT3 prefs (`RT3_Open` / `RT3_SetLicenseInfo`);
- the about box.

## Verification

- `cargo test`: unit tests for the swizzle round-trip, text encoding, the
  validator accepting generated codes and rejecting a wrong name, copy count or
  retail ID, and product resolution.
- `tools/differential_test.py`: loads the original x86_64 Mach-O into
  **Unicorn**, stubs `malloc`, `strcpy`, `sprintf` and the other imports it
  uses, and calls the original `_RG_GenerateAlgorithm`,
  `_RG_GenerateLicenseCode`, `_RG_GenerateRetailCodes`,
  `_RG_GenerateOldStyleCode` and `_RT3_CalcTimePeriod`. It then compares their
  output with the CLI's for every menu product, several names, copy counts and
  timestamps, and random retail IDs. **Result: 444/444 identical.** An
  additional run with non-ASCII Mac OS Roman product names and licensee names
  also matched (16/16).

```sh
pip install unicorn lief
lipo -thin x86_64 DecoderRing.app/Contents/MacOS/DecoderRing -output DecoderRing.x86_64
python3 tools/differential_test.py DecoderRing.x86_64 target/release/decoder-ring src/products.rs
```
