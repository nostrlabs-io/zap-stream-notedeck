#!/usr/bin/env python3
import json
import os
import subprocess
import sys

RELAY = os.environ.get("STORE_SHOTS_RELAY", "wss://relay.zap.stream")
PROFILE_RELAYS = ["wss://relay.damus.io", "wss://relay.primal.net", "wss://relay.zap.stream", "wss://nos.lol"]
BLOCKED_WORDS = {"nsfw", "adult", "18+", "porn", "sex", "nude", "casino", "bet"}
SAFE_SYMBOLS = {0x26A1}


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


def host_of(event):
    t = tags(event)
    return next((x[0] for x in t.get("p", []) if len(x) > 2 and x[2] == "host"), event["pubkey"])


def naddr(event):
    d = tags(event)["d"][0][0]
    return subprocess.run(
        ["nak", "encode", "naddr", "-q", "-k", "30311", "-a", event["pubkey"], "-d", d],
        capture_output=True,
        check=True,
    ).stdout.decode().strip()


def plain_bio(host):
    out = subprocess.run(
        ["nak", "req", "-k", "0", "-a", host, "-l", "1", *PROFILE_RELAYS],
        capture_output=True,
        timeout=40,
    ).stdout.decode()
    for line in out.splitlines():
        try:
            meta = json.loads(json.loads(line)["content"])
        except ValueError:
            continue
        text = (meta.get("about") or "") + (meta.get("name") or "") + (meta.get("display_name") or "")
        if "content-warning" in text.lower():
            return False
        return not any(0x2190 <= ord(c) <= 0x2BFF and ord(c) not in SAFE_SYMBOLS for c in text)
    return False


def viewers(event):
    return int(tags(event).get("current_participants", [["0"]])[0][0] or 0)


def main():
    events = [e for e in fetch() if safe(e)]
    ranked = sorted(events, key=lambda e: -viewers(e))
    pick = None
    for status in ("live", "ended"):
        pick = next((e for e in ranked if tags(e).get("status", [[""]])[0][0] == status), None)
        if pick:
            break
    if not pick:
        sys.exit(1)
    status = tags(pick)["status"][0][0]
    profile = next(
        (host_of(e) for e in ranked if tags(e).get("status", [[""]])[0][0] == status and plain_bio(host_of(e))),
        host_of(pick),
    )
    print(json.dumps({
        "naddr": naddr(pick),
        "host": host_of(pick),
        "profile": profile,
        "status": status,
        "title": tags(pick).get("title", [[""]])[0][0],
    }))


main()
