# Contributing

## Checks

`nix develop` or direnv provides every tool, and `just` lists the commands of the project. CI runs the commands in `.github/workflows/ci.yml`; run the ones for the part you changed before you commit, for example with `just check`, `just python` or `just swift-test`. The Python package has its own in `packages/python/README.md`.

## Commit messages

Commit messages follow [Conventional Commits](https://www.conventionalcommits.org), so that the history shows which part of the project a change belongs to and the changelog can be generated from it. CI checks the format with [committed](https://github.com/crate-ci/committed), configured in `committed.toml`.

```
type(scope): subject in the imperative, lower case, at most 72 characters

A body that says why, and what the diff does not show. Lines wrap at 72
characters.
```

The types:

| Type | For |
| --- | --- |
| `feat` | something a user of the library can newly do |
| `fix` | a correction of wrong behavior |
| `perf` | a change that only makes something faster or smaller |
| `docs` | documentation only |
| `refactor` | a change of structure without a change of behavior |
| `test` | tests and fixtures only |
| `build` | dependencies, the flake, packaging |
| `ci` | the CI workflow |
| `chore` | anything else that does not touch the library |

The scope names the part that changed:

| Scope | Part |
| --- | --- |
| `core` | `crates/core`, the sans-IO library |
| `client` | `crates/client`, the client on reqwest |
| `uniffi` | `bindings/uniffi` and the `uniffi` feature of the core |
| `python` | `packages/python` |
| `swift` | `packages/swift` and `Package.swift` |
| `ci` | a fix that only CI needed |
| `deps` | dependency updates |

A change that touches several parts takes the scope of the part it is for, and a change to the repository as a whole has none. Split a commit that serves two purposes.

A breaking change gets a `!` after the scope and a `BREAKING CHANGE:` paragraph at the end of the body that says what users have to change:

```
feat(core)!: name the ends of a trips query origin and destination

BREAKING CHANGE: the fields from and to of TripsQuery are now origin
and destination.
```

## Changelog

`CHANGELOG.md` is generated from the commit messages with [git-cliff](https://git-cliff.org), configured in `cliff.toml`, and then edited by hand before a release. It lists breaking changes, features, fixes and performance changes; the other types are left out. `git cliff --unreleased` shows what the next release would contain.

The file does not exist before the first release.

## Releases

The crates, the Python package and the Swift package share one version.

1. `just release 0.2.0` writes the version into the manifests and READMEs and the new section into `CHANGELOG.md`. Edit the section, commit the result as `chore: release 0.2.0` and push to `main`.
2. Start the workflow "Release" on `main` with "Publish". It builds the XCFramework, the wheels and the crates and tests them. Then it commits the address and checksum of the XCFramework to `Package.swift`, tags that commit `v0.2.0`, creates the GitHub release with the XCFramework, and publishes to crates.io and PyPI.

Without "Publish" the workflow stops after the builds and tests, on any branch. Use that to try a change to the workflow.

The workflow makes the tag, not the person who releases: the Swift package names the checksum of its binary in `Package.swift`, so the tagged commit can only exist once the binary is built.

crates.io and PyPI trust the workflow `release.yml` in its environment `release`, so no token is stored. If a job fails after the tag exists, run the failed jobs of that run again: each skips what is already published. A new run cannot finish the release, because it would build a binary with another checksum.
