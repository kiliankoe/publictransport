#!/usr/bin/env zsh
# Prepares a release: writes the version into the manifests and their lock
# files, and the changes since the last release into CHANGELOG.md. Run it
# in the development shell (`nix develop`). CONTRIBUTING.md has the steps
# around it.
#
#   scripts/prepare-release.sh 0.1.0, or `just release 0.1.0`
set -euo pipefail
cd "${0:A:h}/.."

version=${1:-}
if [[ ! $version =~ '^[0-9]+\.[0-9]+\.[0-9]+$' ]]; then
  print -u2 "usage: ${0:t} <version>, for example 0.1.0"
  exit 2
fi

# The version of the workspace, of the core as the client asks for it, and
# of the Python package.
perl -pi -e 's/^version = ".*"$/version = "'$version'"/' Cargo.toml packages/python/pyproject.toml
perl -pi -e 's/(publictransport = \{ path = "\.\.\/core", version = )"[^"]*"/$1"'$version'"/' crates/client/Cargo.toml
# The version in the lines for a Swift manifest.
perl -pi -e 's/(publictransport", from: )"[^"]*"/$1"'$version'"/' README.md packages/swift/README.md
for file in Cargo.toml packages/python/pyproject.toml crates/client/Cargo.toml README.md packages/swift/README.md; do
  if ! grep -q "\(version = \|from: \)\"$version\"" $file; then
    print -u2 "$file: found no version to set"
    exit 1
  fi
done
cargo update --quiet --workspace
(cd packages/python && uv lock --quiet)

if [[ -f CHANGELOG.md ]]; then
  git cliff --unreleased --tag v$version --prepend CHANGELOG.md
else
  git cliff --tag v$version --output CHANGELOG.md
fi
print "Version $version is set. Edit CHANGELOG.md, then commit."
