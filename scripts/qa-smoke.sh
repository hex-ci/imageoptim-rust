#!/bin/sh
# Non-intrusive interaction regression: quiet mode + in-process egui input
# injection with assertions (QA-PASS / exit 1 on QA-FAIL). Never activates the
# app or moves the real cursor. Usage: scripts/qa-smoke.sh [path-to-binary]
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
BIN=${1:-$ROOT/target/debug/ImageOptim-Rust}
DIR=$(mktemp -d)
CFG_DIR=$(mktemp -d)
export IMAGEOPTIM_RUST_CONFIG_DIR="$CFG_DIR"
FIX=$ROOT/crates/core/tests/fixtures/unoptimized.png
cp "$FIX" "$DIR/a.png"; cp "$FIX" "$DIR/b.png"; cp "$FIX" "$DIR/c.png"
STATE="$CFG_DIR/ui-state.json"
# Deterministic column layout: default order, Best tool visible (geometry the
# step coordinates below assume), no saved window frame.
cat > "$STATE" <<'JSON'
{"show_size":true,"show_savings":true,"show_original_size":false,"show_best_tool":true,"col_original":85,"col_best":85}
JSON
restore() {
    rm -rf "$DIR" "$CFG_DIR"
}
trap restore EXIT

run() { # $1=input steps $2=expect substrings $3=label
    LOG=$(mktemp)
    IMAGEOPTIM_QUIET=1 IMAGEOPTIM_STAY=1 IMAGEOPTIM_QA_SORT=1 \
    IMAGEOPTIM_QA_INPUT="$1" IMAGEOPTIM_QA_EXPECT="$2" \
        "$BIN" "$DIR" >"$LOG" 2>&1 &
    PID=$!
    i=0
    while [ $i -lt 60 ]; do
        if grep -q 'QA-PASS\|QA-FAIL' "$LOG"; then break; fi
        sleep 0.5; i=$((i+1))
    done
    kill $PID 2>/dev/null || true
    wait $PID 2>/dev/null || true
    if grep -q 'QA-PASS' "$LOG"; then
        echo "PASS: $3"
    else
        echo "FAIL: $3"; grep -E 'QA-' "$LOG" | head -5; rm -f "$LOG"; exit 1
    fi
    rm -f "$LOG"
}

run "wait:4;move:264,11;wait:6" "cursor=ResizeColumn" "resize cursor on column boundary"
run "wait:4;down:264,11;wait:2;move:294,11;wait:2;move:324,11;wait:2;up:324,11;wait:5" "file=274" "column resize drag"
run "wait:4;down:154,11;wait:2;move:300,11;wait:2;move:400,11;wait:2;up:400,11;wait:4;down:100,32;wait:2;move:100,52;wait:2;move:100,70;wait:2;up:100,70;wait:5" 'order=["status", "size", "savings", "file", "original", "best"];rows=["b.png", "c.png", "a.png"]' "column + row reorder drags"
echo "qa-smoke: all pass"
