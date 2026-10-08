#!/bin/bash
# Release acceptance (docs/RELEASE.md): prepare + verify locally, no uploads.
#
# Covers: happy-path prep (wheel, sdist, .crate, manifest, verification,
# workflow, setup instructions, pending state), final-source selection
# (--opt wins over --fork), sdist carries + rebuilds the core, metadata /
# prerequisite failures (each names its field), and per-registry tracking
# (partial never reports complete; failed retries per-registry).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
echo "== release: work=$WORK"

for bin in cargo maturin python3 tar; do
  command -v "$bin" >/dev/null 2>&1 || (echo "FAIL: missing $bin"; exit 1)
done

echo "-- unit tests (release config/state/render, candidates, files_of_patch)"
cargo test -q -p rustsmith-release 2>&1 | tail -n 2
cargo test -q -p rustsmith-cli candidates 2>&1 | tail -n 2

echo "-- stage fork + opt from the crc template"
cp -r "$ROOT/mirror/crc" "$WORK/fork"
git -C "$WORK/fork" init -q -b main
git -C "$WORK/fork" config user.email t@t
git -C "$WORK/fork" config user.name t
git -C "$WORK/fork" add -A
git -C "$WORK/fork" commit -qm seed
cp -r "$WORK/fork" "$WORK/opt"
git -C "$WORK/opt" commit -q --allow-empty -m "final accepted merge"
mkdir -p "$WORK/recon"
OPT_HEAD="$(git -C "$WORK/opt" rev-parse HEAD)"
FORK_HEAD="$(git -C "$WORK/fork" rev-parse HEAD)"
test "$OPT_HEAD" != "$FORK_HEAD" || (echo "FAIL: heads must differ"; exit 1)

echo "-- happy path: prep prefers --opt (final accepted source)"
cargo run -q -p rustsmith-cli -- release-prep \
  --project "$ROOT/mirror/crc" --fork "$WORK/fork" --opt "$WORK/opt" \
  --recon-out "$WORK/recon" --out "$WORK/out" > "$WORK/prep.json"
OUT="$WORK/out"
test -f "$OUT/dist/"*.whl
test -f "$OUT/dist/"*.tar.gz
ls "$OUT"/dist/*.crate >/dev/null
test -f "$OUT/SHA256SUMS" -a -f "$OUT/release-manifest.json" \
  -a -f "$OUT/verification.json" -a -f "$OUT/release-state.json" \
  -a -f "$OUT/.github/workflows/release.yml" -a -f "$OUT/TRUSTED_PUBLISHING_SETUP.md"
(cd "$OUT/dist" && sha256sum -c ../SHA256SUMS) || (echo "FAIL: SHA256SUMS mismatch"; exit 1)

echo "-- manifest: source identity is the opt HEAD"
WORKDIR="$WORK" python3 - <<'EOF'
import json, os, subprocess
w = os.environ['WORKDIR']
m = json.load(open(f'{w}/out/release-manifest.json'))
opt = subprocess.run(['git','-C',f'{w}/opt','rev-parse','HEAD'],capture_output=True,text=True).stdout.strip()
assert m['source_tree'] == 'opt', m
assert m['source_sha'] == opt, m
assert m['release_version'] == '0.1.0' and m['pypi_dist'] == 'crc-rust' and m['rust_crate'] == 'crc-rust-core', m
names = sorted(a['file'] for a in m['artifacts'])
assert any(n.endswith('.whl') for n in names) and any(n.endswith('.tar.gz') for n in names) and any(n.endswith('.crate') for n in names), names
print('manifest OK:', m['source_sha'][:8], names)
EOF

echo "-- verification.json: every check passed"
python3 - "$WORK/out/verification.json" <<'EOF'
import json, sys
v = json.load(open(sys.argv[1]))
for k in ['rust_core_tests','rust_consumer','core_package','wheel_build','sdist_build','sdist_contents','sdist_rebuild']:
    assert v[k]['passed'], (k, v[k])
assert all(r['passed'] for r in v['python_install_smoke']), v['python_install_smoke']
assert v['passed'] is True
print('verification OK')
EOF

echo "-- staged README receipt (generated block, measured only, no hashes)"
python3 - "$OUT/stage/README.md" "$OUT/verification.json" <<'EOF'
import json, re, sys
readme = open(sys.argv[1]).read()
v = json.load(open(sys.argv[2]))
assert '<!-- RUSTSMITH-PERF:BEGIN' in readme and '<!-- RUSTSMITH-PERF:END -->' in readme, readme[-400:]
assert 'do not hand-edit' in readme
assert 'No accepted optimizations yet' in readme, readme[-400:]  # no report staged: mirror baseline
assert v['readme_perf']['passed'] is True and v['readme_perf']['merged_count'] == 0, v['readme_perf']
assert not re.search(r'[0-9a-f]{40,}', readme), 'hash leaked into staged README'
print('readme receipt OK')
EOF
RMD_PATH="$(tar tzf "$OUT"/dist/*.tar.gz | grep 'README.md$' | head -n 1)"
test -n "$RMD_PATH" || (echo "FAIL: no README.md in sdist"; exit 1)
test -n "$(tar xzOf "$OUT"/dist/*.tar.gz "$RMD_PATH" | grep 'RUSTSMITH-PERF:BEGIN')" \
  || (echo "FAIL: receipt missing from sdist README"; exit 1)
echo "sdist receipt OK"

echo "-- sdist carries the core (independent re-check, not just prep's word)"
test -n "$(tar tzf "$OUT"/dist/*.tar.gz | grep 'crc-core/Cargo.toml')" || (echo "FAIL: core missing from sdist"; exit 1)
test -n "$(tar tzf "$OUT"/dist/*.tar.gz | grep 'crc/__init__.py')" || (echo "FAIL: shim missing from sdist"; exit 1)
echo "sdist contents OK"

echo "-- workflow: configured artifacts, TestPyPI lane, same-file publish, prod release steps"
WF="$OUT/.github/workflows/release.yml"
grep -q 'crc-rust' "$WF"
grep -q 'test.pypi.org/legacy/' "$WF"
grep -q 'id-token: write' "$WF"
grep -q 'download-artifact' "$WF"
grep -q 'no rebuild' "$WF"
grep -q 'cargo publish --manifest-path crc-core/Cargo.toml' "$WF"
grep -q "3.9.*3.13\|'3.9', '3.10'" "$WF"
grep -q 'ubuntu-22.04' "$WF"
grep -q '${{ matrix.os }}' "$WF" || (echo "FAIL: workflow matrix expression broken"; exit 1)
grep -q 'dtolnay/rust-toolchain@stable' "$WF"
grep -q 'crates-io-auth-action' "$WF"
grep -q 'CARGO_REGISTRY_TOKEN' "$WF"
grep -q 'crc-rust==0.1.0' "$WF"
grep -q 'run: |' "$WF"
grep -q 'release-evidence' "$WF"
grep -q 'wheels-ubuntu-22.04-py3.9' "$WF"
grep -q "python - <<'PYEOF'" "$WF"
if grep -q 'setup-rust' "$WF"; then echo "FAIL: setup-rust still referenced"; exit 1; fi
if grep -q 'merge-multiple' "$WF"; then echo "FAIL: multi-wheel download remains"; exit 1; fi
if grep -q 'echo "record' "$WF"; then echo "FAIL: echo-record step remains"; exit 1; fi
echo "workflow OK"

echo "-- setup instructions: exact project identities"
SETUP="$OUT/TRUSTED_PUBLISHING_SETUP.md"
grep -q 'John-Cusack/crc-rust' "$SETUP"
grep -q 'crc-rust-core' "$SETUP"
grep -q 'release-record' "$SETUP"
grep -q 'release-status' "$SETUP"
echo "setup instructions OK"

echo "-- state starts pending (status refuses)"
if cargo run -q -p rustsmith-cli -- release-status --state "$OUT/release-state.json"; then
  echo "FAIL: pending state reported complete"; exit 1
else
  echo "pending refused OK"
fi

echo "-- autonomous publish lane (offline paths: plan, gates, no-creds, drift)"
PUB="cargo run -q -p rustsmith-cli -- release-publish --state $OUT/release-state.json"
# Dry-run resolves the exact retained files and records nothing.
env -u TWINE_USERNAME -u TWINE_PASSWORD -u RUSTSMITH_ALLOW_PROD_PUBLISH \
  $PUB --registry testpypi --dry-run | grep -q '"files":\[".*\.whl",".*\.tar\.gz"\]' \
  || (echo "FAIL: dry-run plan missing wheel+sdist"; exit 1)
if $PUB --registry testpypi --dry-run | grep -q '\.crate'; then
  echo "FAIL: .crate leaked into python upload plan"; exit 1
fi
echo "  dry-run plan OK"
# Prod lane refuses without opt-in, even green.
# (Capture first: with `pipefail` the refusing exit would mask grep's match.)
out="$(env -u RUSTSMITH_ALLOW_PROD_PUBLISH $PUB --registry pypi --dry-run 2>&1 || true)"
if printf '%s\n' "$out" | grep -q 'RUSTSMITH_ALLOW_PROD_PUBLISH'; then
  echo "  prod gate OK"
else
  echo "FAIL: prod lane did not demand opt-in"; printf '%s\n' "$out" | tail -n 3; exit 1
fi
# Missing credentials fail naming the variable (nothing uploaded, nothing recorded).
out="$(env -u TWINE_USERNAME -u TWINE_PASSWORD $PUB --registry testpypi 2>&1 || true)"
if printf '%s\n' "$out" | grep -q 'TWINE_'; then
  echo "  missing-creds error OK"
else
  echo "FAIL: missing creds not named"; printf '%s\n' "$out" | tail -n 3; exit 1
fi
# Drifted bytes refuse before any upload.
cp -r "$OUT" "$WORK/drift"
printf 'x' >> "$WORK"/drift/dist/*.whl
out="$(cargo run -q -p rustsmith-cli -- release-publish --state "$WORK/drift/release-state.json" \
    --registry testpypi --dry-run 2>&1 || true)"
if printf '%s\n' "$out" | grep -q 'sha256'; then
  echo "  drift refusal OK"
else
  echo "FAIL: drifted wheel not refused"; printf '%s\n' "$out" | tail -n 3; exit 1
fi
# None of the above recorded anything: state still pending.
if cargo run -q -p rustsmith-cli -- release-status --state "$OUT/release-state.json"; then
  echo "FAIL: publish dry-runs recorded outcomes"; exit 1
else
  echo "  state untouched OK"
fi

echo "-- invalid metadata / prerequisites (each fails naming its field)"
bad() { rm -rf "$WORK/bad"; cp -r "$ROOT/mirror/crc" "$WORK/bad"; git -C "$WORK/bad" init -q -b main; git -C "$WORK/bad" config user.email t@t; git -C "$WORK/bad" config user.name t; git -C "$WORK/bad" add -A; git -C "$WORK/bad" commit -qm seed; mkdir -p "$WORK/badrecon"; }
expect_fail() { # $1=desc $2=needle; project=fork=bad
  # Capture first: with `pipefail` a failing prep would mask grep's match.
  out="$(cargo run -q -p rustsmith-cli -- release-prep --project "$WORK/bad" --fork "$WORK/bad" --recon-out "$WORK/badrecon" --out "$WORK/badout" 2>&1 || true)"
  if printf '%s\n' "$out" | grep -q "$2"; then
    echo "  reject OK: $1"
  else
    echo "FAIL: $1 (needle: $2)"; printf '%s\n' "$out" | tail -n 3; exit 1
  fi
}
bad; sed -i 's/^pypi_dist = .*/pypi_dist = "crc"/' "$WORK/bad/release.toml"
expect_fail "colliding dist name" "collides"
bad; sed -i 's/^version = .*/version = "9.9.9"/' "$WORK/bad/pyproject.toml"
expect_fail "version drift (pyproject)" "pyproject version"
bad; sed -i 's/^version = .*/version = "9.9.9"/' "$WORK/bad/crc-core/Cargo.toml"
expect_fail "version drift (core)" "core version"
bad; printf '\npyo3 = "0.23"\n' >> "$WORK/bad/crc-core/Cargo.toml"
expect_fail "python-tied core" "independent of Python"
bad; rm "$WORK/bad/NOTICE"
expect_fail "missing NOTICE" "NOTICE"
bad; rm "$WORK/bad/crc/__init__.py"
expect_fail "missing import shim" "__init__.py"
bad; sed -i 's/^license_spdx = .*/license_spdx = "WTFPL"/' "$WORK/bad/release.toml"
expect_fail "unknown license" "unknown"
bad; sed -i 's/^python_import = .*/python_import = "crc_rs"/' "$WORK/bad/release.toml"
expect_fail "renamed import" "must equal"
bad; rm -rf "$WORK/bad/.git"
expect_fail "no source commit" "no git HEAD"

echo "-- per-registry tracking: partial never complete, retries per-registry"
ST="$OUT/release-state.json"
if cargo run -q -p rustsmith-cli -- release-record --state "$ST" --registry bogus --result success; then
  echo "FAIL: unknown registry accepted"; exit 1; else echo "  unknown registry refused OK"; fi
if cargo run -q -p rustsmith-cli -- release-record --state "$ST" --registry pypi --result maybe; then
  echo "FAIL: unknown result accepted"; exit 1; else echo "  unknown result refused OK"; fi
cargo run -q -p rustsmith-cli -- release-record --state "$ST" --registry pypi --result success --detail "test" | grep -q partial
if cargo run -q -p rustsmith-cli -- release-status --state "$ST"; then
  echo "FAIL: one-of-three reported complete"; exit 1; else echo "  partial refused OK"; fi
cargo run -q -p rustsmith-cli -- release-record --state "$ST" --registry testpypi --result success --detail "test" | grep -q partial
cargo run -q -p rustsmith-cli -- release-record --state "$ST" --registry crates-io --result failed --detail "403" | grep -q partial
if cargo run -q -p rustsmith-cli -- release-status --state "$ST"; then
  echo "FAIL: failed registry reported complete"; exit 1; else echo "  failed lane visible OK"; fi
cargo run -q -p rustsmith-cli -- release-record --state "$ST" --registry crates-io --result success --detail "retry" | grep -q complete
cargo run -q -p rustsmith-cli -- release-status --state "$ST" | grep -q complete || (echo "FAIL: full success not complete"; exit 1)
echo "  complete only when all three succeed OK"

echo "RELEASE ACCEPTANCE GREEN"
