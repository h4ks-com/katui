<h1 align="center">katui for C</h1>

<p align="center">
  katui is a C library for apps, games and documents drawn in pixels inside the terminal.
</p>

<p align="center">
  <a href="#install">Install</a> ·
  <a href="https://github.com/h4ks-com/katui/blob/main/docs/c.md">Guide</a> ·
  <a href="https://github.com/h4ks-com/katui/tree/main/bindings/c/examples">Examples</a> ·
  <a href="https://github.com/h4ks-com/katui/blob/main/bindings/c/include/katui.h">Header</a>
</p>

<p align="center"><img src="https://raw.githubusercontent.com/h4ks-com/katui/main/assets/demos/widgets.webp" alt="The katui widget tour: a file tree, a theme select, a switch with a spinner, a slider and a table" width="800"></p>

It sends pictures to the terminal through the kitty graphics protocol, and
through sixel or blocks of colored cells where that is missing. Only what
changed between frames is sent. The engine is the
[katui](https://github.com/h4ks-com/katui) Rust library, built into
one static or shared library with a single header, `katui.h`.

```c
#include <stdio.h>

#include "katui.h"

static int count;

static KatuiElement *view(void *user) {
  (void)user;
  char text[32];
  snprintf(text, sizeof text, "Clicked %d times", count);
  return katui_panel(KATUI_LIST(katui_label(text), katui_primary_button("Add one", 1)));
}

static KatuiControl update(void *user, const KatuiMessage *message) {
  (void)user;
  count += message->key == 1;
  return KATUI_CONTROL_REDRAW;
}

int main(void) {
  KatuiApp app = {.view = view, .update = update};
  return katui_run(&app) == KATUI_STATUS_OK ? 0 : 1;
}
```

## Install

katui runs on Linux and macOS, on x86_64 and arm64. Each release has an
archive per platform with `include/katui.h` and `lib/` holding
`libkatui.a` and the shared library. On Linux x86_64:

```sh
curl -L https://github.com/h4ks-com/katui/releases/latest/download/katui-c-linux-x86_64.tar.gz | tar xz
```

<details>
<summary>Linux arm64, macOS</summary>

Replace the archive name with `katui-c-linux-aarch64.tar.gz`,
`katui-c-macos-aarch64.tar.gz` or `katui-c-macos-x86_64.tar.gz`.

</details>

Save the example above as `counter.c` and link it against the static
library:

```sh
cc counter.c -Ikatui-c/include katui-c/lib/libkatui.a -lm -ldl -lpthread -o counter   # Linux
cc counter.c -Ikatui-c/include katui-c/lib/libkatui.a -framework CoreFoundation -liconv -o counter   # macOS
```

Run `./counter`. A panel shows a line of text and a button. Click the
button with the mouse and the count goes up. Press
<kbd>Ctrl</kbd>+<kbd>C</kbd> to quit.

kitty, Ghostty and iTerm2 3.7 show the app at full resolution, and
terminals with sixel graphics, like foot, show it as sixel. Other
terminals show the same app in colored block characters.

## Features

- **Widgets:** labels, buttons, text fields and areas, lists, tables, trees,
  tabs, switches, sliders, selects, menus, tooltips, dialogs, toasts, scroll
  views and split panes, in a dark or light theme or your own.
- **Messages:** every clickable element sends a `uint64_t` key you choose,
  with the index, value or link that was picked.
- **Documents:** `katui_html` and `katui_markdown` turn a page into
  elements, with links that send messages and images you supply.
- **Games:** `katui_play` calls your game 60 times a second to move and
  then draw it, with shapes, glowing light, text and particles.
- **Tests:** `katui_screen_new` draws an app into an image and sends it
  clicks and keys without a terminal, and `katui_frame` does the same for
  games.
- **Safe at the edges:** every function accepts NULL, and failures come
  back as a status with `katui_last_error()` giving the message. The
  header states who frees what.

The [examples](https://github.com/h4ks-com/katui/tree/main/bindings/c/examples)
include a counter, a to-do list and a comet game.

## Status

katui is young, and its API can still change between minor versions. The
C library has the same version as the Rust crates and is released together
with them. Windows is not supported.

## License

katui is licensed under either of the
[Apache License 2.0](https://github.com/h4ks-com/katui/blob/main/LICENSE-APACHE)
or the [MIT license](https://github.com/h4ks-com/katui/blob/main/LICENSE-MIT),
at your option.
