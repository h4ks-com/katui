default:
    @just --list

# fast inner-loop verification; agents run this after every change.
# the same denials as ci, so a warning fails here first
check: clippy

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# auto-fix what is mechanical
fix:
    cargo clippy --workspace --all-targets --all-features --fix --allow-dirty --allow-staged
    cargo fmt --all

test:
    cargo test --workspace --all-features --all-targets

# license, advisory and source bans; config in deny.toml
deny:
    cargo deny check

# spelling gate, matches CI's typos job
typos:
    if command -v typos >/dev/null; then typos; else echo "typos not installed, skipping"; fi

# complexity and maintainability report, per function (cargo install rust-code-analysis-cli)
metrics:
    rust-code-analysis-cli -m -p crates -l rust -O json -o target/metrics -j 8

# copy-paste duplication gate, matches CI's dupes job; config in .jscpd.json
dupes:
    npx --yes jscpd@4 crates/

# unused-dependency gate, matches CI's machete job
machete:
    cargo machete

# install the prek hooks, once per clone
hooks:
    prek install

# print the terminal's capability card (needs a terminal)
info:
    cargo run -p katui-rt --bin katui-info

# the gallery demo: styled cells plus a kitty-protocol image (needs a terminal)
demo-gallery:
    cargo run --release -p katui-ratatui --example gallery

# the plasma demo: an interference plasma in pixels or quarter blocks
demo-plasma:
    cargo run --release -p katui-game --example plasma

# the snake demo: a self-playing game on a pixel surface
demo-snake:
    cargo run --release -p katui-game --example snake

# the to-do demo: a task list on the widgets crate through katui-app
demo-todo:
    cargo run --release -p katui-app --example todo

# the cube demo: a lit cube turning in software 3D, arrow keys steer its spin
demo-cube:
    cargo run --release -p katui-game --example cube

# the dvd demo: the immortal bouncing logo, color per bounce, corner-hit flash
demo-dvd:
    cargo run --release -p katui-game --example dvd

# the life demo: conway's automaton in 256 colors, age-hot cells, ash trails
demo-life:
    cargo run --release -p katui-game --example life

# the lumenshaft demo: stack glowing blocks down a 3d shaft
demo-lumenshaft:
    cargo run --release -p katui-game --example lumenshaft

# the shardrift demo: drift through a nebula and split crystal rocks
demo-shardrift:
    cargo run --release -p katui-game --example shardrift

# the pacing instrument: fps, latency and bytes per frame for each pixel strategy
pace:
    cargo run --release -p katui-rt --example pace

# the irc demo: a client drawn in real pixels (needs a graphics terminal)
demo-irc:
    cargo run --release --manifest-path demos/obby-chat/Cargo.toml

# docs gate: broken intra-doc links and rustdoc warnings fail like code
docs:
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items

# the gate: must pass before any commit; CI runs the same targets
ci: fmt-check clippy test docs typos
