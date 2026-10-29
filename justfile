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
    command -v typos >/dev/null && typos || echo "typos not installed, skipping"

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
    cargo run -p katui-ratatui --example gallery

# the plasma demo: quarter-block animation on any truecolor terminal
demo-plasma:
    cargo run -p katui-ratatui --example plasma

# the snake demo: a self-playing game on a pixel surface
demo-snake:
    cargo run -p katui-ratatui --example snake

# the dvd demo: the immortal bouncing logo, color per bounce, corner-hit flash
demo-dvd:
    cargo run -p katui-ratatui --example dvd

# the gate: must pass before any commit; CI runs the same targets
ci: fmt-check clippy test typos
