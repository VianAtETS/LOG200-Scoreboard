#!/usr/bin/env bash
# Checks the version bump rule of .github/release.sh, in dry run on a throwaway repository.
# Run from the repository root (CI: the lint job; locally: in a container, see docs/spec-analyse-isola.md).
set -euo pipefail

root=$(pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# Repository at tag $1 (version of Cargo.toml: $2), plus one commit per message; prints the tag released.
releases() {
  local tag=$1 version=$2
  shift 2
  rm -rf "$tmp/repo"
  git init -q "$tmp/repo"
  cd "$tmp/repo"
  git config user.name test
  git config user.email test@example.com
  git config core.autocrlf false
  printf '[workspace.package]\nversion = "%s"\n' "${tag#v}" > Cargo.toml
  git add Cargo.toml
  git commit -qm 'feat: init'
  git tag "$tag"
  printf '[workspace.package]\nversion = "%s"\n' "$version" > Cargo.toml
  for message in "$@"; do git commit -qam "$message" --allow-empty; done
  DRY_RUN=1 bash "$root/.github/release.sh" | sed -n 's/^+ git tag \(v[^ ]*\).*/\1/p; s/ is already released$//p'
  cd "$root"
}

expect() {
  local want=$1
  shift
  local got
  got=$(releases "$@")
  [ "$got" = "$want" ] || { echo "release of $*: got '$got', want '$want'" >&2; exit 1; }
}

expect v0.35.1 v0.35.1 0.35.1 'docs: explain'
expect v0.35.1 v0.35.1 0.35.1 'ci(ladder): stop waiting' 'chore: tidy'
expect v0.35.2 v0.35.1 0.35.1 'fix(web): repair'
expect v0.35.2 v0.35.1 0.35.1 'perf(web): faster graph'
expect v0.36.0 v0.35.1 0.35.1 'feat(web): add page' 'fix: repair'
expect v0.36.0 v0.35.1 0.35.1 "feat(web) : refonte de l'interface (#16)"
expect v0.36.0 v0.35.1 0.35.1 'feat(ladder)!: new schema'
expect v0.36.0 v0.35.1 0.35.1 $'fix: repair\n\nBREAKING CHANGE: new format'
expect v0.40.0 v0.35.1 0.40.0 'feat: add'
expect v1.3.0 v1.2.3 1.2.3 'feat: add' 'fix: repair'
expect v2.0.0 v1.2.3 1.2.3 'fix!: drop route'

echo "release: ok"
