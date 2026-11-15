/* Every example drawn after a little use, for the screenshots in examples/README.md. */

#define KATUI_NO_MAIN
#include "../examples/comet.c"
#include "../examples/counter.c"
#include "../examples/todo.c"
#include "check.h"

#define WIDTH 900
#define HEIGHT 560

static void counter_screenshot(void) {
  Counter counter = {.count = 3};
  KatuiApp app = counter_app(&counter);
  KatuiScreen *screen = katui_screen_new(&app, WIDTH, HEIGHT, 10, 20);
  CHECK(screen != NULL);
  shot(katui_screen_render(screen), "example-counter");
  katui_screen_free(screen);
}

static void todo_screenshot(void) {
  Todo todo;
  todo_open(&todo);
  KatuiApp app = todo_app(&todo);
  KatuiScreen *screen = katui_screen_new(&app, WIDTH, HEIGHT, 10, 20);
  CHECK(screen != NULL);
  KatuiEvent *enter = katui_event_press("enter", false, false, false);
  CHECK(enter != NULL);
  const char *tasks[] = {"buy bread", "call the bank"};
  for (size_t index = 0; index < 2; index++) {
    CHECK(katui_screen_type_text(screen, tasks[index]) == KATUI_STATUS_OK);
    CHECK(katui_screen_send(screen, enter) == KATUI_STATUS_OK);
  }
  katui_event_free(enter);
  CHECK(katui_screen_type_text(screen, "walk the dog") == KATUI_STATUS_OK);
  shot(katui_screen_render(screen), "example-todo");
  katui_screen_free(screen);
  todo_close(&todo);
}

static void comet_screenshot(void) {
  Comet comet;
  comet_open(&comet);
  KatuiGame game = comet_game(&comet);
  const char *steer[] = {"right", "down"};
  katui_image_free(katui_frame(&game, 360, 224, 40, steer, 2, 1.0f / 60));
  comet_press(&comet, " ");
  shot(katui_frame(&game, 360, 224, 20, steer, 1, 1.0f / 60), "example-comet");
  comet_close(&comet);
}

int main(void) {
  counter_screenshot();
  todo_screenshot();
  comet_screenshot();
  return 0;
}
