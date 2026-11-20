#!/usr/bin/env zsh
# Prepares a release: writes the version into the manifests and their lock
# files, and the changes since the last release into CHANGELOG.md. Run it
# in the development shell (`nix develop`). CONTRIBUTING.md has the steps
# around it.
#
#   scripts/prepare-release.sh, or `just release`
#   scripts/prepare-release.sh 0.2.0, to choose the version
set -euo pipefail
cd "${0:A:h}/.."

# Without a version, the one that follows from the commits since the last
# release, by the rules in cliff.toml.
version=${1:-${$(git cliff --bumped-version)#v}}
if [[ ! $version =~ '^[0-9]+\.[0-9]+\.[0-9]+$' ]]; then
  print -u2 "usage: ${0:t} [<version>], for example 0.2.0"
  exit 2
fi
if [[ -n $(git tag --list v$version) ]]; then
  print -u2 "$version is released. Without a version to set, that means no feature, fix or breaking change came after it."
  exit 1
fi

# The version of the workspace, of the core as the client asks for it, and
# of the Python package.
perl -pi -e 's/^version = ".*"$/version = "'$version'"/' Cargo.toml packages/python/pyproject.toml
perl -pi -e 's/(publictransport = \{ path = "\.\.\/core", version = )"[^"]*"/$1"'$version'"/' crates/client/Cargo.toml
# The version of the TypeScript package.
perl -pi -e 's/^(  "version": )"[^"]*"/$1"'$version'"/' packages/typescript/package.json
# The version in the lines for a Swift manifest.
perl -pi -e 's/(publictransport", from: )"[^"]*"/$1"'$version'"/' README.md packages/swift/README.md
# The version in the lines for a Gradle build.
perl -pi -e 's/(io\.github\.kiliankoe:publictransport:)[^"]*"/${1}'$version'"/' README.md packages/kotlin/README.md
for file in Cargo.toml packages/python/pyproject.toml crates/client/Cargo.toml packages/typescript/package.json README.md packages/swift/README.md packages/kotlin/README.md; do
  if ! grep -q "\(version = \|\"version\": \|from: \|publictransport:\)\"\?$version\"" $file; then
    print -u2 "$file: found no version to set"
    exit 1
  fi
done
cargo update --quiet --workspace
(cd packages/python && uv lock --quiet)

# Fails if the Rust API broke and the version does not say so, which
# happens when a commit with a breaking change lacks its `!`.
if ! cargo semver-checks --workspace; then
  print -u2 "If this reports a break of the Rust API, a commit lacks its '!'."
  print -u2 "Run this again with the version that the break asks for."
  exit 1
fi

if [[ -f CHANGELOG.md ]]; then
  git cliff --unreleased --tag v$version --prepend CHANGELOG.md
  # git-cliff ends the new section without an empty line.
  perl -0pi -e 's/([^\n])\n(## )/$1\n\n$2/g' CHANGELOG.md
else
  git cliff --tag v$version --output CHANGELOG.md
fi
print "Version $version is set. Edit CHANGELOG.md, then commit."
