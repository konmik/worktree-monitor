import json
from pathlib import Path
import subprocess


root = Path(__file__).resolve().parents[1]
metadata = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", "x86_64-pc-windows-msvc"],
    cwd=root,
))
nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
pending = list(metadata["workspace_members"])
reachable = set()
while pending:
    package_id = pending.pop()
    if package_id not in reachable:
        reachable.add(package_id)
        pending.extend(dependency["pkg"] for dependency in nodes[package_id]["deps"])

packages = sorted(
    (package for package in metadata["packages"] if package["id"] in reachable and package["source"]),
    key=lambda package: (package["name"], package["version"]),
)
sections = ["Third-party notices for Worktree Monitor\n\nWindows x64 dependencies, including build and test dependencies.\n"]
for package in packages:
    directory = Path(package["manifest_path"]).parent
    notices = sorted(
        path for path in directory.rglob("*")
        if path.is_file() and path.name.upper().startswith(
            ("LICENSE", "LICENCE", "COPYING", "NOTICE", "UNLICENSE", "PATENTS")
        ) and path.suffix not in (".a", ".rs")
    )
    if not notices or not package.get("license"):
        raise RuntimeError(f"Missing license or notice for {package['name']} {package['version']}")
    sections.append(
        f"\n{'=' * 72}\n{package['name']} {package['version']}\n"
        f"License: {package['license']}\n"
        f"Source: https://crates.io/crates/{package['name']}/{package['version']}\n"
    )
    for notice in notices:
        sections.append(f"\n--- {notice.relative_to(directory).as_posix()} ---\n\n{notice.read_text(encoding='utf-8')}\n")

output = root / "target" / "THIRD-PARTY-NOTICES.txt"
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text("".join(sections), encoding="utf-8", newline="\n")
print(f"Included notices for {len(packages)} dependencies.")
