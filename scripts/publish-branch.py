"""Publish a local commit to a new remote branch via the GitHub Git data API."""
import base64
import io
import json
import subprocess
import sys
import urllib.request

sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8")

REPO = "Benz-On/visor"


def credential():
    out = subprocess.run(
        ["git", "credential", "fill"],
        input="protocol=https\nhost=github.com\n\n",
        capture_output=True,
        text=True,
        timeout=15,
    )
    return next(
        (line.split("=", 1)[1] for line in out.stdout.splitlines() if line.startswith("password=")),
        None,
    )


TOKEN = credential()
assert TOKEN, "no token"
HDR = {"Authorization": f"Bearer {TOKEN}", "Accept": "application/vnd.github+json"}


def api(path, method="GET", payload=None):
    request = urllib.request.Request(
        f"https://api.github.com{path}",
        method=method,
        headers=HDR,
        data=json.dumps(payload).encode() if payload else None,
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.loads(response.read().decode() or "{}")


def publish(branch, local_ref, base_ref):
    local_sha = subprocess.run(["git", "rev-parse", local_ref], capture_output=True, text=True).stdout.strip()
    base_full = subprocess.run(["git", "rev-parse", base_ref], capture_output=True, text=True).stdout.strip()
    local_tree = subprocess.run(["git", "rev-parse", f"{local_sha}^{{tree}}"], capture_output=True, text=True).stdout.strip()
    base_tree = api(f"/repos/{REPO}/git/commits/{base_full}")["tree"]["sha"]

    diff = subprocess.run(["git", "diff", "--name-only", base_full, local_sha], capture_output=True, text=True)
    files = [name for name in diff.stdout.strip().splitlines() if name]
    print(f"changed files: {len(files)}")

    entries = []
    for path in files:
        stored = subprocess.run(["git", "cat-file", "blob", f"{local_sha}:{path}"], capture_output=True).stdout
        blob = api(
            f"/repos/{REPO}/git/blobs",
            "POST",
            {"content": base64.b64encode(stored).decode(), "encoding": "base64"},
        )
        local_blob = subprocess.run(["git", "rev-parse", f"{local_sha}:{path}"], capture_output=True, text=True).stdout.strip()
        assert blob["sha"] == local_blob, f"blob mismatch: {path}"
        entries.append({"path": path, "mode": "100644", "type": "blob", "sha": blob["sha"]})
    print("all blobs match")

    tree = api(f"/repos/{REPO}/git/trees", "POST", {"base_tree": base_tree, "tree": entries})
    assert tree["sha"] == local_tree, f"tree mismatch: {tree['sha']} vs {local_tree}"
    print(f"tree verified: {tree['sha'][:10]}")

    message = subprocess.run(["git", "log", "-1", "--format=%B", local_sha], capture_output=True, text=True).stdout.strip()
    commit = api(
        f"/repos/{REPO}/git/commits",
        "POST",
        {"message": message, "tree": tree["sha"], "parents": [base_full]},
    )
    print(f"commit: {commit['sha'][:10]}")

    api(f"/repos/{REPO}/git/refs", "POST", {"ref": f"refs/heads/{branch}", "sha": commit["sha"]})
    print(f"branch created: {branch}")
    return commit["sha"]


if __name__ == "__main__":
    branch = sys.argv[1]
    local_ref = sys.argv[2]
    base_ref = sys.argv[3] if len(sys.argv) > 3 else "origin/main"
    publish(branch, local_ref, base_ref)
