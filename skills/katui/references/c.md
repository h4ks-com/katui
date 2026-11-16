# katui in C

One header, `katui.h`, over a static or shared library. Each GitHub release
has an archive per platform with the header and both libraries; the C
README shows how to link it:
https://github.com/h4ks-com/katui/blob/main/bindings/c/README.md.
Full guide: https://github.com/h4ks-com/katui/blob/main/docs/c.md

## Ownership

- A container takes over its children: after passing an element to
  `katui_panel`, `katui_row` or `katui_column`, neither use nor free it.
  `view` hands its element to katui the same way.
- Every other argument is borrowed for the call; katui copies strings it
  keeps.
- What a function returns is yours: free it with the matching
  `katui_*_free`, and a returned `char *` with `katui_string_free`.
- Every function accepts NULL. A failure returns a `KatuiStatus` or NULL,
  and `katui_last_error()` gives the message.

## An app

`KatuiApp` holds `user` plus function pointers: `view` and `update` are
required; `start`, `input` and `theme` are optional.

```c
#include <stdio.h>

#include "katui.h"

enum { ADD = 1 };

typedef struct {
  int count;
} Counter;

static KatuiElement *counter_view(void *user) {
  const Counter *counter = user;
  char text[64];
  snprintf(text, sizeof text, "Clicked %d times", counter->count);
  KatuiElement *panel = katui_panel(KATUI_LIST(katui_label(text), katui_primary_button("Add one", ADD)));
  return katui_element_align(panel, KATUI_ALIGN_CENTER);
}

static KatuiControl counter_update(void *user, const KatuiMessage *message) {
  Counter *counter = user;
  if (message->key == ADD) {
    counter->count++;
  }
  return KATUI_CONTROL_REDRAW;
}

int main(void) {
  Counter counter = {0};
  KatuiApp app = {.user = &counter, .view = counter_view, .update = counter_update};
  if (katui_run(&app) != KATUI_STATUS_OK) {
    fprintf(stderr, "katui: %s\n", katui_last_error());
    return 1;
  }
  return 0;
}
```

`KATUI_LIST(a, b, c)` expands to the array and count a container takes;
NULL children are left out. `katui_column(rows, count)` takes an array.

## Messages

A clickable element takes a `uint64_t` key you choose. `update` gets a
`KatuiMessage` with `key` and a `payload` naming the field that holds a
value: `KATUI_PAYLOAD_INDEX` (`index`: tabs, radio, select, menu, table),
`KATUI_PAYLOAD_ID` (`id`: tree nodes), `KATUI_PAYLOAD_FLOAT` (`value`:
sliders, split dividers), `KATUI_PAYLOAD_POINT` (`x`, `y`: `on_press`),
`KATUI_PAYLOAD_SCROLL` (`scroll`), `KATUI_PAYLOAD_LINK` (`link`: document
links), or `KATUI_PAYLOAD_NONE`. Return `KATUI_CONTROL_REDRAW`, `_WAIT`,
`_ANIMATE` or `_QUIT`. From other threads use the `KatuiWaker` that
`start` gets: `katui_waker_send(waker, key)` and
`katui_waker_every(waker, seconds, key)`.

## Keys, state, layout, themes, documents

- `input` gets a `KatuiEvent`. `katui_event_key(event)` returns the key name
  (one character, or `"enter"`, `"tab"`, `"up"`, ...) as a `char *` to free
  with `katui_string_free`. `katui_event_press(name, ctrl, alt, shift, super, hyper, meta)`
  builds one for tests.
- State types with a `handle` function: `KatuiTextField`
  (`katui_text_field_handle` returns `KATUI_EDIT_SUBMITTED` on Enter),
  `KatuiTextArea`, `KatuiListState`, `KatuiScrollState`, `KatuiTreeState`,
  `KatuiFocus`.
- Layout: `katui_element_gap`, `katui_element_padding`,
  `katui_element_size(element, width, height)` with `katui_px(n)`,
  `katui_fill()` and `katui_content()`, `katui_element_align`.
- Themes: the `theme` callback returns a theme katui frees, or NULL for
  dark. Build it with `katui_theme_dark(cw, ch)` or `katui_theme_light`,
  adjust with `katui_theme_set_accent` and `katui_theme_scaled`.
  `katui_theme_current()` gives a copy inside `view`, and
  `katui_theme_columns(theme, n)` the width of `n` cells. A color with alpha
  0 means the theme's color where a function takes an optional one.
- Documents: `katui_html(source, &document)` and
  `katui_markdown(source, &document)`. A link click sends
  `document.link_key` with the href; a zeroed `KatuiDocument` or NULL takes
  every default.

## Games

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

`KatuiGame` also has `press`, `repeat`, `resize` and `finished`.

## Tests

```c
KatuiScreen *screen = katui_screen_new(&app, 480, 240, 10, 20);
katui_screen_type_text(screen, "milk");
katui_screen_click(screen, 240, 72);
KatuiImage *image = katui_screen_render(screen);
katui_image_save(image, "counter.png");
katui_image_free(image);
katui_screen_free(screen);

const char *held[] = {"right"};
KatuiImage *picture = katui_frame(&game, 320, 180, 10, held, 1, 1.0f / 60);
```

The screen also has `katui_screen_wheel`, `_hover`, `_press`, `_drag`,
`_release`, `_send` and `katui_screen_quit`. `katui_screen_render` returns
NULL when a callback failed. Examples:
https://github.com/h4ks-com/katui/tree/main/bindings/c/examples
