#!/bin/sh
# Draw every binding example headless and write bindings/<binding>/examples/screenshots/<name>.webp.
set -eu
cd "$(dirname "$0")/.."
rm -f target/*-shots/example-*.png

just py-build
(cd bindings/python && uv run --no-sync pytest -q tests/test_screenshots.py)

just node-build
(cd bindings/node && node --test tests/screenshots.test.ts)

just go-lib
(cd bindings/go && go test -count=1 -run TestScreenshot ./examples/...)

case $(uname) in
  Darwin) libs="-framework CoreFoundation -liconv -lm" ;;
  *) libs="-lm -ldl -lpthread -lrt -lutil" ;;
esac
mkdir -p target/c-tests target/c-shots
# shellcheck disable=SC2086
cc -std=c11 -Wall -Wextra -Werror -pedantic -Ibindings/c/include bindings/c/tests/test_screenshots.c \
  target/release/libkatui.a $libs -o target/c-tests/test_screenshots
target/c-tests/test_screenshots

for binding in python node go c; do
  mkdir -p "bindings/$binding/examples/screenshots"
  for png in "target/$binding-shots"/example-*.png; do
    name=${png##*/example-}
    cwebp -quiet -q 80 "$png" -o "bindings/$binding/examples/screenshots/${name%.png}.webp"
  done
done
