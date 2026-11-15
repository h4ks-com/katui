# Building, testing and publishing

katui ships as Rust crates on crates.io, as the `katui` Python package
on PyPI, as the `katui` npm package, as a C library attached to each
GitHub release, and as a Go module. All of them come from the same commit
and carry the same version.

## The version

The workspace `Cargo.toml` holds the version. Every crate inherits it, and
the Python package reads it when maturin builds it. `just version` prints
it. The internal dependencies in `[workspace.dependencies]` repeat it,
because crates.io needs a version on each, and `just release-check`
fails when one of them differs. `bindings/node/package.json` repeats it
too, and the Node sync test fails when it differs.

## The gates

`just ci` runs every check, and the CI workflow runs the same recipes on
every push and pull request. Run it before you commit.

| Recipe | What it checks |
|---|---|
| `just fmt-check` | rustfmt, over the workspace and the IRC demo |
| `just clippy` | clippy with warnings as errors |
| `just test` | the Rust tests, the code in the guides, and the IRC demo |
| `just docs` | rustdoc with warnings as errors |
| `just quality` | complexity and duplication in every language, and a fresh `bindings/rust-api.json` |
| `just py-check` | ruff, strict mypy and ty, the stub against the module, and the Python tests |
| `just c-check` | a fresh `katui.h`, clang-tidy, the C examples and tests, the C docs and the C sync check |
| `just go-check` | gofmt, go vet, golangci-lint, and the Go tests, docs and sync check |
| `just node-check` | biome, strict tsc, fresh generated types, and the Node tests |

Every language meets the same limits. lizard caps each function at a
cyclomatic complexity of 15, 100 lines and 8 parameters (`just
complexity`), and jscpd fails above 2% copy-pasted code (`just dupes`).
Each language's own linter adds its rules on top: clippy's cognitive
complexity ceiling of 20 for Rust, ruff's mccabe and pylint rules for
Python, golangci-lint's gocyclo, gocognit and dupl for Go, biome's
cognitive complexity for TypeScript, and clang-tidy for C. The prek hooks
(`just hooks`) run the fast ones on every commit: rustfmt, clippy,
complexity, duplication, the API manifest, ruff, biome and gofmt.

CI runs `just py-check` on Linux x86_64, Linux arm64 and macOS, with the
oldest and the newest Python the package supports, `just c-check`,
`just go-check`, and `just node-check` with Node 22, the oldest LTS the
package supports, and Node 26, all on the same three machines. It also
runs `just crates-check`, which packages every crate and compiles it the
way crates.io will.

`just c-check` needs `cbindgen`, `clang-tidy` and a C compiler, `just
go-check` needs Go and a C compiler, `just node-check` needs Node 22.18 or
newer, and `just quality` needs uv and Node.

## Typed messages

Python and TypeScript check messages statically, the way Rust does: an app
names its message type (`App[Message]`, `App<Message>`), every element
carries the type of what it sends, and a widget whose message is not that
type fails mypy, ty or tsc. `tests/test_types.py` and `tests/types.test.ts`
hold lines that must fail to type-check; mypy and ty fail on an unneeded
`# type: ignore` and tsc on an unneeded `@ts-expect-error`, so a type that
loosens breaks the build. The Python stub may hold no `Any`, the Node test
fails when a generated declaration that carries messages has no typed
counterpart in `katui.d.ts`, and the runnable code in the Python and Node
guides is type-checked too. `just py-lint` and `just node-lint` run the
checkers against the committed stub and declarations without a build, so
the prek hooks run them on every commit.

## The Rust API manifest

`bindings/rust-api.json` lists the public Rust API every binding covers:
the widget, game and content exports, the public methods of the wrapped
types, the theme's fields, the app and game hooks, the `Control` and
`Edit` variants, and the version. `scripts/rust-api.py` reads it out of the
crate sources; `just api` writes the file and `just api-check` fails while
the committed one is stale. Every binding's sync check compares its names
against this file, so a change to the Rust API shows up as one diff of it,
and each binding fails until it follows.

## Keeping Python in step with Rust

The Python package wraps the Rust API, so a change on the Rust side can
leave it behind. These checks in `just py-check` catch that:

- **The sync test** reads the API manifest. Every public widget, game
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
Logic the three bindings share, like the app bridge or the headless
screen, lives once in `bindings/core`, the internal `katui-bindings`
crate, so one change there applies to Python, Node and C together.

## Keeping C and Go in step with Rust

The C library in `bindings/c` is a C ABI over the Rust crates, and the Go
module in `bindings/go` wraps that C ABI with cgo. The same checks guard
them:

- **The header** `bindings/c/include/katui.h` is generated from
  `bindings/c/src` by cbindgen and committed. `just c-header` writes it
  again, and `just c-check` fails while the committed one is stale.
- **The C sync check**, `bindings/c/tests/sync.py`, reads the API manifest
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

## Keeping Node in step with Rust

The Node package wraps the same API, and `just node-check` catches the
same drift:

- **The sync test** in `bindings/node/tests/sync.test.ts` reads the API
  manifest the same way the Python one does. Bind a new public item in
  `bindings/node/src`, or add it to `OMITTED` there with the reason Node
  does without it. A JavaScript name that is not the camelCase of the Rust
  one goes in `RENAMED`.
- **The generated types.** napi writes `index.js` and `index.d.ts` from the
  Rust code on every build. Both are committed, and the check fails when the
  build changes them, so commit them with the Rust change.
  `bindings/node/index.header.d.ts` holds the shared types napi puts at the
  top of `index.d.ts`, and `katui.d.ts` types the JavaScript layer in
  `katui.js`.
- **tsc** checks the examples and the tests in strict mode.
- **The docs test** parses every TypeScript block in `docs/node.md` and the
  package README, and type-checks and runs each one that starts an app or a
  game.
- **The example tests** run every example in `bindings/node/examples`
  without a terminal, and save their screenshots in `target/node-shots`.

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

`just node-build` builds the Node package in release into `bindings/node`,
and `just node-demo widgets` runs an example with it.

## Releasing

1. Set the new version in the workspace `Cargo.toml`, in
   `[workspace.package]` and on every internal dependency, and in
   `bindings/node/package.json`.
2. Run `just ci` and commit.
3. Tag the commit with the version, like `v0.2.0`, and push the tag.

The release workflow then:

1. checks that the tag names the workspace version (`just release-check`),
   and runs `just ci`;
2. builds wheels for Linux x86_64 and arm64 (glibc and musl) and macOS
   Apple Silicon and Intel, and builds the source package, which carries
   `bindings/core` with the Python crate, and checks that it compiles a
   working module (`just py-test-sdist`);
3. installs the glibc and macOS wheels on matching machines and runs the
   Python tests against them (`just py-test-wheel`); the musl wheels ship
   without that test;
4. publishes the wheels and the source package to PyPI, and every crate
   to crates.io in dependency order (`just crates-publish`);
5. builds the native Node module for the same six platforms (`just
   node-artifact`), packs one npm package per module and the main `katui`
   package (`just node-pack`), installs the tarballs on matching glibc and
   macOS machines and runs the Node tests against them (`just
   node-test-pack`), and publishes them to npm with provenance (`just
   node-publish`). The glibc modules link against glibc 2.17, and the musl
   ones are cross-compiled with zig and ship without that test;
6. builds the C archive for Linux x86_64 and arm64 and macOS Apple
   Silicon and Intel (`just c-dist`), each holding `include/katui.h` and
   `lib/` with `libkatui.a` and the shared library, and attaches them to
   the GitHub release of the tag;
7. publishes the Go module (`just go-release` and `just go-publish`).

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
libraries, built with the `dist` profile (fat LTO, no debug info), add
about 56 MB to the module.

The Linux arm64 runners are free for public repositories only, so the
repository has to be public before the first release.

The wheels use the stable ABI, so one wheel per platform works on every
Python from 3.14 on.

## One-time setup

The release workflow publishes to PyPI, crates.io and npm with trusted
publishing: each job trades a short-lived GitHub OIDC token for a
publish token, so the repository holds no registry secret. Before the
first release:

1. On PyPI, add a pending trusted publisher for the project `katui`: owner
   `h4ks-com`, repository `katui`, workflow `release.yml`,
   environment `pypi`.
2. In the repository settings on GitHub, create an environment named
   `pypi`. Add yourself as a required reviewer to approve each release
   before it publishes.
3. crates.io sets up trusted publishing per crate, and only for a crate
   that exists. Publish the first version by hand from a clean checkout of
   the release tag: `cargo login` with a token that has the publish-new
   scope, then `just crates-publish`, then revoke the token. On crates.io,
   in the settings of each of the 13 crates, add a trusted publisher:
   repository `h4ks-com/katui`, workflow `release.yml`, environment
   `crates-io`. Create a GitHub environment named `crates-io` and add
   yourself as a required reviewer. On that first release, reject the
   `crates` job's deployment, since the crates already went up by hand.
4. Let GitHub Actions write to the repository: the C release job creates
   the GitHub release, and the Go release job pushes the
   `bindings/go/vX.Y.Z` tag. A tag protection rule or ruleset that covers
   `bindings/go/v*` has to allow GitHub Actions.
5. npm sets up trusted publishing per package, and only for a package that
   exists. The packages live under the `@mattfillipe` scope, since npm
   refuses the bare name `katui` as too close to the existing `kat-ui`.
   Publish version 0.0.0 of each of `@mattfillipe/katui` and its six
   platform packages, `@mattfillipe/katui-linux-x64-gnu`,
   `-linux-arm64-gnu`, `-linux-x64-musl`, `-linux-arm64-musl`,
   `-darwin-x64` and `-darwin-arm64`, once by hand with `--access public`,
   as a placeholder. Then, on npmjs.com, add a trusted publisher to each of
   them: owner `h4ks-com`, repository `katui`, workflow
   `release.yml`, environment `npm`. The workflow publishes with
   provenance and holds no npm token.
6. Create a GitHub environment named `npm` and add yourself as a required
   reviewer.
