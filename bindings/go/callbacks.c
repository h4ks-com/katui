// The C side of the Go callbacks: each katui callback forwards to an
// exported Go function, and `user` carries a cgo.Handle to the Go value.

#include "_cgo_export.h"
#include "callbacks.h"

static KatuiElement *view(void *user) { return katuiGoView((uintptr_t)user); }

static KatuiControl update(void *user, const KatuiMessage *message) {
  return katuiGoUpdate((uintptr_t)user, (KatuiMessage *)message);
}

static KatuiControl input(void *user, const KatuiEvent *event) {
  return katuiGoInput((uintptr_t)user, (KatuiEvent *)event);
}

static KatuiTheme *theme(void *user, uint16_t cell_width, uint16_t cell_height) {
  return katuiGoTheme((uintptr_t)user, cell_width, cell_height);
}

static void start(void *user, KatuiWaker *waker) { katuiGoStart((uintptr_t)user, waker); }

static KatuiApp app(uintptr_t handle) {
  return (KatuiApp){.user = (void *)handle,
                    .start = start,
                    .view = view,
                    .update = update,
                    .input = input,
                    .theme = theme};
}

KatuiStatus katui_go_run(uintptr_t handle) {
  KatuiApp callbacks = app(handle);
  return katui_run(&callbacks);
}

KatuiScreen *katui_go_screen_new(uintptr_t handle, uint32_t width, uint32_t height, uint16_t cell_width,
                                 uint16_t cell_height) {
  KatuiApp callbacks = app(handle);
  return katui_screen_new(&callbacks, width, height, cell_width, cell_height);
}

static void press(void *user, const char *key) { katuiGoPress((uintptr_t)user, (char *)key, false); }

static void repeat(void *user, const char *key) { katuiGoPress((uintptr_t)user, (char *)key, true); }

static void resize(void *user, const KatuiLight *light) {
  katuiGoResize((uintptr_t)user, (KatuiLight *)light);
}

static void step(void *user, const KatuiKeys *keys, float dt) {
  katuiGoStep((uintptr_t)user, (KatuiKeys *)keys, dt);
}

static void render(void *user, KatuiLight *light) { katuiGoRender((uintptr_t)user, light); }

static bool finished(void *user) { return katuiGoFinished((uintptr_t)user); }

static KatuiGame game(uintptr_t handle) {
  return (KatuiGame){.user = (void *)handle,
                     .press = press,
                     .repeat = repeat,
                     .resize = resize,
                     .step = step,
                     .render = render,
                     .finished = finished};
}

KatuiStatus katui_go_play(uintptr_t handle) {
  KatuiGame callbacks = game(handle);
  return katui_play(&callbacks);
}

KatuiImage *katui_go_frame(uintptr_t handle, uint32_t width, uint32_t height, uint32_t steps,
                           const char *const *held, size_t held_count, float dt) {
  KatuiGame callbacks = game(handle);
  return katui_frame(&callbacks, width, height, steps, held, held_count, dt);
}

static KatuiElement *row(void *user, size_t index, bool selected) {
  return katuiGoRow((uintptr_t)user, index, selected);
}

KatuiElement *katui_go_list(const KatuiListState *state, uintptr_t handle) {
  return katui_list(state, row, (void *)handle);
}

static const KatuiImage *image(void *user, const char *src) {
  return katuiGoImage((uintptr_t)user, (char *)src);
}

KatuiElement *katui_go_document(const char *source, bool markdown, KatuiDocument document,
                                uintptr_t handle, bool images) {
  document.image = images ? image : NULL;
  document.user = (void *)handle;
  return markdown ? katui_markdown(source, &document) : katui_html(source, &document);
}

static KatuiRgb shade(void *user, size_t x, size_t y) { return katuiGoShade((uintptr_t)user, x, y); }

void katui_go_light_paint(KatuiLight *light, uintptr_t handle) {
  katui_light_paint(light, shade, (void *)handle);
}

static bool place(void *user, KatuiV3 at, KatuiPoint *on_screen) {
  return katuiGoPlace((uintptr_t)user, at, on_screen);
}

void katui_go_sparks_draw(const KatuiSparks *sparks, KatuiLight *light, uintptr_t handle) {
  katui_sparks_draw(sparks, light, place, (void *)handle);
}
