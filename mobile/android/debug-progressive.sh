#!/usr/bin/env bash
set -euo pipefail

ADB="${ADB:-$HOME/Library/Android/sdk/platform-tools/adb}"
PKG="com.jxlui"
ACT=".MainActivity"
SAMPLE="${1:-progressive_5.jxl}"
CHUNK="${CHUNK:-0.5}"
DELAY="${DELAY:-20}"

if [[ ! -x "$ADB" ]]; then
  echo "adb not found at: $ADB" >&2
  exit 1
fi

echo "Launching sample=$SAMPLE chunk_pct=$CHUNK delay_ms=$DELAY"
"$ADB" shell am force-stop "$PKG" || true
"$ADB" logcat -c || true
"$ADB" shell am start -n "$PKG/$ACT" \
  --es sample_name "$SAMPLE" \
  --ez simulate_slow true \
  --ef slow_chunk_pct "$CHUNK" \
  --el slow_delay_ms "$DELAY"

echo
echo "Streaming logs (Ctrl+C to stop):"
"$ADB" logcat -v time | grep -E --line-buffered "JxlIntent|JxlDecode|AndroidRuntime|OutOfMemory|JNI DETECTED"
