#!/usr/bin/env bash
# verify-sample: independently re-check a sample of `export-records` JSONL.
#
# Re-validates schema v1, recomputes record_id, cross-checks gate verdicts
# and row fields against store.db, and verifies each sampled patch
# (`git apply --stat` well-formedness + files_touched agreement; with
# --tree, reverse-apply --check against the delivered tree proves the patch
# content matches the graded artifact).
#
# Usage:
#   verify-sample.sh --records <records.jsonl> --store <store.db>
#                    [--n N] [--seed S] [--tree DIR]
#
# Exit 0 when every sampled check passes, 1 otherwise.
set -euo pipefail

RECORDS=""; STORE=""; N=5; SEED=1; TREE=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --records) RECORDS="$2"; shift 2;;
    --store) STORE="$2"; shift 2;;
    --n) N="$2"; shift 2;;
    --seed) SEED="$2"; shift 2;;
    --tree) TREE="$2"; shift 2;;
    *) echo "verify-sample: unknown arg $1" >&2; exit 2;;
  esac
done
[[ -n "$RECORDS" && -n "$STORE" ]] || { echo "verify-sample: --records and --store required" >&2; exit 2; }
[[ -f "$RECORDS" ]] || { echo "verify-sample: no such file $RECORDS" >&2; exit 2; }
[[ -f "$STORE" ]] || { echo "verify-sample: no such file $STORE" >&2; exit 2; }

export VERIFY_RECORDS="$RECORDS" VERIFY_STORE="$STORE" VERIFY_N="$N" VERIFY_SEED="$SEED" VERIFY_TREE="$TREE"

python3 - <<'PYEOF'
import hashlib, json, math, os, random, sqlite3, subprocess, sys

records_path = os.environ["VERIFY_RECORDS"]
store_path = os.environ["VERIFY_STORE"]
n = int(os.environ["VERIFY_N"])
seed = int(os.environ["VERIFY_SEED"])
tree = os.environ["VERIFY_TREE"] or None

fails = []
checked = {"schema": 0, "record_id": 0, "db": 0, "patch": 0, "reverse": 0}

def fail(msg):
    fails.append(msg)
    print(f"FAIL {msg}")

def ok(msg):
    print(f"ok {msg}")

KINDS = {"mirror_unit", "optimization", "failed_optimization", "harvest_item"}

def finite_or_null(v, name, rid):
    if v is None:
        return
    if not isinstance(v, (int, float)) or not math.isfinite(v):
        fail(f"{rid}: {name} not null-or-finite: {v!r}")

recs = []
with open(records_path) as f:
    for ln, line in enumerate(f, 1):
        line = line.strip()
        if not line:
            continue
        try:
            recs.append((ln, json.loads(line)))
        except json.JSONDecodeError as e:
            fail(f"line {ln}: invalid JSON: {e}")
if not recs:
    fail("no records in file")
    sys.exit(1)

# Full-file schema pass (cheap), sampled deep pass.
for ln, r in recs:
    rid = r.get("record_id", f"line{ln}")
    for k in ["schema_version", "record_id", "record_kind", "run_id", "unit_id",
              "provenance", "code", "measurement", "review", "rights"]:
        if k not in r:
            fail(f"{rid}: missing key {k}")
    if r.get("schema_version") != 1:
        fail(f"{rid}: schema_version != 1")
    if r.get("record_kind") not in KINDS:
        fail(f"{rid}: bad record_kind {r.get('record_kind')!r}")
    m = r.get("measurement", {})
    if not isinstance(m.get("oracle", {}).get("total"), int) or m["oracle"]["total"] <= 0:
        fail(f"{rid}: oracle.total <= 0")
    for name in ["visible_gain_pct", "heldout_gain_pct", "divergence_pct", "ci_low",
                 "ci_high", "rss_delta_pct", "alloc_delta_pct", "binary_delta_pct",
                 "compile_delta_pct", "noise_floor_pct"]:
        finite_or_null(m.get(name), name, rid)
    finite_or_null(r.get("provenance", {}).get("cost_usd"), "cost_usd", rid)
    if not r.get("rights", {}).get("upstream_license"):
        fail(f"{rid}: empty rights.upstream_license")
    rejected = r.get("record_kind") == "failed_optimization" and m.get("outcome") == "rejected_at_proposal"
    if r.get("record_kind") == "mirror_unit":
        if not r.get("code", {}).get("commit_sha"):
            fail(f"{rid}: mirror_unit without commit_sha")
    elif not rejected and (not r.get("code", {}).get("parent_sha") or not r.get("code", {}).get("patch_text")):
        fail(f"{rid}: empty parent_sha/patch_text outside rejected_at_proposal")
    checked["schema"] += 1

    # record_id recompute (exporter contract: sha256 hex of the join, [:16]).
    want = hashlib.sha256("|".join([
        r.get("run_id", ""), r.get("unit_id", ""),
        r.get("code", {}).get("parent_sha", ""), r.get("code", {}).get("commit_sha", ""),
    ]).encode()).hexdigest()[:16]
    if r.get("record_id") != want:
        fail(f"{rid}: record_id mismatch (want {want})")
    else:
        checked["record_id"] += 1

# Sampled deep checks against store.db + git.
rng = random.Random(seed)
sample = rng.sample(recs, min(n, len(recs)))
db = sqlite3.connect(f"file:{store_path}?mode=ro", uri=True)
db.row_factory = sqlite3.Row
for ln, r in sample:
    rid = r["record_id"]
    run_id, unit_id = r["run_id"], r["unit_id"]
    kind = r["record_kind"]
    if kind == "mirror_unit":
        u = db.execute("SELECT status, COALESCE(commit_sha,'') AS sha FROM units WHERE id=? AND run_id=?",
                       (unit_id, run_id)).fetchone()
        if u is None:
            fail(f"{rid}: unit {unit_id} missing in store"); continue
        if u["status"] == "passed":
            want_outcome = "merged" if all(v["passed"] for v in r["review"]["gate_verdicts"]) else "gate_failed"
        elif u["status"] == "failed":
            want_outcome = "gate_failed"
        elif u["status"] in ("parked", "abandoned"):
            want_outcome = u["status"]
        else:
            fail(f"{rid}: unit status {u['status']} not a terminal lane status"); continue
        if r["measurement"]["outcome"] != want_outcome:
            fail(f"{rid}: outcome {r['measurement']['outcome']!r} != {want_outcome!r} for status {u['status']}"); continue
        if u["sha"] != r["code"]["commit_sha"]:
            fail(f"{rid}: commit_sha drift (store {u['sha']!r})"); continue
        rows = db.execute(
            "SELECT gate, passed FROM gate_results WHERE unit_id=? ORDER BY id", (unit_id,)).fetchall()
        got = {(g, bool(p)) for g, p in rows}
        want_v = {(v["gate"], v["passed"]) for v in r["review"]["gate_verdicts"]}
        if got != want_v:
            fail(f"{rid}: gate verdicts drift (store {sorted(got)} vs record {sorted(want_v)})"); continue
        # Held-out magnitudes must never appear in summaries.
        for v in r["review"]["gate_verdicts"]:
            s = v.get("detail_summary", "")
            if "heldout_rate" in s or "visible_rate" in s:
                fail(f"{rid}: held-out magnitude leaked into {v['gate']} summary"); break
        else:
            checked["db"] += 1
            ok(f"{rid}: mirror verdicts match ({len(rows)} gates)")
            continue
    elif kind == "optimization":
        hotspot = unit_id[len(run_id) + 1:] if unit_id.startswith(run_id + ":") else unit_id
        rows = db.execute("SELECT round, technique, parent_sha, commit_sha FROM optimizations "
                          "WHERE run_id=? AND hotspot=?", (run_id, hotspot)).fetchall()
        if not rows:
            fail(f"{rid}: optimization row {hotspot} missing in store"); continue
        want = (r["round"], r["provenance"]["technique"], r["code"]["parent_sha"], r["code"]["commit_sha"])
        if not any((o["round"], o["technique"], o["parent_sha"], o["commit_sha"]) == want for o in rows):
            fail(f"{rid}: optimization row drift (no store row matches {want!r})"); continue
        checked["db"] += 1
        ok(f"{rid}: optimization row matches")
    elif kind == "failed_optimization":
        hotspot = unit_id[len(run_id) + 1:] if unit_id.startswith(run_id + ":") else unit_id
        # (run, hotspot) is not unique: several attempts can fail on one
        # hotspot. Match the exact row the record was projected from.
        rows = db.execute("SELECT round, outcome, gate, parent_sha FROM failed_optimizations "
                          "WHERE run_id=? AND hotspot=?", (run_id, hotspot)).fetchall()
        if not rows:
            fail(f"{rid}: failed row {hotspot} missing in store"); continue
        want = (r["round"], r["measurement"]["outcome"], r["measurement"]["gate"], r["code"]["parent_sha"])
        if not any((f["round"], f["outcome"], f["gate"], f["parent_sha"]) == want for f in rows):
            fail(f"{rid}: failed row drift (no store row matches {want!r})"); continue
        checked["db"] += 1
        ok(f"{rid}: failed row matches")
    else:
        continue

    patch = r["code"].get("patch_text", "")
    if not patch:
        continue
    # Well-formedness + files_touched agreement (needs no tree).
    p = subprocess.run(["git", "apply", "--stat", "--summary"], input=patch,
                       capture_output=True, text=True)
    if p.returncode != 0:
        fail(f"{rid}: patch ill-formed: {p.stderr.strip()[:200]}"); continue
    stat_files = set()
    for pline in (p.stdout + p.stderr).splitlines():
        pline = pline.strip()
        if " | " in pline:
            stat_files.add(pline.split(" | ")[0].strip())
    touched = set(r["code"].get("files_touched", []))
    if touched and not touched <= {f.removeprefix("a/").removeprefix("b/") for f in stat_files} \
            and not touched <= stat_files:
        fail(f"{rid}: files_touched {sorted(touched)} not in patch stat {sorted(stat_files)}"); continue
    checked["patch"] += 1
    ok(f"{rid}: patch well-formed ({len(stat_files)} file(s))")
    if tree:
        q = subprocess.run(["git", "-C", tree, "apply", "--check", "--reverse"],
                           input=patch, capture_output=True, text=True)
        if q.returncode != 0:
            fail(f"{rid}: reverse-apply --check failed against {tree}: {q.stderr.strip()[:200]}")
        else:
            checked["reverse"] += 1
            ok(f"{rid}: reverse-applies against tree")

print(f"sampled={len(sample)} schema={checked['schema']} ids={checked['record_id']} "
      f"db={checked['db']} patches={checked['patch']} reverse={checked['reverse']} failures={len(fails)}")
sys.exit(1 if fails else 0)
PYEOF
