#!/usr/bin/env bash
set -euo pipefail
# usage: scripts/desktop-perf.sh <outdir> <seconds> <WxH> [ENV=value ...]
out=$1; secs=$2; res=$3; shift 3
bin=${BIN:-${CARGO_TARGET_DIR:-target}/release/zap_stream_app}
mkdir -p "$out"
data=$(mktemp -d /tmp/zs-perf-data.XXXX)
env_args="ZAP_STREAM_FRAME_STATS=1 XDG_DATA_HOME=$data RUST_LOG=zap_stream_app=info $*"
HSWAY_RES=$res HSWAY_ENV="$env_args" ~/.agents/bin/hsway-res "$secs" "$out" \
  bash -c "$bin & pid=\$!; sleep $((secs - 5)); ps -o rss= -p \$pid > $out/rss.txt; wait \$pid"
rm -rf "$data"
grep -o "first_frame.*" "$out/app.log" | head -1 || true
python3 "$(dirname "$0")/frame_summary.py" "$out/app.log"
echo "rss_kb $(tr -d ' ' < "$out/rss.txt" 2>/dev/null)"
