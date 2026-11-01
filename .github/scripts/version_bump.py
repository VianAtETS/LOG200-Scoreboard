"""Usage: version_bump.py BASE HEAD [PR_TITLE]"""

import re
import subprocess
import sys

VERSION = re.compile(r'^version = "(.+)"$', re.MULTILINE)
BREAKING = re.compile(r"^[a-z]+(\([^)]*\))?!:|^BREAKING[ -]CHANGE:", re.MULTILINE)
FEATURE = re.compile(r"^feat(\([^)]*\))?:", re.MULTILINE)


def git(*args):
    return subprocess.run(["git", *args], check=True, capture_output=True, text=True).stdout


def manifest(rev):
    text = git("show", f"{rev}:Cargo.toml")
    return VERSION.search(text)[1], VERSION.sub("", text)


def lock_without_own_versions(rev):
    return re.sub(r'(name = "isolation-[^"]+"\n)version = "[^"]+"\n', r"\1", git("show", f"{rev}:Cargo.lock"))


def parse(version):
    return tuple(int(part) for part in version.split("-")[0].split("."))


def suggest(old, messages):
    major, minor, patch = parse(old)
    if BREAKING.search(messages):
        return ("minor", f"{major}.{minor + 1}.0") if major == 0 else ("major", f"{major + 1}.0.0")
    if FEATURE.search(messages):
        return "minor", f"{major}.{minor + 1}.0"
    return "patch", f"{major}.{minor}.{patch + 1}"


def main(base, head, title=""):
    base = git("merge-base", base, head).strip()
    old, base_rest = manifest(base)
    new, head_rest = manifest(head)

    rust_changed = (
        subprocess.run(["git", "diff", "--quiet", base, head, "--", "crates", "rust-toolchain.toml"]).returncode != 0
        or base_rest != head_rest
        or lock_without_own_versions(base) != lock_without_own_versions(head)
    )

    if parse(new) < parse(old):
        return f"::error file=Cargo.toml::Version {new} is lower than {old} on the base branch."

    bumped = new != old
    kind, suggested = suggest(old, git("log", "--format=%B", f"{base}..{head}") + "\n" + title)
    if rust_changed and not bumped:
        return (
            f"::error file=Cargo.toml::Rust code changed: bump [workspace.package] version "
            f"(suggested: {suggested}, a {kind} bump from the commit messages), then run cargo check."
        )

    if bumped and not rust_changed:
        return f"::error file=Cargo.toml::No Rust code changed: keep the version at {old}."

    print(f"Version {old} -> {new} (suggested: {suggested})." if bumped else f"No Rust change, version stays {old}.")
    return None


if __name__ == "__main__":
    error = main(*sys.argv[1:])
    if error:
        print(error)
        sys.exit(1)
