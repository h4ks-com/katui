# Building, testing and publishing

katui ships as Rust crates on crates.io, as the `katui` Python package
on PyPI, as a C library attached to each GitHub release, and as a Go
module. All of them come from the same commit and carry the same version.

## The version

The workspace `Cargo.toml` holds the version. Every crate inherits it, and
the Python package reads it when maturin builds it. `just version` prints
it. The internal dependencies in `[workspace.dependencies]` repeat it,
because crates.io needs a version on each, and `just release-check`
fails when one of them differs.

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
| `just c-check` | a fresh `katui.h`, the C examples and tests, the C docs and the C sync check |
| `just go-check` | go vet, gofmt, and the Go tests, docs and sync check |

CI runs `just py-check` on Linux x86_64, Linux arm64 and macOS, with the
oldest and the newest Python the package supports, `just c-check` and
`just go-check` on the same three machines, and `just crates-check`,
which packages every crate and compiles it the way crates.io will.

`just c-check` needs `cbindgen` and a C compiler, and `just go-check`
needs Go and a C compiler.

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

## Keeping C and Go in step with Rust

The C library in `bindings/c` is a C ABI over the Rust crates, and the Go
module in `bindings/go` wraps that C ABI with cgo. The same checks guard
them:

- **The header** `bindings/c/include/katui.h` is generated from
  `bindings/c/src` by cbindgen and committed. `just c-header` writes it
  again, and `just c-check` fails while the committed one is stale.
- **The C sync check**, `bindings/c/tests/sync.py`, reads the Rust sources
  and the header. Every public widget, game item, type method, theme
  field, app and game hook, and `Control` and `Edit` variant needs a C
  function, type or constant. Bind it in `bindings/c/src`, or add it to
  `OMITTED` with the reason C does without it.
- **The Go sync test**, `bindings/go/sync_test.go`, does the same against
  the Go package's exported names, with its own `omitted` and `renamed`
  maps.
- **The docs tests** build every C block in `docs/c.md` and the C README,
  and every Go block in `docs/go.md` and the Go README, and run each one
  that starts an app or a game, with one frame drawn off-terminal.
- **The example tests** run the C and Go examples without a terminal and
  save their screenshots in `target/c-shots` and `target/go-shots`.

A change to the Rust API is finished when the C binding and the Go
wrapper follow it too.

## Building locally

`just py-build` builds the package in release into
`bindings/python/.venv`, and `just py-demo widgets` runs an example with
it. `just py-dist` builds a wheel for this machine and the source package
into `target/wheels`.

`just c-build` builds the C library in release into `target/release`, and
`just c-demo todo` runs a C example. `just go-lib` copies the header and
this machine's static library into `bindings/go/include` and
`bindings/go/lib/<os>_<arch>`, which git ignores, and `just go-demo todo`
runs a Go example. Inside the repository, the Go recipes always build the
library from source first.

## Releasing

1. Set the new version in the workspace `Cargo.toml`, in
   `[workspace.package]` and on every internal dependency.
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
4. publishes the wheels and the source package to PyPI, and every crate
   to crates.io in dependency order (`just crates-publish`);
5. builds the C archive for Linux x86_64 and arm64 and macOS Apple
   Silicon and Intel (`just c-dist`), each holding `include/katui.h` and
   `lib/` with `libkatui.a` and the shared library, and attaches them to
   the GitHub release of the tag;
6. publishes the Go module (`just go-release` and `just go-publish`).

## How the Go module ships

`go get` downloads the module's directory at a tag and builds it with
cgo, so the static library has to be inside the module. The main branch
holds no binaries. For each release, the workflow takes the static
libraries out of the four C archives into `bindings/go/lib/<os>_<arch>`,
copies the header into `bindings/go/include`, commits them on a commit off
the release tag that no branch contains, and pushes the tag
`bindings/go/vX.Y.Z`. The Go proxy fetches the module at that tag, and the
cgo flags in `bindings/go/katui.go` link the library for the platform
being built. Users need Go and a C compiler, and no Rust. The four static
libraries add about 180 MB to the module.

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
3. Create a crates.io API token with the publish-new and publish-update
   scopes, and a GitHub environment named `crates-io` with the token as
   its secret `CARGO_REGISTRY_TOKEN`. Add yourself as a required reviewer
   there too.
4. Let GitHub Actions write to the repository: the C release job creates
   the GitHub release, and the Go release job pushes the
   `bindings/go/vX.Y.Z` tag. A tag protection rule or ruleset that covers
   `bindings/go/v*` has to allow GitHub Actions.
