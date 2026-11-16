# C

The katui C library runs the same engine as the Rust crates: apps with
widgets, HTML and Markdown documents, and games, behind one header,
`katui.h`. This guide builds a small app and a small game, and shows how to
test both without a terminal.

Each release has an archive per platform with the header and the static
and shared libraries; the [C README](../bindings/c/README.md) shows how to
download and link it. In a clone, `just c-build` builds the library into
`target/release` and `just c-demo counter` runs an example.

## An app

An app is a `KatuiApp`: function pointers over your state, which katui
hands back as `user`. `view` returns the elements to show, and `update`
takes the messages they send. Here is a shopping list you type into:

```c
#include <stdio.h>
#include <string.h>

#include "katui.h"

enum { TOGGLE = 100 };

typedef struct {
  char items[16][64];
  bool bought[16];
  size_t count;
  KatuiTextField *draft;
} Shopping;

static KatuiElement *view(void *user) {
  Shopping *list = user;
  KatuiElement *rows[16] = {0};
  for (size_t i = 0; i < list->count; i++) {
    rows[i] = katui_checkbox(list->items[i], list->bought[i], TOGGLE + i);
  }
  KatuiElement *items = katui_column(rows, list->count);
  return katui_panel(KATUI_LIST(katui_title("Shopping"),
                                katui_text_field(list->draft, "Add an item", true), items));
}

static KatuiControl update(void *user, const KatuiMessage *message) {
  Shopping *list = user;
  if (message->key >= TOGGLE && message->key < TOGGLE + list->count) {
    list->bought[message->key - TOGGLE] = !list->bought[message->key - TOGGLE];
  }
  return KATUI_CONTROL_REDRAW;
}

static KatuiControl input(void *user, const KatuiEvent *event) {
  Shopping *list = user;
  if (katui_text_field_handle(list->draft, event) == KATUI_EDIT_SUBMITTED && list->count < 16) {
    char *text = katui_text_field_take(list->draft);
    snprintf(list->items[list->count++], 64, "%s", text);
    katui_string_free(text);
  }
  return KATUI_CONTROL_REDRAW;
}

int main(void) {
  Shopping list = {.draft = katui_text_field_new(NULL)};
  KatuiApp app = {.user = &list, .view = view, .update = update, .input = input};
  KatuiStatus status = katui_run(&app);
  katui_text_field_free(list.draft);
  if (status != KATUI_STATUS_OK) {
    fprintf(stderr, "katui: %s\n", katui_last_error());
  }
  return status;
}
```

Run it, type an item and press <kbd>Enter</kbd>. Click an item to tick it.
<kbd>Ctrl</kbd>+<kbd>C</kbd> quits.

katui calls `view` again after every change and sends the terminal only the
parts that changed. `KATUI_LIST(a, b, c)` expands to the array and count a
container takes, and NULL children are left out.

## Ownership

The header states the rules at its top. In short:

- A container takes over its children: once an element is passed to
  `katui_panel` or `katui_row`, you neither use nor free it. `view` hands
  its element to katui the same way.
- Every other argument is borrowed for the call. katui copies the strings
  it keeps.
- What a function returns is yours: free it with the matching
  `katui_*_free`, and a returned `char *` with `katui_string_free`.
- Every function accepts NULL. A failure returns a `KatuiStatus` or NULL,
  and `katui_last_error()` gives its message.

## Messages

A clickable element takes a `uint64_t` key you choose, and katui hands it
to `update` in a `KatuiMessage`. Widgets that report a value add a payload:
`payload` names the field that holds it.

| Payload | Field | Sent by |
|---|---|---|
| `KATUI_PAYLOAD_NONE` | | buttons, checkboxes, switches, `on_click` |
| `KATUI_PAYLOAD_INDEX` | `index` | tabs, radio, select, menu and table rows |
| `KATUI_PAYLOAD_ID` | `id` | tree nodes |
| `KATUI_PAYLOAD_FLOAT` | `value` | sliders and split dividers |
| `KATUI_PAYLOAD_POINT` | `x`, `y` | `on_press` and `on_grip_press` |
| `KATUI_PAYLOAD_SCROLL` | `scroll` | scroll views and `on_scroll` |
| `KATUI_PAYLOAD_LINK` | `link` | links in documents |

`update` returns what to do next:

| `KatuiControl` | What happens |
|---|---|
| `KATUI_CONTROL_REDRAW` | katui calls `view` once and draws the changes |
| `KATUI_CONTROL_WAIT` | nothing is drawn until the next event |
| `KATUI_CONTROL_ANIMATE` | katui keeps drawing at 30 frames a second |
| `KATUI_CONTROL_QUIT` | the app ends and the terminal is restored |

To send messages from another thread, keep the `KatuiWaker` that `start`
gets. `katui_waker_send(waker, key)` delivers one, and
`katui_waker_every(waker, seconds, key)` delivers one on a steady beat.

## Keys and state

Keys and pastes go to `input`. `katui_event_key` names the key: one
character, or `"enter"`, `"tab"`, `"escape"`, `"up"`, `"page_down"`,
`"f1"` and so on. Keypad, media and lone modifier keys have names like
`"kp_5"`, `"media_play"` and `"left_shift"`. Besides
`katui_event_ctrl`, `katui_event_alt` and `katui_event_shift`, an event
reports `katui_event_super`, `katui_event_hyper` and `katui_event_meta`,
which only terminals with the kitty keyboard protocol send. The widgets
that keep state have a state type you keep
in your app and pass to the widget, and each one takes events with its
`handle` function: `KatuiTextField`, `KatuiTextArea`, `KatuiListState`,
`KatuiScrollState`, `KatuiTreeState` and `KatuiFocus`.

## Themes

The `theme` callback returns the theme for each frame, which katui frees,
or NULL for the dark one. Inside `view`, `katui_theme_current()` gives a
copy of it for your own elements, and `katui_theme_columns(theme, n)` the
width of `n` terminal cells in pixels.

## Documents

`katui_html(source, &document)` and `katui_markdown(source, &document)`
return an element. A click on a link sends `document.link_key` with the
href. `document.image` gives the image for each `src`, and a zeroed
`KatuiDocument`, or NULL, takes every default.

To follow a link to `#install`, call `katui_element_reveal(element,
katui_anchor_id("#install"))` on the scrolling element around the document.
Its `on_scroll` key receives the new position; store it and stop calling
`katui_element_reveal`.

## Games

A game is a `KatuiGame` with `step` and `render`. `katui_play` runs it at
60 steps a second:

```c
#include "katui.h"

static float x = 40;

static void step(void *user, const KatuiKeys *keys, float dt) {
  (void)user;
  x += 60 * dt * ((float)katui_keys_is_held(keys, "right") - (float)katui_keys_is_held(keys, "left"));
}

static void render(void *user, KatuiLight *light) {
  (void)user;
  katui_light_dim(light, 0.8f);
  katui_light_glow(light, (KatuiPoint){x, 30}, 6, katui_srgb(255, 140, 60));
}

int main(void) {
  KatuiGame game = {.step = step, .render = render};
  return katui_play(&game);
}
```

The light is a buffer of light, not of colors: shapes add to what is
there, values above one bloom, and `katui_light_dim` fades the last frame
into a trail. `katui_srgb` turns an ordinary color into light. For 3D,
`KatuiV3`, `KatuiCamera`, `KatuiRng` and `KatuiSparks` come with it.

The other callbacks are optional. `mouse` gets a `KatuiEvent` and its
point in the buffer's pixels. Return false from `changed` while nothing
moves, and `katui_play` stops drawing until something does.
`katui_light_text` draws with a 5 by 7 pixel font,
`katui_light_ui_text` with katui's smooth UI font at any size, and
`katui_light_image` draws a `KatuiImage` blended by its alpha.

## Tests

`katui_screen_new` runs an app without a terminal.
`katui_screen_render` returns a `KatuiImage`, and `katui_screen_click`,
`katui_screen_wheel`, `katui_screen_hover`, `katui_screen_send` and
`katui_screen_type_text` deliver input the way the terminal would.
`katui_frame` steps a game with some keys held and returns its picture.
`katui_image_save` writes an image as a PNG.

## Differences from Rust

The C names follow the Rust API as `katui_<type>_<method>`, with a few
changes:

- Messages are a `uint64_t` key plus a payload.
- Keys are named by strings, like `"left"` and `"enter"`.
- Lengths are `katui_px(n)`, `katui_fill()` and `katui_content()`.
- A color with alpha 0 means the theme's color where a function takes an
  optional one.
- Table columns take a width of 0 to fill the rest.

The [examples](../bindings/c/examples) include a counter, a to-do list and
a comet game.
