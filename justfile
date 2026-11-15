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

# copy-paste duplication in every language, from .jscpd.json
dupes:
    npx --yes jscpd@4 crates/ bindings/ demos/obby-chat/src/ demos/md-viewer/src/

# cyclomatic complexity, length and parameter count of every function, in every language
complexity:
    uvx --from lizard==1.26.0 lizard crates bindings demos/obby-chat/src demos/md-viewer/src \
        -l rust -l python -l go -l typescript -l c \
        -x '*/target/*' -x '*/node_modules/*' -x '*/.venv/*' \
        -x '*/index.js' -x '*/index.d.ts' -x '*/katui.h' \
        -C 15 -L 100 -a 8 -w

# write bindings/rust-api.json, the public rust api every binding covers
api:
    python3 scripts/rust-api.py > target/rust-api.json
    mv target/rust-api.json bindings/rust-api.json

# fail when bindings/rust-api.json no longer matches the rust sources
api-check:
    python3 scripts/rust-api.py | diff -u bindings/rust-api.json -

# the code-quality gates every language shares: complexity, duplication and the api manifest
quality: complexity dupes api-check

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
    cargo doc --workspace --exclude katui-python --exclude katui-node --exclude katui-bindings --no-deps --all-features
    echo '<meta http-equiv="refresh" content="0; url=katui/index.html">' > target/doc/index.html
    python3 scripts/llms.py

# every gate, before every commit
ci: fmt-check clippy test docs typos quality py-check c-check go-check node-check

# the python package, built in release into bindings/python/.venv
[working-directory('bindings/python')]
py-build:
    uv sync --quiet --no-install-project
    uv run --no-sync maturin develop --quiet --release

# ruff, strict mypy and ty on the python code, against the stub, without a build
[working-directory('bindings/python')]
py-lint:
    uv sync --quiet --no-install-project
    uv run --no-sync ruff format --check .
    uv run --no-sync ruff check .
    uv run --no-sync mypy
    uv run --no-sync ty check

# the python lints and types, then the stub against the module, and the tests
[working-directory('bindings/python')]
py-check: py-lint py-build
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

# biome's format and lint checks and strict tsc on the node code, without a build
[working-directory('bindings/node')]
node-lint:
    npm ci --silent
    npx biome ci .
    npx tsc -p .

# the node lints and types, then fresh generated types and the tests
[working-directory('bindings/node')]
node-check: node-lint node-build
    git diff --exit-code -- index.js index.d.ts
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
    npm install --silent --omit=optional "../npm/matheusfillipe-katui-$version.tgz" "../npm/matheusfillipe-katui-$platform-$version.tgz"
    node --test tests/api.test.ts tests/examples.test.ts

# publish every tarball in `dist` to npm with provenance, the native modules first
node-publish dist:
    #!/usr/bin/env bash
    set -euo pipefail
    main={{quote(dist)}}/matheusfillipe-katui-$(just version).tgz
    for tarball in {{quote(dist)}}/matheusfillipe-katui-*.tgz; do
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
    uv venv --clear --python 3.14 target/wheel-venv
    VIRTUAL_ENV=target/wheel-venv uv pip install --no-index --find-links {{quote(dist)}} katui
    VIRTUAL_ENV=target/wheel-venv uv pip install pytest
    target/wheel-venv/bin/pytest bindings/python/tests

# the source package in `dist` builds a module that imports
py-test-sdist dist:
    uv venv --clear --python 3.14 target/sdist-venv
    VIRTUAL_ENV=target/sdist-venv uv pip install {{quote(dist)}}/*.tar.gz
    target/sdist-venv/bin/python -c "import katui, katui.widgets, katui.game; print(katui.__version__)"

# a python example, like `just py-demo widgets`
[working-directory('bindings/python')]
py-demo name *args: py-build
    uv run --no-sync python examples/{{name}}.py {{args}}

c_flags := "-std=c11 -Wall -Wextra -Werror -pedantic -Ibindings/c/include"
c_libs := if os() == "macos" { "-framework CoreFoundation -liconv -lm" } else { "-lm -ldl -lpthread -lrt -lutil" }

# the C library, static and shared, in release into target/release
c-build:
    cargo build --release -p katui-c

# write bindings/c/include/katui.h from bindings/c/src
c-header:
    cbindgen --quiet --config bindings/c/cbindgen.toml --crate katui-c --output bindings/c/include/katui.h bindings/c

# the C library's gates: a fresh header, the examples, the C tests, the docs and the sync check
c-check: c-build
    #!/usr/bin/env bash
    set -euo pipefail
    cbindgen --quiet --verify --config bindings/c/cbindgen.toml --crate katui-c --output bindings/c/include/katui.h bindings/c
    mkdir -p target/c-tests target/c-shots
    for source in bindings/c/examples/*.c; do
      cc {{c_flags}} "$source" target/release/libkatui.a {{c_libs}} -o "target/c-tests/$(basename "$source" .c)"
    done
    version="-DKATUI_EXPECTED_VERSION=\"$(just version)\""
    for source in bindings/c/tests/test_*.c; do
      test="target/c-tests/$(basename "$source" .c)"
      cc {{c_flags}} "$version" "$source" target/release/libkatui.a {{c_libs}} -o "$test"
      "$test"
    done
    cc {{c_flags}} bindings/c/tests/test_counter.c -Ltarget/release -lkatui -Wl,-rpath,"$PWD/target/release" -o target/c-tests/test_shared
    target/c-tests/test_shared
    python3 bindings/c/tests/docs.py target/release/libkatui.a {{c_libs}}
    python3 bindings/c/tests/sync.py
    clang_tidy=$(command -v clang-tidy || echo "$(brew --prefix llvm)/bin/clang-tidy")
    "$clang_tidy" --quiet bindings/c/examples/*.c bindings/c/tests/test_*.c -- -std=c11 -Ibindings/c/include

# a C example, like `just c-demo todo`
c-demo name: c-build
    mkdir -p target/c-tests
    cc {{c_flags}} bindings/c/examples/{{name}}.c target/release/libkatui.a {{c_libs}} -o target/c-tests/{{name}}
    target/c-tests/{{name}}

# the C archive for a rust target, like aarch64-apple-darwin, into target/dist
c-dist target:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --profile dist -p katui-c --target {{target}}
    target={{target}}
    arch=${target%%-*}
    case $target in
      *-linux-*) os=linux shared=libkatui.so ;;
      *-apple-*) os=macos shared=libkatui.dylib ;;
    esac
    out=target/dist/katui-c
    rm -rf "$out" && mkdir -p "$out/include" "$out/lib"
    cp bindings/c/include/katui.h "$out/include/"
    cp target/{{target}}/dist/libkatui.a "target/{{target}}/dist/$shared" "$out/lib/"
    just strip-bitcode "$out/lib/libkatui.a"
    cp bindings/c/README.md LICENSE-MIT LICENSE-APACHE "$out/"
    tar -czf "target/dist/katui-c-$os-$arch.tar.gz" -C target/dist katui-c
    rm -rf "$out"

# copy the header and this machine's static library into the go module
go-lib: c-build
    #!/usr/bin/env bash
    set -euo pipefail
    lib="bindings/go/lib/$(go env GOOS)_$(go env GOARCH)"
    mkdir -p bindings/go/include "$lib"
    cp bindings/c/include/katui.h bindings/go/include/
    cp target/release/libkatui.a "$lib/"
    just strip-bitcode "$lib/libkatui.a"

# drop the LLVM bitcode that rust's standard library carries from a static
# library, since newer go refuses to link an archive with bitcode in it
strip-bitcode archive:
    #!/usr/bin/env bash
    set -euo pipefail
    objcopy="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin/llvm-objcopy"
    "$objcopy" --remove-section=.llvmbc --remove-section=.llvmcmd \
        --remove-section=__LLVM,__bitcode --remove-section=__LLVM,__cmdline "{{archive}}"

# the go module's gates: vet, gofmt, and the tests, which run the examples, the docs and the sync check
[working-directory('bindings/go')]
go-check: go-fmt-check go-lib
    go vet ./...
    go run github.com/golangci/golangci-lint/v2/cmd/golangci-lint@v2.15.0 run ./...
    go test ./...

# gofmt's check on the go module, without a build
[working-directory('bindings/go')]
go-fmt-check:
    test -z "$(gofmt -l .)" || { gofmt -l .; exit 1; }

# a go example, like `just go-demo todo`
[working-directory('bindings/go')]
go-demo name: go-lib
    go run ./examples/{{name}}

# commit the static libraries of the C archives in `dist` into the go module, on a commit tagged bindings/go/v<version>
go-release dist:
    #!/usr/bin/env bash
    set -euo pipefail
    for archive in {{quote(dist)}}/katui-c-*.tar.gz; do
      platform=$(basename "$archive" .tar.gz)
      platform=${platform#katui-c-}
      os=${platform%-*} arch=${platform#*-}
      [ "$os" = macos ] && os=darwin
      case $arch in x86_64) arch=amd64 ;; aarch64) arch=arm64 ;; esac
      mkdir -p "bindings/go/lib/${os}_$arch"
      tar -xzf "$archive" -C "bindings/go/lib/${os}_$arch" --strip-components=2 katui-c/lib/libkatui.a
    done
    mkdir -p bindings/go/include
    cp bindings/c/include/katui.h bindings/go/include/
    git add --force bindings/go/include bindings/go/lib
    git commit --quiet -m "chore: ship the go module libraries for v$(just version)"
    git tag "bindings/go/v$(just version)"

# push the tag `go-release` made, which publishes the go module
go-publish:
    git push origin "bindings/go/v$(just version)"

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

# render every demo headless into the screenshots folder beside it (needs ffmpeg and cwebp)
screenshots:
    sh scripts/screenshots-rust.sh
    sh scripts/screenshots-bindings.sh

# a markdown reader for a folder of docs, katui's own by default
demo-md path=".":
    cargo run --release -p md-viewer -- {{path}}

# an irc client
demo-irc:
    cargo run --release --manifest-path demos/obby-chat/Cargo.toml
