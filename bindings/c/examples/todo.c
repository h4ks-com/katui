/* A to-do list: type a task, press Enter to add it, Tab to tick the selected one. */

#include <stdbool.h>
#include <stdio.h>
#include <string.h>

#include "katui.h"

#define MAX_TASKS 64
#define TEXT_SIZE 128
#define VISIBLE 10

/* a click on the checkbox of the task at index i sends TOGGLE + i */
enum { PICK_FILTER = 1, TOGGLE = 1000 };

static const char *const FILTERS[] = {"All", "Active", "Done"};

typedef struct {
  char text[TEXT_SIZE];
  bool done;
} Task;

typedef struct {
  Task tasks[MAX_TASKS];
  size_t count;
  size_t filter;
  KatuiTextField *draft;
  KatuiListState *selection;
} Todo;

static bool todo_wanted(const Todo *todo, const Task *task) {
  return todo->filter == 0 || task->done == (todo->filter == 2);
}

static size_t todo_shown(const Todo *todo, size_t shown[MAX_TASKS]) {
  size_t count = 0;
  for (size_t index = 0; index < todo->count; index++) {
    if (todo_wanted(todo, &todo->tasks[index])) {
      shown[count++] = index;
    }
  }
  return count;
}

static void todo_add(Todo *todo, const char *text) {
  if (todo->count == MAX_TASKS) {
    return;
  }
  Task *task = &todo->tasks[todo->count++];
  snprintf(task->text, sizeof task->text, "%s", text);
  task->done = false;
}

static void todo_refit(Todo *todo) {
  size_t shown[MAX_TASKS];
  katui_list_state_resize(todo->selection, todo_shown(todo, shown), VISIBLE);
}

static void todo_pick_filter(Todo *todo, size_t filter) {
  todo->filter = filter;
  todo_refit(todo);
}

static KatuiElement *todo_row(void *user, size_t position, bool selected) {
  (void)selected;
  const Todo *todo = user;
  size_t shown[MAX_TASKS];
  if (position >= todo_shown(todo, shown)) {
    return NULL;
  }
  const Task *task = &todo->tasks[shown[position]];
  return katui_checkbox(task->text, task->done, TOGGLE + shown[position]);
}

static KatuiElement *todo_view(void *user) {
  Todo *todo = user;
  size_t shown[MAX_TASKS];
  size_t done = 0;
  for (size_t index = 0; index < todo->count; index++) {
    done += todo->tasks[index].done;
  }
  KatuiElement *body = todo_shown(todo, shown) > 0 ? katui_list(todo->selection, todo_row, todo)
                                                    : katui_hint("Type a task and press Enter");
  KatuiTheme *theme = katui_theme_current();
  KatuiElement *card = katui_panel(KATUI_LIST(
      katui_title("Tasks"), katui_tabs(FILTERS, 3, todo->filter, PICK_FILTER),
      katui_text_field(todo->draft, "Add a task", true), body,
      katui_progress(todo->count > 0 ? (float)done / (float)todo->count : 0.0f),
      katui_hint("Press Up and Down to pick a task, Tab to toggle it")));
  card = katui_element_width(card, katui_px(katui_theme_columns(theme, 60)));
  katui_theme_free(theme);
  return katui_element_align(katui_page(KATUI_LIST(card)), KATUI_ALIGN_CENTER);
}

static KatuiControl todo_update(void *user, const KatuiMessage *message) {
  Todo *todo = user;
  if (message->key == PICK_FILTER) {
    todo_pick_filter(todo, message->index);
  } else if (message->key >= TOGGLE && message->key < TOGGLE + todo->count) {
    Task *task = &todo->tasks[message->key - TOGGLE];
    task->done = !task->done;
  }
  return KATUI_CONTROL_REDRAW;
}

static void todo_submit(Todo *todo) {
  char *text = katui_text_field_take(todo->draft);
  if (text != NULL && strspn(text, " ") < strlen(text)) {
    todo_add(todo, text);
    todo_refit(todo);
  }
  katui_string_free(text);
}

static KatuiControl todo_input(void *user, const KatuiEvent *event) {
  Todo *todo = user;
  if (katui_list_state_handle(todo->selection, event)) {
    return KATUI_CONTROL_REDRAW;
  }
  char *key = katui_event_key(event);
  bool tab = key != NULL && strcmp(key, "tab") == 0;
  katui_string_free(key);
  size_t shown[MAX_TASKS];
  size_t count = todo_shown(todo, shown);
  if (tab && count > 0) {
    size_t selected = katui_list_state_selected(todo->selection);
    Task *task = &todo->tasks[shown[selected < count ? selected : count - 1]];
    task->done = !task->done;
    return KATUI_CONTROL_REDRAW;
  }
  KatuiEdit edit = katui_text_field_handle(todo->draft, event);
  if (edit == KATUI_EDIT_SUBMITTED) {
    todo_submit(todo);
  }
  return edit == KATUI_EDIT_IGNORED ? KATUI_CONTROL_WAIT : KATUI_CONTROL_REDRAW;
}

static void todo_open(Todo *todo) {
  *todo = (Todo){.draft = katui_text_field_new(NULL)};
  todo_add(todo, "water the plants");
  todo_add(todo, "fix the bike");
  todo->tasks[1].done = true;
  todo->selection = katui_list_state_new(todo->count, VISIBLE);
}

static void todo_close(Todo *todo) {
  katui_text_field_free(todo->draft);
  katui_list_state_free(todo->selection);
}

static KatuiApp todo_app(Todo *todo) {
  return (KatuiApp){.user = todo, .view = todo_view, .update = todo_update, .input = todo_input};
}

#ifndef KATUI_NO_MAIN
int main(void) {
  Todo todo;
  todo_open(&todo);
  KatuiApp app = todo_app(&todo);
  KatuiStatus status = katui_run(&app);
  todo_close(&todo);
  if (status != KATUI_STATUS_OK) {
    fprintf(stderr, "katui: %s\n", katui_last_error());
    return 1;
  }
  return 0;
}
#endif
