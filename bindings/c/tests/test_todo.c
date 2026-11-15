/* The to-do list adds typed tasks, ticks them with Tab and filters them. */

#define KATUI_NO_MAIN
#include "../examples/todo.c"
#include "check.h"

static void press(KatuiScreen *screen, const char *name) {
  KatuiEvent *event = katui_event_press(name, false, false, false);
  CHECK(event != NULL);
  CHECK(katui_screen_send(screen, event) == KATUI_STATUS_OK);
  katui_event_free(event);
}

int main(void) {
  Todo todo;
  todo_open(&todo);
  KatuiApp app = todo_app(&todo);
  KatuiScreen *screen = katui_screen_new(&app, 900, 480, 10, 20);
  CHECK(screen != NULL);
  katui_image_free(katui_screen_render(screen));

  CHECK(katui_screen_type_text(screen, "buy bread") == KATUI_STATUS_OK);
  press(screen, "enter");
  CHECK(todo.count == 3);
  CHECK(strcmp(todo.tasks[2].text, "buy bread") == 0);
  CHECK(!todo.tasks[2].done);

  press(screen, "tab");
  CHECK(todo.tasks[0].done);

  todo_pick_filter(&todo, 2);
  size_t shown[MAX_TASKS];
  CHECK(todo_shown(&todo, shown) == 2);
  CHECK(shown[0] == 0 && shown[1] == 1);
  shot(katui_screen_render(screen), "todo");

  katui_screen_free(screen);
  todo_close(&todo);
  return 0;
}
