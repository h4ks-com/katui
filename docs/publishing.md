# Building, testing and publishing

katui ships as Rust crates, used from this repository with
`cargo add --git`, and as the `katui` Python package on PyPI. Both come
from the same commit and carry the same version.

## The version

The workspace `Cargo.toml` holds the only version number. Every crate
inherits it, and the Python package reads it when maturin builds it.
`just version` prints it.

## The gates

`just ci` runs every check, and the CI workflow runs the same recipes on
every push and pull request. Run it before you commit.

| Recipe | What it checks |
|---|---|
| `just fmt-check` | rustfmt, over the workspace and the IRC demo |
| `just clippy` | clippy with warnings as errors |
| `just test` | the Rust tests, the code in the guides, and the IRC demo |
| `just docs` | rustdoc with warnings as errors |
| `just py-check` | ruff, strict mypy, the stub, and the Python tests |

CI runs `just py-check` on Linux x86_64, Linux arm64 and macOS, with the
oldest and the newest Python the package supports.

## Keeping Python in step with Rust

The Python package wraps the Rust API, so a change on the Rust side can
leave it behind. These checks in `just py-check` catch that:

- **The sync test** reads the Rust sources. Every public widget, game
  item, type method, app and game hook, and `Control` and `Edit` variant
  needs a Python counterpart. When you add a public item in Rust, bind it in
  `bindings/python/src`, or add it to `OMITTED` in
  `bindings/python/tests/test_sync.py` with the reason Python does without
  it. A Python name that differs from the Rust one goes in `RENAMED`.
- **stubtest** compares `python/katui/_katui.pyi` with the compiled
  module, so the type stub can't miss a function, a parameter or a default.
- **The docs test** compiles every Python block in `docs/python.md` and the
  package README, and runs each one that starts an app or a game.
- **The example tests** run every example in `bindings/python/examples`
  without a terminal, and save their screenshots in
  `target/python-shots`.

A change to the Rust API is finished when `just ci` passes, with the
binding, the stub, the Python guide and the examples updated with it.

## Building locally

`just py-build` builds the package in release into
`bindings/python/.venv`, and `just py-demo widgets` runs an example with
it. `just py-dist` builds a wheel for this machine and the source package
into `target/wheels`.

## Releasing

1. Set the new version in the workspace `Cargo.toml`.
2. Run `just ci` and commit.
3. Tag the commit with the version, like `v0.2.0`, and push the tag.

The release workflow then:

1. checks that the tag names the workspace version (`just release-check`),
   and runs `just ci`;
2. builds wheels for Linux x86_64 and arm64 (glibc and musl) and macOS
   Apple Silicon and Intel, and builds the source package and checks that
   it compiles a working module (`just py-test-sdist`);
3. installs the glibc and macOS wheels on matching machines and runs the
   Python tests against them (`just py-test-wheel`); the musl wheels ship
   without that test;
4. publishes everything to PyPI.

The Linux arm64 runners are free for public repositories only, so the
repository has to be public before the first release.

The wheels use the stable ABI, so one wheel per platform works on every
Python from 3.10 on.

## One-time setup

The release workflow publishes with PyPI's trusted publishing, so the
repository holds no PyPI token. Before the first release:

1. On PyPI, add a pending trusted publisher for the project `katui`: owner
   `matheusfillipe`, repository `katui`, workflow `release.yml`,
   environment `pypi`.
2. In the repository settings on GitHub, create an environment named
   `pypi`. Add yourself as a required reviewer to approve each release
   before it publishes.
