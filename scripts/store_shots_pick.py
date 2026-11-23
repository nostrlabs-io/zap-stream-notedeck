#!/usr/bin/env python3
import json
import os
import subprocess
import sys

RELAY = os.environ.get("STORE_SHOTS_RELAY", "wss://relay.zap.stream")
BLOCKED_WORDS = {"nsfw", "adult", "18+", "porn", "sex", "nude", "casino", "bet"}


def fetch():
    out = subprocess.run(
        ["nak", "req", "-k", "30311", "-l", "120", RELAY],
        capture_output=True,
        timeout=60,
    ).stdout.decode()
    events = []
    for line in out.splitlines():
        try:
            events.append(json.loads(line))
        except ValueError:
            continue
    return events


def tags(event):
    found = {}
    for tag in event["tags"]:
        found.setdefault(tag[0], []).append(tag[1:])
    return found


def safe(event):
    t = tags(event)
    if "content-warning" in t or "sensitive" in t:
        return False
    text = " ".join(
        [" ".join(x[0] for x in t.get(k, []) if x) for k in ("title", "summary", "t")]
    ).lower()
    return not any(w in text for w in BLOCKED_WORDS)


def naddr(event):
    d = tags(event)["d"][0][0]
    return subprocess.run(
        ["nak", "encode", "naddr", "-q", "-k", "30311", "-a", event["pubkey"], "-d", d],
        capture_output=True,
        check=True,
    ).stdout.decode().strip()


def main():
    wanted = sys.argv[1:] or ["live", "ended"]
    events = [e for e in fetch() if safe(e)]
    for status in wanted:
        for event in sorted(events, key=lambda e: -int(tags(e).get("current_participants", [["0"]])[0][0] or 0)):
            t = tags(event)
            if t.get("status", [[""]])[0][0] != status:
                continue
            host = next((x[0] for x in t.get("p", []) if len(x) > 2 and x[2] == "host"), event["pubkey"])
            title = t.get("title", [[""]])[0][0]
            print(json.dumps({"naddr": naddr(event), "host": host, "status": status, "title": title}))
            return
    sys.exit(1)


main()
