#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# Kernel Freeze guard (D115 / audit A-05)
#
# A-05 said the crate graph was cyclic and the "integration contract" was a
# name with nothing behind it. The fix is not a promise — it is this script.
# It fails the build if the invariants that make the fix real are broken.
#
# Run: scripts/check-freeze.sh
# ─────────────────────────────────────────────────────────────────────────────
set -uo pipefail

cd "$(dirname "$0")/.."
ROOT=$(pwd)
fail=0

# BUGFIX 2026-09-09 (matt): path log tadinya hardcoded ke /tmp/freeze-*.log.
# Dengan banyak user di satu mesin, file itu dimiliki siapa pun yang menjalankan
# lebih dulu; user berikutnya gagal menimpa (Operation not permitted), redirect
# gagal, dan skrip melaporkan FAIL PALSU seolah build/clippy/test rusak.
# Sekarang tiap run pakai direktori unik per-user dan dibersihkan saat keluar.
TMPD=$(mktemp -d "${TMPDIR:-/tmp}/freeze-check-$(id -un)-XXXXXX") || {
  echo "cannot create temp dir"; exit 127; }
trap 'rm -rf "$TMPD"' EXIT
note() { printf '  %s\n' "$*"; }
ok()   { printf '  \033[32mOK\033[0m   %s\n' "$*"; }
bad()  { printf '  \033[31mFAIL\033[0m %s\n' "$*"; fail=1; }

# BUGFIX 2026-09-09 (matt): sebelumnya ada 4 jalur yang MELEWATI pemeriksaan
# asiklisitas tanpa menyetel fail=1 — python3 tidak ada (tanpa else, jadi senyap
# total), cargo metadata gagal, meta.json tak ter-parse, dan 'kernel' tidak ada
# di metadata. Pesan akhir tetap mencetak "kernel is acyclic" di keempatnya.
# Itu klaim yang tidak didukung pemeriksaan apa pun — kelas cacat yang sama
# dengan gate yang mengesahkan sifat yang tidak pernah diukurnya.
# Sekarang tiap lompatan dicatat, dan pesan akhir menyesuaikan.
skipped=0
skip() { printf '  \033[33mSKIP\033[0m %s\n' "$*"; skipped=$((skipped + 1)); }

command -v cargo >/dev/null 2>&1 || { echo "cargo not found"; exit 127; }

echo "── 1. kernel exists and has source ──────────────────────────────────────"
if [ -f crates/kernel/Cargo.toml ]; then
  ok "crates/kernel/Cargo.toml"
else
  bad "crates/kernel/Cargo.toml missing"
fi
NSRC=$(find crates/kernel/src -name '*.rs' 2>/dev/null | wc -l | tr -d ' ')
if [ "$NSRC" -ge 5 ]; then
  ok "$NSRC source files in crates/kernel/src"
else
  bad "only $NSRC source files — contract looks empty (A-05)"
fi

echo
echo "── 2. kernel has NO internal crate dependencies (the acyclicity proof) ──"
# Anything path-based in kernel's manifest would be an internal dep.
if grep -nE 'path\s*=\s*"\.\.' crates/kernel/Cargo.toml >/dev/null 2>&1; then
  bad "kernel depends on an internal crate via path:"
  grep -nE 'path\s*=\s*"\.\.' crates/kernel/Cargo.toml | sed 's/^/        /'
else
  ok "no path dependencies — kernel cannot participate in a cycle"
fi

echo
echo "── 3. kernel dependency allowlist (max: serde, serde_json, async-trait, thiserror) ──"
ALLOWED='serde serde_json async-trait thiserror'
DEPS=$(python3 - "$ROOT/crates/kernel/Cargo.toml" <<'PY'
import sys, re
txt = open(sys.argv[1], encoding='utf-8').read()
# crude but sufficient: take the [dependencies] section
m = re.search(r'\[dependencies\](.*?)(\n\[|\Z)', txt, re.S)
if not m:
    sys.exit(0)
for line in m.group(1).splitlines():
    line = line.split('#')[0].strip()
    if not line or line.startswith('['):
        continue
    # Handles both `serde = { workspace = true }` and `serde.workspace = true`.
    name = re.split(r'[\s=.]', line, maxsplit=1)[0].strip()
    if name:
        print(name)
PY
)
for d in $DEPS; do
  case " $ALLOWED " in
    *" $d "*) ok "allowed: $d" ;;
    *) bad "DISALLOWED dependency in kernel: $d (needs an ADR)" ;;
  esac
done
ND=$(echo "$DEPS" | grep -c . || true)
note "total: $ND direct dependencies"
# Runtime / IO / JS crates in the kernel would drag policy into the contract.
for forbidden in tokio reqwest quickjs rquickjs rusqlite sqlx axum hyper futures rayon; do
  if echo "$DEPS" | grep -qx "$forbidden"; then
    bad "kernel must not depend on '$forbidden' — that is policy, not contract"
  fi
done

echo
echo "── 4. crate graph is a DAG ──────────────────────────────────────────────"
# cargo metadata gives the resolve graph; detect any cycle among workspace crates.
if command -v python3 >/dev/null 2>&1; then
  cargo metadata --format-version 1 --no-deps 2>/dev/null > $TMPD/meta.json || true
  if [ -s $TMPD/meta.json ]; then
    FREEZE_META="$TMPD/meta.json" python3 - <<'PY'
import json, os, sys
try:
    meta = json.load(open(os.environ['FREEZE_META'], encoding='utf-8'))
except Exception as e:
    print(f"  \033[33mSKIP\033[0m could not parse cargo metadata: {e}")
    sys.exit(2)   # 2 = diperiksa-tapi-tak-berhasil, beda dari 0 = benar-benar lulus

ws = set(meta.get('workspace_members', []))
pkgs = {p['id']: p for p in meta.get('packages', [])}
edges = {}
for p in meta.get('packages', []):
    deps = []
    for d in p.get('dependencies', []):
        # only internal (path/workspace) deps matter for cycles
        if d.get('path') or d.get('source') is None:
            deps.append(d['name'].replace('-', '_'))
    edges[p['name'].replace('-', '_')] = deps

# DFS cycle detection
WHITE, GREY, BLACK = 0, 1, 2
color = {n: WHITE for n in edges}
stack = []
# BUGFIX 2026-09-09 (matt): tadinya `cycle = None`, lalu `cycle[:] = ...`
# melempar TypeError, sehingga pesan "cycle detected" tidak pernah tercetak.
# Gagalnya fail-closed (exit 1 -> fail=1) jadi BUKAN lolos palsu, tapi yang
# muncul traceback dan diagnostik siklusnya hilang.
cycle = []

def visit(n):
    global cycle
    if cycle:
        return
    color[n] = GREY
    stack.append(n)
    for m in edges.get(n, []):
        if m not in color:
            continue
        if color[m] == GREY:
            i = stack.index(m)
            cycle[:] = stack[i:] + [m]
            return
        if color[m] == WHITE:
            visit(m)
            if cycle:
                return
    stack.pop()
    color[n] = BLACK

for n in list(edges):
    if color[n] == WHITE:
        visit(n)
    if cycle:
        break

if cycle:
    print("  \033[31mFAIL\033[0m cycle detected: " + " -> ".join(cycle))
    sys.exit(1)
print(f"  \033[32mOK\033[0m   no cycles among {len(edges)} workspace crate(s)")

# kernel must have zero internal deps
k = edges.get('kernel')
if k is None:
    print("  \033[33mSKIP\033[0m kernel not in metadata")
    sys.exit(2)
elif k:
    print(f"  \033[31mFAIL\033[0m kernel has internal deps: {k}")
    sys.exit(1)
else:
    print("  \033[32mOK\033[0m   kernel has 0 internal dependencies")
PY
    dag_rc=$?
    if [ "$dag_rc" -eq 2 ]; then
      skip "pemeriksaan DAG tidak selesai - asiklisitas TIDAK terverifikasi"
    elif [ "$dag_rc" -ne 0 ]; then
      fail=1
    fi
  else
    skip "cargo metadata unavailable — asiklisitas TIDAK diperiksa"
  fi
else
  skip "python3 tidak ada — pemeriksaan DAG tidak dijalankan sama sekali"
fi

echo
echo "── 5. no unsafe in kernel ───────────────────────────────────────────────"
if grep -rn 'unsafe' crates/kernel/src/ 2>/dev/null | grep -v 'forbid(unsafe_code)' | grep -v '^\s*//' >/dev/null; then
  bad "found 'unsafe' in kernel source:"
  grep -rn 'unsafe' crates/kernel/src/ | grep -v 'forbid(unsafe_code)' | sed 's/^/        /'
else
  ok "no unsafe code (#![forbid(unsafe_code)] enforced)"
fi

echo
echo "── 6. build + clippy + tests ────────────────────────────────────────────"
if cargo build --all-targets --quiet >$TMPD/build.log 2>&1; then
  ok "cargo build --all-targets"
else
  bad "cargo build failed:"; sed 's/^/        /' $TMPD/build.log | head -30
fi

if cargo clippy --all-targets --quiet >$TMPD/clippy.log 2>&1; then
  NW=$(grep -c '^warning' $TMPD/clippy.log || true)
  if [ "$NW" -eq 0 ]; then ok "cargo clippy (0 warnings)"; else bad "clippy produced $NW warnings"; fi
else
  bad "cargo clippy failed"; sed 's/^/        /' $TMPD/clippy.log | head -20
fi

if cargo test --quiet >$TMPD/test.log 2>&1; then
  NP=$(grep -oE '[0-9]+ passed' $TMPD/test.log | awk '{s+=$1} END {print s+0}')
  if [ "$NP" -ge 15 ]; then
    ok "cargo test — $NP tests passed"
  else
    bad "only $NP tests passed — expected >= 15 contract tests"
  fi
else
  bad "cargo test failed"; sed 's/^/        /' $TMPD/test.log | head -30
fi

echo
echo "── 7. contract surface is non-empty (A-05 was: 'contract is a name') ────"
for sym in 'pub trait Node' 'pub trait SpillStore' 'pub trait PriorOutputs' \
           'pub enum ItemList' 'pub struct Item ' 'pub enum TaskStatus' \
           'pub struct Checkpoint' 'pub enum SideEffect' 'pub struct ParameterSchema'; do
  if grep -rq "$sym" crates/kernel/src/ 2>/dev/null; then
    ok "$sym"
  else
    bad "missing from contract: $sym"
  fi
done

echo
if [ "$fail" -eq 0 ] && [ "$skipped" -eq 0 ]; then
  printf '\033[32mFREEZE CHECK PASSED\033[0m — kernel is acyclic, dependency-light, and non-empty.\n'
  exit 0
elif [ "$fail" -eq 0 ]; then
  # Lulus, tapi ada pemeriksaan yang tidak berjalan. Klaim penuh tidak jujur di sini.
  printf '\033[33mFREEZE CHECK PASSED WITH %d SKIP\033[0m — dependency-light dan non-empty terverifikasi;\n' "$skipped"
  printf '\033[33m  asiklisitas BELUM tentu\033[0m (lihat baris SKIP di atas). Perbaiki lingkungan lalu jalankan ulang.\n'
  exit 0
else
  printf '\033[31mFREEZE CHECK FAILED\033[0m — see FAIL lines above.\n'
  exit 1
fi
