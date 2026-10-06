"""Final emission generator for decoders.rs (charset-normalizer Rust port).

Generator command (recorded in decoders.rs header):
    python3 gen_final.py --tables /tmp/dec_tables.rs --report /tmp/dec_report.txt
Requires: CPython 3.12 (probed on 3.12.3), no third-party deps.

Emits:
  --tables : Rust `const` tables (CJK doubles incl. multi-char pairs, triples,
             iso2022 override/subset tables, gb18030 quad segments)
  --report : counts + sharing decisions + edge-model notes for the header
Also runs verification asserts for every hand-rolled edge model; exits nonzero
on any violation.
"""
import argparse, sys

CJK = ['big5', 'big5hkscs', 'cp932', 'cp949', 'cp950', 'euc_jis_2004',
       'euc_jisx0213', 'euc_jp', 'euc_kr', 'gb18030', 'gb2312', 'gbk',
       'johab', 'shift_jis', 'shift_jis_2004', 'shift_jisx0213']
VARS = {}
FAIL = []
def check(name, cond, extra=""):
    print(("PASS " if cond else "FAIL ") + name + ("" if cond else f" :: {extra}"))
    if not cond:
        FAIL.append(name)

def sdec(enc, data):
    try:
        return data.decode(enc, errors='strict')
    except Exception:
        return None

def idec(enc, data):
    try:
        return data.decode(enc, errors='ignore')
    except Exception as e:
        return f"ERR:{e}"

def doubles(enc):
    """(lead-first only) -> ({key: ch}, {key: (c1,c2)}) single/strict oracle."""
    sing = set()
    for b in range(256):
        if sdec(enc, bytes([b])) is not None:
            sing.add(b)
    one, two = {}, {}
    for a in range(256):
        if a in sing:
            continue
        for b in range(256):
            r = sdec(enc, bytes([a, b]))
            if r is not None:
                k = (a << 8) | b
                if len(r) == 1:
                    one[k] = r
                else:
                    two[k] = tuple(r)
    return sing, one, two

def triples(enc):
    one = {}
    for a in range(256):
        for b in range(256):
            r = sdec(enc, bytes([0x8F, a, b]))
            if r is not None:
                assert len(r) == 1, (enc, a, b, r)
                one[(a << 8) | b] = r
    return one

def iso_grid(variant, pre):
    tab = {}
    for a in range(0x21, 0x7F):
        for b in range(0x21, 0x7F):
            r = sdec(variant, pre + bytes([a, b]))
            if r:
                tab[(a << 8) | b] = r
    return tab

def rust_char(c):
    o = ord(c)
    if o < 0x20 or o in (0x7F,) or c in "'\\":
        return f"'\\u{{{o:X}}}'"
    if 0x20 <= o <= 0x7E:
        return f"'{c}'"
    return f"'\\u{{{o:X}}}'"

def emit_map(f, name, tab, per_line=8):
    f.write(f"pub(crate) const {name}: &[(u16, char)] = &[\n")
    ks = sorted(tab)
    for i in range(0, len(ks), per_line):
        chunk = ks[i:i + per_line]
        f.write("    " + ", ".join(f"(0x{k:04X}, {rust_char(tab[k])})" for k in chunk) + ",\n")
    f.write("];\n")

def emit_multi(f, name, tab, per_line=4):
    f.write(f"pub(crate) const {name}: &[(u16, (char, char))] = &[\n")
    for k in sorted(tab):
        c1, c2 = tab[k]
        f.write(f"    (0x{k:04X}, ({rust_char(c1)}, {rust_char(c2)})),\n")
    f.write("];\n")

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--tables', default='/tmp/dec_tables.rs')
    ap.add_argument('--report', default='/tmp/dec_report.txt')
    args = ap.parse_args()
    rep = []

    S, D1, D2 = {}, {}, {}
    for enc in CJK:
        s, one, two = doubles(enc)
        S[enc], D1[enc], D2[enc] = s, one, two
        rep.append(f"{enc}: singles={len(s)} doubles1={len(one)} doubles2={len(two)}")
        print(f"{enc}: singles={len(s)} doubles1={len(one)} doubles2={len(two)}", flush=True)
    for enc in CJK:
        for k, v in sorted(D2[enc].items()):
            rep.append(f"  multi {enc} 0x{k:04X} -> {'+'.join(f'U+{ord(c):04X}' for c in v)}")
    # gb18030 doubles must have no digit-trail keys (quad-path disjointness)
    bad = [k for k in D1['gb18030'] if 0x30 <= (k & 0xFF) <= 0x39]
    check("gb18030-dbl-no-digit-trail", not bad, bad[:5])
    # euc 8F-first doubles must be empty (SS3 triple-only)
    for enc in ['euc_jp', 'euc_jis_2004', 'euc_jisx0213']:
        bad = [k for k in D1[enc] if (k >> 8) == 0x8F]
        check(f"{enc}-no-8F-doubles", not bad, bad[:5])
    # 8E trail range note
    for enc in ['euc_jp', 'euc_jis_2004', 'euc_jisx0213']:
        tr = sorted(k & 0xFF for k in D1[enc] if (k >> 8) == 0x8E)
        rep.append(f"{enc} 8E-trails: {min(tr):#x}..{max(tr):#x} n={len(tr)}")

    T = {e: triples(e) for e in ['euc_jp', 'euc_jis_2004', 'euc_jisx0213']}
    rep.append(f"triples: EJ_T={len(T['euc_jp'])} E4_T={len(T['euc_jis_2004'])} E3_T={len(T['euc_jisx0213'])}")
    tdiff = [(k, T['euc_jis_2004'].get(k), T['euc_jisx0213'].get(k))
             for k in set(T['euc_jis_2004']) | set(T['euc_jisx0213'])
             if T['euc_jis_2004'].get(k) != T['euc_jisx0213'].get(k)]
    rep.append(f"E4_T-vs-E3_T diffs={len(tdiff)} {[(hex(k), a, b) for k, a, b in tdiff]}")
    check("E4_T-E3_T-1diff", len(tdiff) == 1, str(tdiff))
    arena_subset = set(T['euc_jp']) <= set(T['euc_jis_2004'])
    check("EJ_T-keys-subset-E4_T", arena_subset)
    check("EJ_T-vals-match-E4_T", all(T['euc_jp'][k] == T['euc_jis_2004'][k] for k in T['euc_jp']))

    # subset relations for sharing (base + EXTRA, same values required)
    def subset(a, b):
        ka, kb = set(D1[a]), set(D1[b])
        same = all(D1[a][k] == D1[b][k] for k in ka & kb)
        return ka <= kb, ka - kb, kb - ka, same
    for a, b in [('big5', 'big5hkscs'), ('euc_kr', 'cp949'), ('shift_jis', 'cp932'),
                 ('big5', 'cp950'), ('shift_jis', 'shift_jis_2004'),
                 ('euc_jp', 'euc_jis_2004'), ('gb2312', 'gbk'), ('gbk', 'gb18030')]:
        sub, only_a, only_b, same = subset(a, b)
        rep.append(f"subset {a}<= {b}: {sub} only_a={len(only_a)} only_b={len(only_b)} same={same}")
        print(f"subset {a}<= {b}: {sub} only_a={len(only_a)} only_b={len(only_b)} same={same}")

    # iso grids
    G = {}
    G['jp_B'] = iso_grid('iso2022_jp', b'\x1b$B')
    G['2004_B'] = iso_grid('iso2022_jp_2004', b'\x1b$B')
    G['jp3_B'] = iso_grid('iso2022_jp_3', b'\x1b$B')
    G['jp3_O'] = iso_grid('iso2022_jp_3', b'\x1b$(O')
    G['2004_Q'] = iso_grid('iso2022_jp_2004', b'\x1b$(Q')
    G['jp3_P'] = iso_grid('iso2022_jp_3', b'\x1b$(P')
    G['2004_P'] = iso_grid('iso2022_jp_2004', b'\x1b$(P')
    G['jp2_C'] = iso_grid('iso2022_jp_2', b'\x1b$(C')
    def sh(tab):
        """ISO-space (0x21-0x7E)^2 grid -> EUC-space keys."""
        return {(((a + 0x80) << 8) | (b + 0x80)): v for (k, v) in tab.items()
                for (a, b) in [((k >> 8), (k & 0xFF))]}
    no8e = lambda tab: {k: v for k, v in tab.items() if (k >> 8) != 0x8E}
    check("2004_B==EJ_D-noSS2", sh(G['2004_B']) == no8e(D1['euc_jp']), "")
    check("jp3_B==EJ_D-noSS2", sh(G['jp3_B']) == no8e(D1['euc_jp']), "")
    check("jp_B==EJ_D-noSS2", sh(G['jp_B']) == no8e(D1['euc_jp']), "")
    check("EJ-excluded-is-SS2", all((k >> 8) == 0x8E for k in set(D1['euc_jp']) - set(sh(G['jp_B']))), "")
    check("jp3_P==2004_P", G['jp3_P'] == G['2004_P'])
    p_multi = {k: v for k, v in G['jp3_P'].items() if len(v) != 1}
    check("ISO_P-all-single", not p_multi, str(list(p_multi.items())[:3]))
    c_multi = {k: v for k, v in G['jp2_C'].items() if len(v) != 1}
    check("ISO_C-all-single", not c_multi, str(list(c_multi.items())[:3]))
    # $(O)/$(Q): full-string grids; multi pairs + tilde overrides vs EUC base
    for tag, g, base in [('jp3_O', G['jp3_O'], D1['euc_jisx0213']), ('2004_Q', G['2004_Q'], D1['euc_jis_2004'])]:
        gs = sh(g)
        multi = {k: v for k, v in g.items() if len(v) != 1}
        rep.append(f"{tag}: n={len(g)} multi={len(multi)}")
        for k, v in sorted(multi.items()):
            rep.append(f"  multi {tag} 0x{k:04X} -> {'+'.join(f'U+{ord(c):04X}' for c in v)}")
        ovr = {k: v for k, v in g.items() if len(v) == 1 and base.get((((k >> 8) + 0x80) << 8) | ((k & 0xFF) + 0x80)) != v}
        rep.append(f"{tag}: single-overrides={len(ovr)} {[(hex(k), v) for k, v in sorted(ovr.items())]}")
        gs1 = {k: v for k, v in gs.items() if len(v) == 1}
        onlyg = set(gs1) - set(base)
        onlyb = set(no8e(base)) - set(gs1)
        rep.append(f"{tag}: keys-not-in-base={len(onlyg)} keys-not-in-grid={len(onlyb)}")
        check(f"{tag}-keys-match-base", not onlyg and not onlyb, "")
    def eshift(tab):
        return {((((k >> 8) - 0x80) << 8) | ((k & 0xFF) - 0x80)): tuple(v) for k, v in tab.items()}
    check("M_ISO_O==M_E3", eshift(D2['euc_jisx0213']) == {k: tuple(v) for k, v in G['jp3_O'].items() if len(v) != 1}, "")
    check("M_ISO_Q==M_E4", eshift(D2['euc_jis_2004']) == {k: tuple(v) for k, v in G['2004_Q'].items() if len(v) != 1}, "")
    for enc in CJK:
        ks = sorted(D1[enc])
        leads = sorted(set(k >> 8 for k in ks))
        rep.append(f"{enc} leads: n={len(leads)} {leads[0]:#x}..{leads[-1]:#x}")
    gb_keys = set(D1['gb2312'])
    check("hz-FE-safety", not any((k >> 8) == 0xFE for k in gb_keys), "")
    for enc in ['gb18030', 'gbk']:
        bad80 = [k for k in D1[enc] if (k >> 8) in (0x80, 0xFF)]
        rep.append(f"{enc} 80/FF-lead keys: {len(bad80)}")
    onlyc = {k: v for k, v in G['jp2_C'].items() if ((((k >> 8) + 0x80) << 8) | ((k & 0xFF) + 0x80)) not in D1['euc_kr']}
    rep.append(f"jp2_C extra vs EK_D: {[(hex(k), v) for k, v in sorted(onlyc.items())]}")
    check("jp2_C-keys", set(sh(G['jp2_C'])) >= set(D1['euc_kr']), "")

    # gb18030 quad full enumeration -> segments
    print("quad full enumeration (1.587M decodes)...", flush=True)
    valid = {}
    for b1 in range(0x81, 0xFF):
        for b2 in range(0x30, 0x3A):
            for b3 in range(0x81, 0xFF):
                base = bytes([b1, b2, b3])
                for b4 in range(0x30, 0x3A):
                    try:
                        r = (base + bytes([b4])).decode('gb18030', errors='strict')
                    except Exception:
                        continue
                    if len(r) == 1:
                        ptr = (b1 - 0x81) * 12600 + (b2 - 0x30) * 1260 + (b3 - 0x81) * 10 + (b4 - 0x30)
                        valid[ptr] = ord(r)
    rep.append(f"quad valid={len(valid)} maxptr={max(valid)}")
    print(f"quad valid={len(valid)} maxptr={max(valid)}", flush=True)
    items = sorted(valid.items())
    segs = []
    s0, c0 = items[0]
    p0 = s0
    for (p1, c1), (p2, c2) in zip(items, items[1:]):
        if p2 == p1 + 1 and c2 == c1 + 1:
            continue
        segs.append((s0, p1, c0))
        s0, c0 = p2, c2
    segs.append((s0, items[-1][0], c0))
    rep.append(f"quad segments={len(segs)}")
    for s in segs:
        rep.append(f"  seg ptr {s[0]}..{s[1]} -> U+{s[2]:04X}+off")
    # verify formula reproduces oracle on FULL space (valid + invalid)
    def quad_lookup(ptr):
        import bisect
        return None
    los = [s[0] for s in segs]
    import bisect
    bad = 0
    for b1 in range(0x81, 0xFF):
        for b2 in range(0x30, 0x3A):
            for b3 in range(0x81, 0xFF):
                for b4 in range(0x30, 0x3A):
                    ptr = (b1 - 0x81) * 12600 + (b2 - 0x30) * 1260 + (b3 - 0x81) * 10 + (b4 - 0x30)
                    i = bisect.bisect_right(los, ptr) - 1
                    cp = None
                    if i >= 0:
                        lo, hi, base = segs[i]
                        if lo <= ptr <= hi:
                            cp = base + (ptr - lo)
                    try:
                        r = bytes([b1, b2, b3, b4]).decode('gb18030', errors='strict')
                        want = ord(r) if len(r) == 1 else 'MULTI'
                    except Exception:
                        want = None
                    got = cp
                    if (want is None) != (got is None) or (isinstance(want, int) and want != got):
                        bad += 1
                        if bad < 5:
                            rep.append(f"  QUAD-MISMATCH {b1:02x}{b2:02x}{b3:02x}{b4:02x} want={want} got={got}")
    check("quad-formula-exact", bad == 0, f"{bad} mismatches")
    rep.append(f"quad verify mismatches={bad}")

    # edge-model verification asserts
    check("8F-fallback", idec('euc_jp', bytes([0x8F, 0xA4, 0xA2])) == 'あ')
    check("F4-max", sdec('utf_8', bytes([0xF4, 0x8F, 0xBF, 0xBF])) == '\U0010FFFF')
    check("hz-end-tilde-brace-err", sdec('hz', b'~{0e~{') is None)
    check("hz-end-tilde-brace-ig", idec('hz', b'~{0e~{') == '板')
    check("hz-end-exit", sdec('hz', b'~{0e~}') == '板')
    check("hz-lf-cont", sdec('hz', b'~\nA') == 'A')
    check("iso-trailing-odd", idec('iso2022_jp', b'\x1b$B\x24\x22\x20') == 'あ')
    check("iso-inv-desig-X", idec('iso2022_jp', b'\x1b(XAB') == 'AB')
    check("iso-inv-desig-dot", idec('iso2022_jp', b'\x1b.AAB') == 'AB')
    check("iso-inv-desig-3B", idec('iso2022_jp', b'\x1b$XAB') == 'AB')
    check("iso-inv-desig-4B", idec('iso2022_jp', b'\x1b$(XAB') == 'AB')
    check("iso-inv-desig-(A-jp", idec('iso2022_jp', b'\x1b(AAB') == 'AB')
    check("iso-trunc-desig-end", idec('iso2022_jp', b'\x1b$B\x24\x22\x1b$') == 'あ')
    check("iso-inv-4B-end", idec('iso2022_jp', b'\x1b$B\x24\x22\x1b$(X') == 'あ')
    check("kr-SO-then-desig", sdec('iso2022_kr', b'\x0e\x1b$(CAB') == 'AB')
    check("kr-KS-ctrl", sdec('iso2022_kr', b'\x1b$(C\x00AB') == '\x00좌')
    check("kr-hi-err", sdec('iso2022_kr', b'\x80') is None and idec('iso2022_kr', b'\x80A') == 'A')
    check("qb3-80", idec('gb18030', bytes([0x81, 0x30, 0x80, 0x30])) == '0')
    check("qb3-FF", sdec('gb18030', bytes([0x81, 0x30, 0xFF, 0x30]) + b'A') is None)
    check("qb1-80", idec('gb18030', bytes([0x80, 0x30, 0x81, 0x30])) == '0')
    check("cp932-80-single", sdec('cp932', bytes([0x80])) == '\x80')
    check("cp932-A0-single", sdec('cp932', bytes([0xA0])) == '\uf8f0')
    check("cp932-FD", sdec('cp932', bytes([0xFD])) == '\uf8f1')

    # emit tables
    with open(args.tables, 'w') as f:
        f.write("//! GENERATED by gen_final.py (CPython 3.12.3 oracle). Do not hand-edit.\n")
        # shared bases + extras
        f.write("// sharing: GBK_D base; GB18030_X extra; E3_D base; E4_X extra(+10);\n")
        f.write("// XS21_D base (shift_jisx0213); XS04_X extra(+10); XS04_O1 override(0xFC5A);\n")
        emit_map(f, "T_GBK_D", D1['gbk'])
        emit_map(f, "T_GB18030_X", {k: v for k, v in D1['gb18030'].items() if k not in D1['gbk']})
        emit_map(f, "T_GB2312_D", D1['gb2312'])
        emit_map(f, "T_E3_D", D1['euc_jisx0213'])
        emit_map(f, "T_E4_X", {k: v for k, v in D1['euc_jis_2004'].items() if k not in D1['euc_jisx0213']})
        emit_map(f, "T_EJ_D", D1['euc_jp'])
        emit_map(f, "T_EK_D", D1['euc_kr'])
        emit_map(f, "T_BIG5_D", D1['big5'])
        emit_map(f, "T_CP950_X", {k: v for k, v in D1['cp950'].items() if k not in D1['big5'] or D1['big5'][k] != v})
        emit_map(f, "T_HKS_D", D1['big5hkscs'])
        emit_map(f, "T_SJ_D", D1['shift_jis'])
        emit_map(f, "T_CP932_X", {k: v for k, v in D1['cp932'].items() if k not in D1['shift_jis'] or D1['shift_jis'].get(k) != v})
        emit_map(f, "T_XS21_D", D1['shift_jisx0213'])
        emit_map(f, "T_XS04_X", {k: v for k, v in D1['shift_jis_2004'].items() if k not in D1['shift_jisx0213'] or D1['shift_jisx0213'].get(k) != v})
        emit_map(f, "T_CP949_X", {k: v for k, v in D1['cp949'].items() if k not in D1['euc_kr'] or D1['euc_kr'].get(k) != v})
        emit_map(f, "T_JOHAB_D", D1['johab'])
        emit_map(f, "T_EJ_T", T['euc_jp'])
        emit_map(f, "T_E34_T", T['euc_jis_2004'])
        emit_map(f, "T_E3_O1", {tdiff[0][0]: tdiff[0][2]})
        emit_multi(f, "M_HKS", D2['big5hkscs'])
        emit_multi(f, "M_E4", D2['euc_jis_2004'])
        emit_multi(f, "M_E3", D2['euc_jisx0213'])
        emit_multi(f, "M_XS04", D2['shift_jis_2004'])
        emit_multi(f, "M_XS21", D2['shift_jisx0213'])
        # iso pair tables: T_ISO_P shared plane-2; $(O)/$(Q) = E3/E4 base + ISO-space overrides/multis
        emit_map(f, "T_ISO_P", {k: v for k, v in G['jp3_P'].items()})
        f.write("// $(O)/$(Q) base = T_E3_D/T_E4_D (shifted key); overrides below (multi pairs + U+007E tilde)\n")
        def ekey(k):
            return (((k >> 8) + 0x80) << 8) | ((k & 0xFF) + 0x80)
        o_ovr = {k: v for k, v in G['jp3_O'].items() if len(v) == 1 and D1['euc_jisx0213'].get(ekey(k)) != v}
        q_ovr = {k: v for k, v in G['2004_Q'].items() if len(v) == 1 and D1['euc_jis_2004'].get(ekey(k)) != v}
        emit_map(f, "T_ISO_O_OVR", o_ovr)
        emit_map(f, "T_ISO_Q_OVR", q_ovr)
        o_multi = {k: tuple(v) for k, v in G['jp3_O'].items() if len(v) != 1}
        q_multi = {k: tuple(v) for k, v in G['2004_Q'].items() if len(v) != 1}
        emit_multi(f, "M_ISO_O", o_multi)
        emit_multi(f, "M_ISO_Q", q_multi)
        c_extra = {k: v for k, v in G['jp2_C'].items() if len(v) == 1 and ekey(k) not in D1['euc_kr']}
        emit_map(f, "T_ISO_C_X", c_extra)
        rep.append(f"emit: O_OVR={len(o_ovr)} Q_OVR={len(q_ovr)} O_M={len(o_multi)} Q_M={len(q_multi)} C_X={len(c_extra)} P={len(G['jp3_P'])}")
        f.write("pub(crate) const GB4_SEGS: &[(u32, u32, u32)] = &[\n")
        for s in segs:
            f.write(f"    ({s[0]}, {s[1]}, 0x{s[2]:X}),\n")
        f.write("];\n")
    with open(args.report, 'w') as f:
        f.write("\n".join(rep) + "\n")
    print(f"TABLES->{args.tables} REPORT->{args.report}")
    if FAIL:
        print(f"FAILURES: {FAIL}")
        sys.exit(1)
    print("ALL-CHECKS-PASS")

if __name__ == '__main__':
    main()
