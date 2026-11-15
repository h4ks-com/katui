default:
    @just --list

# clippy with ci's denials, after every change
check: clippy

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# apply clippy's and rustfmt's fixes
fix:
    cargo clippy --workspace --all-targets --all-features --fix --allow-dirty --allow-staged
    cargo fmt --all

test:
    cargo test --workspace --all-features --all-targets
    cargo test --workspace --all-features --doc

# licenses, advisories and sources, from deny.toml
deny:
    cargo deny check

typos:
    if command -v typos >/dev/null; then typos; else echo "typos not installed, skipping"; fi

# complexity per function, into target/metrics (cargo install rust-code-analysis-cli)
metrics:
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
    cargo doc --workspace --no-deps --all-features
    echo '<meta http-equiv="refresh" content="0; url=katui/index.html">' > target/doc/index.html

# every gate, before every commit
ci: fmt-check clippy test docs typos

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
