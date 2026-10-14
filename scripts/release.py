#!/usr/bin/env python3
"""Create and validate release archives using Python 3.11+ and the standard library."""

import argparse
import gzip
import hashlib
from pathlib import Path
import re
import subprocess
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parent.parent
TARGETS = (
    "aarch64-apple-darwin",
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
)
MEMBERS = {"git-tui", "README.md", "LICENSE"}


def package_version():
    with (ROOT / "Cargo.toml").open("rb") as manifest:
        version = tomllib.load(manifest)["package"]["version"]
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", version):
        raise ValueError(f"Unsupported release version: {version}")
    return version


def check_tag(tag, version):
    if tag != f"v{version}":
        raise ValueError(f"Tag {tag!r} must match Cargo.toml version v{version}")


def archive_name(version, target):
    return f"git-tui-v{version}-{target}.tar.gz"


def checksum(path):
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    return f"{digest}  {path.name}\n"


def package(binary, target, output, version):
    reported = subprocess.check_output([str(binary.resolve()), "--version"], text=True).strip()
    if reported != f"git-tui {version}":
        raise ValueError(f"Binary version mismatch: {reported!r}, expected git-tui {version}")
    output.mkdir(parents=True, exist_ok=True)
    archive = output / archive_name(version, target)
    # Normalize metadata so packaging does not expose the build user's identity.
    with archive.open("wb") as raw, gzip.GzipFile(fileobj=raw, filename="", mode="wb", mtime=0) as zipped:
        with tarfile.open(fileobj=zipped, mode="w") as bundle:
            for name, source in (("git-tui", binary), ("README.md", ROOT / "README.md"), ("LICENSE", ROOT / "LICENSE")):
                info = bundle.gettarinfo(str(source), arcname=name)
                info.uid = info.gid = info.mtime = 0
                info.uname = info.gname = ""
                info.mode = 0o755 if name == "git-tui" else 0o644
                with source.open("rb") as stream:
                    bundle.addfile(info, stream)
    archive.with_name(archive.name + ".sha256").write_text(checksum(archive), encoding="utf-8")
    print(archive)


def collect(output, version):
    expected = {archive_name(version, target) for target in TARGETS}
    actual = {path.name for path in output.glob("*.tar.gz")}
    if actual != expected:
        raise ValueError(f"Release assets mismatch: missing={expected - actual}, unexpected={actual - expected}")
    lines = []
    for name in sorted(expected):
        archive = output / name
        line = checksum(archive)
        recorded = archive.with_name(name + ".sha256").read_text(encoding="utf-8")
        if recorded != line:
            raise ValueError(f"Checksum mismatch: {name}")
        with tarfile.open(archive, "r:gz") as bundle:
            members = bundle.getmembers()
            if len(members) != len(MEMBERS) or {member.name for member in members} != MEMBERS:
                raise ValueError(f"Unexpected archive contents: {name}")
            if not all(member.isfile() for member in members):
                raise ValueError(f"Archive must contain regular files only: {name}")
            if bundle.getmember("git-tui").mode != 0o755:
                raise ValueError(f"Binary is not executable: {name}")
        lines.append(line)
    (output / "SHA256SUMS").write_text("".join(lines), encoding="utf-8")
    print("".join(lines), end="")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    tag_parser = commands.add_parser("check-tag")
    tag_parser.add_argument("tag")
    package_parser = commands.add_parser("package")
    package_parser.add_argument("--target", required=True, choices=TARGETS)
    package_parser.add_argument("--binary", required=True, type=Path)
    package_parser.add_argument("--output", type=Path, default=Path("dist"))
    collect_parser = commands.add_parser("collect")
    collect_parser.add_argument("--output", type=Path, default=Path("dist"))
    args = parser.parse_args()
    version = package_version()
    try:
        if args.command == "check-tag":
            check_tag(args.tag, version)
        elif args.command == "package":
            package(args.binary, args.target, args.output, version)
        else:
            collect(args.output, version)
    except (OSError, ValueError, tarfile.TarError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"release: {error}\n")


if __name__ == "__main__":
    main()
