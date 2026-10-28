"""Differential tester: runs the ORIGINAL DecoderRing x86_64 generator code
under Unicorn and compares against the Rust CLI port."""
import random, struct, subprocess, sys
import lief
from unicorn import Uc, UC_ARCH_X86, UC_MODE_64, UC_HOOK_CODE
from unicorn.x86_const import *

BIN, CLI = sys.argv[1], sys.argv[2]
mb = lief.parse(BIN)
uc = Uc(UC_ARCH_X86, UC_MODE_64)
for seg in mb.segments:
    if seg.name == "__PAGEZERO" or seg.virtual_size == 0:
        continue
    uc.mem_map(seg.virtual_address, (seg.virtual_size + 0xfff) & ~0xfff)
    uc.mem_write(seg.virtual_address, bytes(seg.content))

HEAP, STACK, SENT = 0x200000000, 0x300000000, 0x400000000
uc.mem_map(HEAP, 0x4000000); uc.mem_map(STACK, 0x100000); uc.mem_map(SENT, 0x1000)
heap_ptr = [HEAP]

def cstr(a):
    out = b""
    while True:
        c = uc.mem_read(a, 1)
        if c == b"\0": return out
        out += bytes(c); a += 1

def alloc(n):
    p = heap_ptr[0]; heap_ptr[0] += (n + 15) & ~15; return p

R = lambda r: uc.reg_read(r)
def stub_ret(val=None):
    if val is not None: uc.reg_write(UC_X86_REG_RAX, val & 0xffffffffffffffff)
    rsp = R(UC_X86_REG_RSP)
    uc.reg_write(UC_X86_REG_RIP, struct.unpack("<Q", uc.mem_read(rsp, 8))[0])
    uc.reg_write(UC_X86_REG_RSP, rsp + 8)

def s_malloc(): stub_ret(alloc(R(UC_X86_REG_RDI)))
def s_free(): stub_ret(0)
def s_strcpy():
    d, s = R(UC_X86_REG_RDI), R(UC_X86_REG_RSI); uc.mem_write(d, cstr(s) + b"\0"); stub_ret(d)
def s_memset():
    d = R(UC_X86_REG_RDI); uc.mem_write(d, bytes([R(UC_X86_REG_RSI) & 0xff]) * R(UC_X86_REG_RDX)); stub_ret(d)
def s_memcpy():
    d = R(UC_X86_REG_RDI); uc.mem_write(d, bytes(uc.mem_read(R(UC_X86_REG_RSI), R(UC_X86_REG_RDX)))); stub_ret(d)
def s_sprintf():  # only "%*.*i", 6, 6, n is used
    d = R(UC_X86_REG_RDI); assert cstr(R(UC_X86_REG_RSI)) == b"%*.*i"
    w, p, n = R(UC_X86_REG_RDX), R(UC_X86_REG_RCX), R(UC_X86_REG_R8) & 0xffffffff
    s = ("%*.*i" % (w, p, n)).encode(); uc.mem_write(d, s + b"\0"); stub_ret(len(s))
def s_fatal():
    print("dLogDebugLog line", cstr(R(UC_X86_REG_RCX)).decode()); stub_ret(0)

STUBS = {0x10000b6c4: s_malloc, 0x10000b694: s_free, 0x10000b6ee: s_strcpy,
         0x10000b6d0: s_memset, 0x10000b6ca: s_memcpy, 0x10000b6dc: s_sprintf,
         0x10000b65e: s_fatal}

def hook(u, addr, size, _):
    if 0x10000b51a <= addr < 0x10000b718:
        if addr not in STUBS: raise RuntimeError(f"unstubbed import {addr:#x}")
        STUBS[addr]()
uc.hook_add(UC_HOOK_CODE, hook, begin=0x10000b51a, end=0x10000b718)

SYM = {s.name: s.value for s in mb.symbols if s.value}
ARGREGS = [UC_X86_REG_RDI, UC_X86_REG_RSI, UC_X86_REG_RDX, UC_X86_REG_RCX, UC_X86_REG_R8, UC_X86_REG_R9]
def call(name, *args):
    rsp = STACK + 0xff000
    uc.mem_write(rsp, struct.pack("<Q", SENT))
    for k, a in enumerate(args[6:]): uc.mem_write(rsp + 8 + 8 * k, struct.pack("<Q", a & 0xffffffffffffffff))
    uc.reg_write(UC_X86_REG_RSP, rsp)
    for r, a in zip(ARGREGS, args): uc.reg_write(r, a & 0xffffffffffffffff)
    uc.emu_start(SYM[name], SENT)
    return R(UC_X86_REG_RAX)

def s(b):
    p = alloc(len(b) + 1); uc.mem_write(p, b + b"\0"); return p

def orig_rt3(product, depth, name, copies, period):
    alg = call("_RG_GenerateAlgorithm", s(product), s(product), depth)
    out = alloc(256)
    call("_RG_GenerateLicenseCode", alg, s(product), s(name), copies, period, out)
    return cstr(out).decode()

def orig_retail(product, depth, rid):
    alg = call("_RG_GenerateAlgorithm", s(product), s(product), depth)
    ids, codes, txt = alloc(8), alloc(8), alloc(64)
    call("_RG_GenerateRetailCodes", alg, s(product), rid, 1, 1, ids, codes)
    code = struct.unpack("<Q", uc.mem_read(codes, 8))[0]
    call("_RT3_LicenseCodeToText", code, txt)
    return cstr(txt).decode()

def orig_old(product, name, copies, style):
    out = alloc(16)
    call("_RG_GenerateOldStyleCode", s(product), s(name), copies, style, out)
    return cstr(out).decode()

def port(*args):
    o = subprocess.run([CLI, "generate", *args], capture_output=True, text=True, check=True).stdout
    return next(l.split()[1] for l in o.splitlines() if l.startswith("Code:"))

MENU = [l.strip().strip('",') for l in open(sys.argv[3]).read().split("pub const MENU")[1].split("];")[0].splitlines()[1:]]
MENU = [m for m in MENU if m]
def resolve(i):
    t = MENU[i]; d = 0
    while t.startswith(" "): d += 1; t = MENU[i - d]
    for suf, sch in (("?", "ext"), ("!", "old2"), ("**", "rt3-retail"), ("*", "rt3")):
        if t.endswith(suf): return t[: -len(suf)], d, sch
    return t, d, "old1"

rnd = random.Random(1234)
names = ["John Doe", "A. N. Other", "x", "Ambrosia Software, Inc.", "zzz", "O'Brien-Smythe 3rd", "MARY ann"]
n = fails = 0
for i in range(len(MENU)):
    key, depth, sch = resolve(i)
    for _ in range(6):
        name = rnd.choice(names); copies = rnd.choice([1, 2, 5, 10, 99, 500, 32767])
        if sch == "ext": continue
        if sch.startswith("old"):
            st = 1 if sch == "old1" else 2
            a, b = orig_old(key.encode(), name.encode(), copies, st), port("--custom", key, "--scheme", sch, "-n", name, "-c", str(copies))
        else:
            t = rnd.randrange(1_000_000_000, 2_100_000_000)
            period = call("_RT3_CalcTimePeriod", (t - 86400) & 0xffffffff) & 0xff
            a = orig_rt3(key.encode(), depth, name.encode(), copies, period)
            b = port("--custom", key, "--scheme", "rt3", "--depth", str(depth), "-n", name, "-c", str(copies), "--now", str(t))
            if sch == "rt3-retail":
                rid = rnd.randrange(0, 1_000_000)
                a2, b2 = orig_retail(key.encode(), depth, rid), port("--custom", key, "--scheme", sch, "--depth", str(depth), "-n", "%06d" % rid)
                n += 1
                if a2 != b2: fails += 1; print("RETAIL MISMATCH", key, depth, rid, a2, b2)
        n += 1
        if a != b: fails += 1; print("MISMATCH", sch, key, depth, name, copies, a, b)
print(f"{n} comparisons, {fails} mismatches")
