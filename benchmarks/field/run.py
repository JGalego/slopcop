#!/usr/bin/env python3
"""Scan the pinned field-test projects and compare detections with the baseline.

    run.py check   [--only NAME ...]   fail when detections differ from the baseline
    run.py update  [--only NAME ...]   rewrite the baseline from the current detections

Every project is pinned to a commit, so a difference always comes from slopcop.
"""

import argparse
import hashlib
import json
import subprocess
import sys
import tomllib
from collections import Counter
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
MANIFEST = HERE / "projects.toml"
BASELINE = HERE / "baseline.json"


def fetch(project, cache):
    """Check out the pinned commit into the cache, reusing a matching checkout."""
    dest = cache / project["name"].replace(" ", "-")
    sha = project["commit"]
    if (dest / ".git").exists():
        head = subprocess.run(
            ["git", "-C", str(dest), "rev-parse", "HEAD"], capture_output=True, text=True
        )
        if head.stdout.strip() == sha:
            return dest
        subprocess.run(["rm", "-rf", str(dest)], check=True)
    dest.mkdir(parents=True)
    for args in (
        ["init", "--quiet"],
        ["remote", "add", "origin", project["url"]],
        ["fetch", "--quiet", "--depth", "1", "origin", sha],
        ["checkout", "--quiet", "--detach", "FETCH_HEAD"],
    ):
        subprocess.run(["git", "-C", str(dest), *args], check=True)
    return dest


def scan(slopcop, checkout):
    """Run slopcop from inside the checkout so finding paths are repository-relative."""
    result = subprocess.run(
        [slopcop, ".", "--format", "json"], cwd=checkout, capture_output=True, text=True
    )
    # Exit 1 means findings were reported; anything else is a failure to scan.
    if result.returncode not in (0, 1):
        raise RuntimeError(f"slopcop exited {result.returncode}: {result.stderr.strip()}")
    return json.loads(result.stdout)


def summarize(report):
    findings = report["findings"]
    keys = sorted(
        f"{f['path']}:{f['location']['line']}:{f['rule_id']}" for f in findings
    )
    return {
        "scanned_files": report["summary"]["scanned_files"],
        "findings": len(findings),
        "rules": dict(sorted(Counter(f["rule_id"] for f in findings).items())),
        "digest": hashlib.sha256("\n".join(keys).encode()).hexdigest()[:16],
    }, keys


def describe_difference(old, new):
    lines = []
    for rule in sorted(set(old["rules"]) | set(new["rules"])):
        before, after = old["rules"].get(rule, 0), new["rules"].get(rule, 0)
        if before != after:
            lines.append(f"    {rule}: {before} -> {after} ({after - before:+d})")
    if old["scanned_files"] != new["scanned_files"]:
        lines.append(f"    scanned files: {old['scanned_files']} -> {new['scanned_files']}")
    if not lines:
        lines.append("    same counts per rule, but findings moved or swapped (see the artifact)")
    return lines


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("mode", choices=["check", "update"])
    parser.add_argument("--slopcop", default=str(ROOT / "target/release/slopcop"))
    parser.add_argument("--cache", type=Path, default=ROOT / "target/field/checkouts")
    parser.add_argument("--out", type=Path, default=ROOT / "target/field/findings")
    parser.add_argument("--only", nargs="*", default=[], help="project names to scan")
    args = parser.parse_args()

    projects = tomllib.loads(MANIFEST.read_text())["project"]
    names = {p["name"] for p in projects}
    unknown = set(args.only) - names
    if unknown:
        sys.exit(f"unknown project(s): {', '.join(sorted(unknown))}")
    selected = [p for p in projects if not args.only or p["name"] in args.only]

    args.cache.mkdir(parents=True, exist_ok=True)
    args.out.mkdir(parents=True, exist_ok=True)
    baseline = json.loads(BASELINE.read_text()) if BASELINE.exists() else {}
    current, errors, changed = dict(baseline), [], []

    for project in selected:
        name = project["name"]
        try:
            report = scan(args.slopcop, fetch(project, args.cache))
        except (subprocess.CalledProcessError, RuntimeError) as error:
            errors.append(f"{name}: {error}")
            continue
        summary, keys = summarize(report)
        summary["commit"] = project["commit"]
        current[name] = summary
        (args.out / f"{name.replace(' ', '-')}.json").write_text(
            json.dumps(report["findings"], indent=1)
        )
        old = baseline.get(name)
        if old is None:
            changed.append(f"  {name}: not in the baseline ({summary['findings']} findings)")
        elif old != summary:
            changed.append(f"  {name}: {old['findings']} -> {summary['findings']} findings")
            changed += describe_difference(old, summary)
        else:
            print(f"ok       {name} ({summary['findings']} findings)")

    for name in list(current):
        if name not in names:
            del current[name]
            changed.append(f"  {name}: removed from the manifest")

    for error in errors:
        print(f"error    {error}", file=sys.stderr)

    if args.mode == "update":
        if errors:
            sys.exit("not updating the baseline while scans are failing")
        BASELINE.write_text(json.dumps(dict(sorted(current.items())), indent=2) + "\n")
        print(f"baseline written for {len(selected)} project(s)")
        return

    if changed:
        print("Detections differ from the baseline:")
        print("\n".join(changed))
        print("If the change is intended, run `make field-baseline` and commit the result.")
    sys.exit(2 if errors else 1 if changed else 0)


if __name__ == "__main__":
    main()
