# mdurl lane WORKLOAD.md (frozen 2026-10-29)

Distribution: doc-synthetic, modeled on per-link markdown URL handling
(markdown-it call path: `mdurl.parse` + `mdurl.format` per link).

Corpus: 2000 URLs, 4 classes × 500:
- absolute with auth+port+query+fragment:
  `https://user{i}:pass{i}@example{i%100}.com:8080/p/{i}?a=1&b={i}#frag`
- relative path + query + fragment: `/relative/path/{i}?x={i}&y=z#s{i}`
- unicode host + path: `https://münchen-{i%50}.de/ünicode/{i}`
- mailto: `mailto:user+tag-{i}@mail{i%100}.example.com`

`run()`: for each URL, `mdurl.parse(u)` then `mdurl.format(p)`,
accumulate formatted lengths (prevents dead-code elimination).

Scale: REPS=300 full-corpus passes = 600,000 parse+format ops.
Measured wall ~3.0s on Ryzen 9 5950X (CPython 3.12.3).
Probe wall (1 pass, 2000 URLs) was 0.031s; ×300 sizes to seconds.

Tracks:
- representative: the frozen corpus above (mixed classes).
- No showcase/overhead split: per-URL cost is uniform (~5us);
  single crossing per URL already satisfies P1 by construction.
