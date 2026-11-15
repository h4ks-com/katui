default:
    @just --list

# clippy with ci's denials, after every change
check: clippy

fmt:
    cargo fmt --all
    cargo fmt --manifest-path demos/obby-chat/Cargo.toml

fmt-check:
    cargo fmt --all -- --check
    cargo fmt --manifest-path demos/obby-chat/Cargo.toml -- --check

clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo clippy --manifest-path demos/obby-chat/Cargo.toml --all-targets -- -D warnings

# apply clippy's and rustfmt's fixes
fix:
    cargo clippy --workspace --all-targets --all-features --fix --allow-dirty --allow-staged
    cargo fmt --all

test:
    cargo test --workspace --all-features --all-targets
    cargo test --workspace --all-features --doc
    cargo test --manifest-path demos/obby-chat/Cargo.toml

# licenses, advisories and sources, from deny.toml
deny:
    cargo deny check

typos:
    typos

# complexity per function, into target/metrics (cargo install rust-code-analysis-cli)
metrics:
    mkdir -p target/metrics
    rust-code-analysis-cli -m -p crates -l rust -O json -o target/metrics -j 8

# copy-paste duplication, from .jscpd.json
dupes:
    npx --yes jscpd@4 crates/

# unused dependencies
machete:
    cargo machete

# install the git hooks, once per clone
hooks:
    prek install

# rustdoc with warnings as errors
docs:
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items

# the public api reference, as github pages publishes it
api-docs:
    cargo doc --workspace --exclude katui-python --exclude katui-node --no-deps --all-features
    echo '<meta http-equiv="refresh" content="0; url=katui/index.html">' > target/doc/index.html

# every gate, before every commit
ci: fmt-check clippy test docs typos py-check node-check

# the python package, built in release into bindings/python/.venv
[working-directory('bindings/python')]
py-build:
    uv sync --quiet --no-install-project
    uv run --no-sync maturin develop --quiet --release

# python formatting, lints, types, the stub against the module, and the tests
[working-directory('bindings/python')]
py-check: py-build
    uv run --no-sync ruff format --check .
    uv run --no-sync ruff check .
    uv run --no-sync mypy
    uv run --no-sync python -m mypy.stubtest katui._katui --allowlist stubtest-allowlist.txt
    uv run --no-sync pytest

# apply ruff's formatting and fixes to the python code
[working-directory('bindings/python')]
py-fmt:
    uv sync --quiet --no-install-project
    uv run --no-sync ruff format .
    uv run --no-sync ruff check --fix .

# the wheel for this machine and the source package, into target/wheels
[working-directory('bindings/python')]
py-dist:
    uv sync --quiet --no-install-project
    uv run --no-sync maturin build --release --out ../../target/wheels
    uv run --no-sync maturin sdist --out ../../target/wheels

# the node package, built in release into bindings/node with its generated types
[working-directory('bindings/node')]
node-build:
    npm ci --silent
    npx napi build --platform --release --esm

# node types fresh, strict tsc, biome's lints and formatting, and the tests
[working-directory('bindings/node')]
node-check: node-build
    git diff --exit-code -- index.js index.d.ts
    npx tsc -p .
    npx biome ci .
    node --test 'tests/*.test.ts'

# apply biome's formatting and fixes to the node code
[working-directory('bindings/node')]
node-fmt:
    npx biome check --write .

# a node example, like `just node-demo widgets`
[working-directory('bindings/node')]
node-demo name *args: node-build
    node examples/{{name}}.ts {{args}}

# the native module for `target` into bindings/node/artifacts, for a release
[working-directory('bindings/node')]
node-artifact target *flags:
    npm ci --silent
    npx napi build --platform --release --esm --target {{quote(target)}} --output-dir artifacts {{flags}}

# the npm tarballs into target/npm: one per native module in `dist`, then the main package
node-pack dist:
    #!/usr/bin/env bash
    set -euo pipefail
    version=$(just version)
    rm -rf target/npm bindings/node/npm && mkdir -p target/npm/stage
    cd bindings/node
    npm ci --silent
    npx napi create-npm-dirs --npm-dir npm
    npx napi artifacts --output-dir {{quote(absolute_path(dist))}} --npm-dir npm
    cp package.json README.md LICENSE-MIT LICENSE-APACHE katui.js katui.d.ts index.js index.d.ts ../../target/npm/stage/
    for platform in npm/*/; do
        if ls "$platform"*.node >/dev/null 2>&1; then
            npm pack --silent --pack-destination ../../target/npm "./$platform"
            name=$(node -p "require('./${platform}package.json').name")
            (cd ../../target/npm/stage && npm pkg set "optionalDependencies.$name=$version")
        fi
    done
    npm pack --silent --pack-destination ../../target/npm ../../target/npm/stage

# the packed package for this machine, installed like users install it, passes the tests
node-test-pack:
    #!/usr/bin/env bash
    set -euo pipefail
    version=$(just version)
    platform=$(node -p "process.platform + '-' + process.arch + (process.platform === 'linux' ? '-gnu' : '')")
    rm -rf target/node-smoke && mkdir -p target/node-smoke
    cp -R bindings/node/tests bindings/node/examples target/node-smoke/
    cd target/node-smoke
    echo '{"name": "katui-smoke", "private": true, "type": "module"}' > package.json
    npm install --silent --omit=optional "../npm/katui-$version.tgz" "../npm/katui-$platform-$version.tgz"
    node --test tests/api.test.ts tests/examples.test.ts

# publish every tarball in `dist` to npm with provenance, the native modules first
node-publish dist:
    #!/usr/bin/env bash
    set -euo pipefail
    main={{quote(dist)}}/katui-$(just version).tgz
    for tarball in {{quote(dist)}}/katui-*.tgz; do
        if [ "$tarball" != "$main" ]; then
            npx --yes npm@11.6.2 publish "$tarball" --provenance --access public
        fi
    done
    npx --yes npm@11.6.2 publish "$main" --provenance --access public

# the version every package ships, from the workspace Cargo.toml
version:
    @sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1

# fail unless `tag` names the workspace version, like v0.1.0, and every
# internal dependency asks for that version
release-check tag:
    test {{quote(tag)}} = "v$(just version)"
    test -z "$(grep -E '^katui-[a-z]+ = .*version = ' Cargo.toml | grep -v "version = \"$(just version)\"")"

# package and compile every crate the way crates.io will see it
crates-check:
    cargo publish --workspace --dry-run

# publish every crate to crates.io, in dependency order
crates-publish:
    cargo publish --workspace

# the python tests against the wheels in `dist`, installed like users install them
py-test-wheel dist:
    uv venv --clear --python 3.10 target/wheel-venv
    VIRTUAL_ENV=target/wheel-venv uv pip install --no-index --find-links {{quote(dist)}} katui
    VIRTUAL_ENV=target/wheel-venv uv pip install pytest
    target/wheel-venv/bin/pytest bindings/python/tests

# the source package in `dist` builds a module that imports
py-test-sdist dist:
    uv venv --clear --python 3.10 target/sdist-venv
    VIRTUAL_ENV=target/sdist-venv uv pip install {{quote(dist)}}/*.tar.gz
    target/sdist-venv/bin/python -c "import katui, katui.widgets, katui.game; print(katui.__version__)"

# a python example, like `just py-demo widgets`
[working-directory('bindings/python')]
py-demo name *args: py-build
    uv run --no-sync python examples/{{name}}.py {{args}}

# print what katui detects in this terminal
info:
    cargo run -p katui-rt --bin katui-info

# fps, latency and bytes per frame for each pixel strategy
pace:
    cargo run --release -p katui-rt --example pace

# a click counter, the smallest app
demo-counter:
    cargo run --release -p katui --example counter --features app

# a ball the arrow keys steer, the smallest game
demo-glow:
    cargo run --release -p katui --example glow --features game

# a task list with tabs, a text field and a progress bar
demo-todo:
    cargo run --release -p katui-app --example todo

# every widget in one app
demo-widgets:
    cargo run --release -p katui-app --example widgets

# local html pages with links and scrolling
demo-browse page="":
    cargo run --release -p katui-app --example browse -- {{page}}

# every part of the html and css subset on one page
demo-html:
    cargo run --release -p katui-app --example browse -- crates/katui-app/examples/pages/showcase.html

# an image viewer made of ratatui widgets
demo-ratatui *files:
    cargo run --release -p katui-ratatui --features run --example viewer -- {{files}}

# a video file or stream url, or a test pattern without one
demo-video clip="":
    cargo run --release -p katui-game --example video -- {{clip}}

# fly through a nebula and split crystal rocks
demo-shardrift:
    cargo run --release -p katui-game --example shardrift

# stack glowing blocks down a 3d shaft
demo-lumenshaft:
    cargo run --release -p katui-game --example lumenshaft

# a lit cube in software 3d, steered with the arrows
demo-cube:
    cargo run --release -p katui-game --example cube

# a snake that plays itself
demo-snake:
    cargo run --release -p katui-game --example snake

# the bouncing logo
demo-dvd:
    cargo run --release -p katui-game --example dvd

# conway's game of life with glowing cells
demo-life:
    cargo run --release -p katui-game --example life

# an interference plasma
demo-plasma:
    cargo run --release -p katui-game --example plasma

# an irc client
demo-irc:
    cargo run --release --manifest-path demos/obby-chat/Cargo.toml
