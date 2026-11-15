#!/bin/sh
# Render every Rust demo headlessly and write its screenshot next to it.
set -eu

cd "$(dirname "$0")/.."
shots=target/screenshots

cargo test -q -p katui --features app,game --example counter --example glow
cargo test -q -p katui-app --example todo --example widgets --example browse
cargo test -q -p katui-game --examples -- --include-ignored
cargo test -q -p katui-ratatui --features run --example viewer
cargo test -q --manifest-path demos/obby-chat/Cargo.toml conversation
cargo test -q -p md-viewer the_view

# shot <folder> <png name> [webp name]
shot() {
    mkdir -p "$1/screenshots"
    cwebp -quiet -q 80 "$shots/$2.png" -o "$1/screenshots/${3:-$2}.webp"
}

for name in counter glow; do shot crates/katui/examples "$name"; done
shot crates/katui-app/examples todo
shot crates/katui-app/examples widgets-dusk widgets
shot crates/katui-app/examples browse
shot crates/katui-app/examples showcase
for name in cube dvd life lumenshaft plasma shardrift snake video; do
    shot crates/katui-game/examples "$name"
done
shot crates/katui-ratatui/examples viewer
shot demos/obby-chat conversation
shot demos/md-viewer md-viewer
