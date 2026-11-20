# Contributing

## Checks

`nix develop` or direnv provides every tool, and `just` lists the commands of the project. CI runs the commands in `.github/workflows/ci.yml`; run the ones for the part you changed before you commit, for example with `just check`, `just python`, `just swift-test` or `just kotlin`, and `just examples` after a change to what the library offers. The Python package has its own in `packages/python/README.md`.

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
| `kotlin` | `packages/kotlin` |
| `ci` | a fix that only CI needed |
| `deps` | dependency updates |

A change that touches several parts takes the scope of the part it is for, and a change to the repository as a whole has none. Split a commit that serves two purposes.

A breaking change gets a `!` after the scope and a `BREAKING CHANGE:` paragraph at the end of the body that says what users have to change:

```
feat(core)!: name the ends of a trips query origin and destination

BREAKING CHANGE: the fields from and to of TripsQuery are now origin
and destination.
```

## Pull requests

A squash turns the title and the description of a pull request into the commit message, so the title follows the format above. CI checks it, and it checks that a pull request that breaks the Rust API has a `!` in its title and in one of its commits.

## Changelog

`CHANGELOG.md` is generated from the commit messages with [git-cliff](https://git-cliff.org) and edited by hand before a release. `git cliff --unreleased` shows what the next release would contain.

## Releases

The crates and the Python, Swift and Kotlin packages share one version.

1. `just release` sets the next version, which follows from the commits, and writes the new section into `CHANGELOG.md`. `just release 0.2.0` sets another version. Edit the section, commit the result as `chore: release 0.2.0` and push to `main`.
2. Start the workflow "Release" on `main` with "Publish". It builds and tests everything, tags the release, and publishes to crates.io, PyPI, Maven Central, GitHub releases and the documentation site.

Without "Publish" the workflow only builds and tests, on any branch.

crates.io and PyPI trust the workflow itself. Maven Central needs secrets in the `release` environment: a user token of the Central Portal as `MAVEN_CENTRAL_USERNAME` and `MAVEN_CENTRAL_PASSWORD`, and the armored GPG key with its passphrase as `SIGNING_KEY` and `SIGNING_PASSWORD`.

If a job fails after the tag exists, run the failed jobs of that run again: each skips what is already published. A new run cannot finish the release.
