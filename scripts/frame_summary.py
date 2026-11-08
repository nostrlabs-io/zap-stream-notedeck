import re
import sys

log = open(sys.argv[1], encoding="utf-8", errors="replace").read().splitlines()
skip = int(sys.argv[2]) if len(sys.argv) > 2 else 2
for kind in ("frame_stats", "work_stats"):
    rows = [dict(re.findall(r"(\w+)=([\d.]+)", l)) for l in log if f"{kind} frames=" in l][skip:]
    if not rows:
        continue
    col = lambda k: [float(r[k]) for r in rows]
    print(
        f"{kind}: windows={len(rows)} frames/5s={sum(col('frames'))/len(rows):.0f} "
        f"p50={sum(col('p50'))/len(rows):.2f} p95(mean)={sum(col('p95'))/len(rows):.2f} "
        f"p95(worst)={max(col('p95')):.2f} p99(worst)={max(col('p99')):.2f} max={max(col('max')):.2f}"
    )
slow = [l.split("slow_section ")[1] for l in log if "slow_section" in l]
print(f"slow_sections={len(slow)} {' '.join(slow[:12])}")
