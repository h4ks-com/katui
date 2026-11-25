/* Every widget draws, links and themes reach the app, NULL is safe
   everywhere, and failures come back as statuses with a message. */

#include <string.h>

#include "check.h"

enum { NOTHING, TAB, SLIDE, SCROLL, SPLIT, PICK, TOGGLE_NODE, SELECT_NODE, LINK, DRAG };

typedef struct {
  KatuiTextField *field;
  KatuiTextArea *area;
  KatuiListState *list;
  KatuiScrollState *scroll;
  KatuiTreeState *tree;
  KatuiTreeNode *root;
  KatuiToasts *toasts;
  KatuiImage *picture;
  bool light;
  uint64_t last_key;
  char last_link[64];
} Tour;

static KatuiElement *tour_row(void *user, size_t index, bool selected) {
  (void)user;
  char text[32];
  snprintf(text, sizeof text, "%s row %zu", selected ? ">" : " ", index);
  return katui_label(text);
}

static const KatuiImage *tour_image(void *user, const char *src) {
  const Tour *tour = user;
  return strcmp(src, "dot.png") == 0 ? tour->picture : NULL;
}

static KatuiElement *tour_controls(Tour *tour) {
  static const char *const tabs[] = {"One", "Two"};
  static const char *const options[] = {"Red", "Blue"};
  static const KatuiKeyHint hints[] = {{"q", "quit"}, {"tab", "move"}};
  KatuiSpan spans[] = {{.text = "rich ", .bold = true},
                       {.text = "text", .color = {255, 120, 0, 255}, .underline = true}};
  return katui_column(KATUI_LIST(
      katui_title("Title"), katui_heading("Heading"), katui_hint("Hint"), katui_rich(spans, 2),
      katui_row(KATUI_LIST(katui_button("Button", NOTHING), katui_primary_button("Primary", NOTHING),
                                     katui_focusable_button("Focused", NOTHING, true))),
      katui_text_field(tour->field, "field", true), katui_text_area(tour->area, "area", false),
      katui_checkbox("Check", true, NOTHING), katui_switch("Switch", false, NOTHING), katui_progress(0.5f),
      katui_tabs(tabs, 2, 1, TAB), katui_radio(options, 2, 0, PICK), katui_slider(0.25f, SLIDE),
      katui_spinner(0.6), katui_key_hints(hints, 2),
      katui_select(options, 2, 0, false, NOTHING, PICK),
      katui_tooltip(katui_label("Anchor"), "tip", true, NOTHING, NOTHING), katui_image(tour->picture)));
}

static KatuiElement *tour_lists(Tour *tour) {
  static const KatuiColumn columns[] = {{"Name", 8}, {"Size", 0}};
  static const char *const cells[] = {"a.txt", "1 kB", "b.txt", "2 kB"};
  const KatuiTreeNode *roots[] = {tour->root};
  KatuiDocument document = {.link_key = LINK, .image = tour_image, .user = tour};
  KatuiElement *scrolled =
      katui_scroll_view(tour->scroll, KATUI_LIST(katui_label("scrolled"), katui_label("content")), SCROLL);
  KatuiElement *docs = katui_column(KATUI_LIST(
      katui_markdown("# Doc\n\n[a link](next)", &document),
      katui_html("<p>html <b>bold</b></p><img src=\"dot.png\">", &document)));
  KatuiElement *lists = katui_column(KATUI_LIST(
      katui_list(tour->list, tour_row, tour), katui_table(columns, 2, cells, 2, tour->list, PICK),
      katui_tree(roots, 1, tour->tree, TOGGLE_NODE, SELECT_NODE), katui_toasts(tour->toasts),
      katui_menu("Menu", (const char *const[]){"Open", "Close"}, 2, true, NOTHING, PICK)));
  return katui_split(lists, katui_column(KATUI_LIST(scrolled, docs)), 0.5f, KATUI_ORIENTATION_HORIZONTAL,
                     SPLIT);
}

static KatuiElement *tour_view(void *user) {
  Tour *tour = user;
  KatuiElement *body = katui_row(KATUI_LIST(tour_controls(tour), tour_lists(tour)));
  body = katui_element_gap(katui_element_padding_xy(body, 8, 4), 12);
  body = katui_element_size(body, katui_fill(), katui_fill());
  KatuiElement *badge = katui_element_on_press(
      katui_element_radius(katui_element_background(katui_text("badge"), (KatuiColor){40, 80, 160, 255}), 4),
      DRAG);
  badge = katui_element_border(katui_element_color(badge, (KatuiColor){255, 255, 255, 255}), 1,
                               (KatuiColor){200, 200, 200, 255});
  badge = katui_element_weight(katui_element_font_size(badge, 18), 700);
  return katui_page(KATUI_LIST(katui_stack(KATUI_LIST(body, katui_element_popup(badge, 700, 8))),
                               katui_spacer()));
}

static KatuiControl tour_update(void *user, const KatuiMessage *message) {
  Tour *tour = user;
  tour->last_key = message->key;
  if (message->payload == KATUI_PAYLOAD_LINK) {
    snprintf(tour->last_link, sizeof tour->last_link, "%s", message->link);
  }
  if (message->payload == KATUI_PAYLOAD_SCROLL) {
    katui_scroll_state_apply(tour->scroll, message->scroll);
  }
  return KATUI_CONTROL_REDRAW;
}

static KatuiTheme *tour_theme(void *user, uint16_t cell_width, uint16_t cell_height) {
  const Tour *tour = user;
  if (!tour->light) {
    return NULL;
  }
  KatuiTheme *theme = katui_theme_light(cell_width, cell_height);
  katui_theme_set_accent(theme, (KatuiColor){224, 85, 155, 255});
  return theme;
}

static void tour_open(Tour *tour) {
  uint8_t pixels[4 * 4 * 4];
  memset(pixels, 200, sizeof pixels);
  KatuiTreeNode *children[] = {katui_tree_node_leaf(2, "leaf"), NULL};
  *tour = (Tour){
      .field = katui_text_field_new("typed"),
      .area = katui_text_area_new(3),
      .list = katui_list_state_new(5, 3),
      .scroll = katui_scroll_state_new(),
      .tree = katui_tree_state_new(),
      .root = katui_tree_node_branch(1, "root", children, 2),
      .toasts = katui_toasts_new(),
      .picture = katui_image_from_rgba(4, 4, pixels, sizeof pixels),
  };
  katui_tree_state_toggle(tour->tree, 1);
  katui_toasts_push(tour->toasts, "saved");
}

static void tour_close(Tour *tour) {
  katui_text_field_free(tour->field);
  katui_text_area_free(tour->area);
  katui_list_state_free(tour->list);
  katui_scroll_state_free(tour->scroll);
  katui_tree_state_free(tour->tree);
  katui_tree_node_free(tour->root);
  katui_toasts_free(tour->toasts);
  katui_image_free(tour->picture);
}

static void every_widget_draws_and_the_theme_switches(void) {
  Tour tour;
  tour_open(&tour);
  KatuiApp app = {.user = &tour, .view = tour_view, .update = tour_update, .theme = tour_theme};
  KatuiScreen *screen = katui_screen_new(&app, 1200, 700, 10, 20);
  CHECK(screen != NULL);
  KatuiImage *dark = katui_screen_render(screen);
  CHECK(dark != NULL);
  CHECK(lit(dark) > 1000);
  tour.light = true;
  KatuiImage *light = katui_screen_render(screen);
  KatuiColor before;
  KatuiColor after;
  CHECK(katui_image_pixel(dark, 1199, 699, &before));
  CHECK(katui_image_pixel(light, 1199, 699, &after));
  CHECK(before.r != after.r);
  CHECK(!katui_image_pixel(dark, 1200, 0, &before));
  shot(dark, "tour");
  shot(light, "tour-light");
  CHECK(katui_screen_wheel(screen, 900, 100, true) == KATUI_STATUS_OK);
  CHECK(katui_screen_hover(screen, 300, 300) == KATUI_STATUS_OK);
  katui_screen_free(screen);
  tour_close(&tour);
}

typedef struct {
  char link[64];
  int clicks;
} Reader;

static KatuiElement *reader_view(void *user) {
  (void)user;
  KatuiDocument document = {.link_key = LINK, .font_size = 20};
  return katui_markdown("[open the keys page](keys)", &document);
}

static KatuiControl reader_update(void *user, const KatuiMessage *message) {
  Reader *reader = user;
  if (message->key == LINK && message->payload == KATUI_PAYLOAD_LINK) {
    snprintf(reader->link, sizeof reader->link, "%s", message->link);
    reader->clicks++;
  }
  return KATUI_CONTROL_REDRAW;
}

static void a_link_click_sends_its_href(void) {
  Reader reader = {0};
  KatuiApp app = {.user = &reader, .view = reader_view, .update = reader_update};
  KatuiScreen *screen = katui_screen_new(&app, 400, 100, 10, 20);
  CHECK(screen != NULL);
  for (int y = 2; y < 60 && reader.clicks == 0; y += 4) {
    for (int x = 4; x < 200 && reader.clicks == 0; x += 8) {
      CHECK(katui_screen_click(screen, (float)x, (float)y) == KATUI_STATUS_OK);
    }
  }
  CHECK(reader.clicks == 1);
  CHECK(strcmp(reader.link, "keys") == 0);
  katui_screen_free(screen);
}

static KatuiElement *revealed_view(void *user) {
  (void)user;
#define FILLER "filler\n\nfiller\n\nfiller\n\nfiller\n\n"
  static const char source[] = FILLER FILLER FILLER FILLER FILLER "## The target\n\nend";
#undef FILLER
  KatuiElement *doc = katui_column(KATUI_LIST(katui_markdown(source, NULL)));
  doc = katui_element_reveal(katui_element_scroll(doc, 0), katui_anchor_id("#the-target"));
  return katui_element_size(katui_element_on_scroll(doc, SCROLL), katui_fill(), katui_fill());
}

static KatuiControl revealed_update(void *user, const KatuiMessage *message) {
  float *offset = user;
  if (message->key == SCROLL && message->payload == KATUI_PAYLOAD_SCROLL) {
    *offset = message->scroll.offset;
  }
  return KATUI_CONTROL_REDRAW;
}

static void reveal_scrolls_to_an_anchor(void) {
  float offset = 0;
  KatuiApp app = {.user = &offset, .view = revealed_view, .update = revealed_update};
  KatuiScreen *screen = katui_screen_new(&app, 200, 100, 10, 20);
  katui_image_free(katui_screen_render(screen));
  CHECK(offset > 0);
  CHECK(katui_anchor_id("#the-target") == katui_anchor_id("the-target"));
  katui_screen_free(screen);
}

static KatuiElement *broken_view(void *user) {
  (void)user;
  return NULL;
}

static void a_broken_callback_fails_with_a_message(void) {
  Reader reader = {0};
  KatuiApp missing = {.user = &reader, .view = broken_view};
  CHECK(katui_screen_new(&missing, 100, 100, 10, 20) == NULL);
  CHECK(strstr(katui_last_error(), "update") != NULL);
  KatuiApp broken = {.user = &reader, .view = broken_view, .update = reader_update};
  KatuiScreen *screen = katui_screen_new(&broken, 100, 100, 10, 20);
  CHECK(katui_screen_render(screen) == NULL);
  CHECK(strstr(katui_last_error(), "view") != NULL);
  katui_screen_free(screen);
}

static void null_is_safe_everywhere(void) {
  CHECK(katui_run(NULL) == KATUI_STATUS_NULL_ARGUMENT);
  CHECK(katui_play(NULL) == KATUI_STATUS_NULL_ARGUMENT);
  CHECK(katui_screen_new(NULL, 10, 10, 10, 20) == NULL);
  CHECK(katui_screen_render(NULL) == NULL);
  CHECK(katui_screen_click(NULL, 0, 0) == KATUI_STATUS_NULL_ARGUMENT);
  CHECK(katui_screen_send(NULL, NULL) == KATUI_STATUS_NULL_ARGUMENT);
  CHECK(!katui_screen_quit(NULL));
  CHECK(katui_element_padding(NULL, 4) == NULL);
  CHECK(katui_frame(NULL, 10, 10, 1, NULL, 0, 0.1f) == NULL);
  CHECK(katui_image_save(NULL, "x.png") == KATUI_STATUS_NULL_ARGUMENT);
  CHECK(katui_image_width(NULL) == 0);
  CHECK(katui_text_field_handle(NULL, NULL) == KATUI_EDIT_IGNORED);
  CHECK(!katui_list_state_handle(NULL, NULL));
  CHECK(katui_event_kind(NULL) == KATUI_EVENT_OTHER);
  CHECK(katui_event_key(NULL) == NULL);
  CHECK(katui_theme_body(NULL) == 0);
  CHECK(!katui_waker_send(NULL, 1));
  CHECK(!katui_keys_is_held(NULL, "left"));
  CHECK(katui_sparks_is_empty(NULL));
  katui_element_free(NULL);
  katui_image_free(NULL);
  katui_string_free(NULL);
  katui_light_dim(NULL, 0.5f);
  katui_element_free(katui_panel(NULL, 3));
  katui_element_free(katui_label(NULL));
  katui_element_free(katui_html(NULL, NULL));
}

static void bad_values_fail_with_a_message(void) {
  uint8_t pixels[3] = {0};
  CHECK(katui_image_from_rgba(2, 2, pixels, 3) == NULL);
  CHECK(strstr(katui_last_error(), "RGBA") != NULL);
  CHECK(katui_image_decode(pixels, 3) == NULL);
  CHECK(katui_image_open("no/such/file.png") == NULL);
  CHECK(katui_event_press("nonsense", false, false, false, false, false, false) == NULL);
  CHECK(katui_focus_new(0) == NULL);
}

static void events_and_state_answer(void) {
  KatuiEvent *event = katui_event_press("down", true, false, false, false, false, false);
  CHECK(katui_event_kind(event) == KATUI_EVENT_KEY);
  char *key = katui_event_key(event);
  CHECK(strcmp(key, "down") == 0);
  katui_string_free(key);
  CHECK(katui_event_ctrl(event) && !katui_event_shift(event));
  KatuiEvent *chord = katui_event_press("left_super", false, false, false, true, true, true);
  CHECK(katui_event_super(chord) && katui_event_hyper(chord) && katui_event_meta(chord));
  CHECK(!katui_event_ctrl(chord));
  katui_event_free(chord);
  KatuiListState *list = katui_list_state_new(5, 2);
  KatuiEvent *plain = katui_event_press("down", false, false, false, false, false, false);
  CHECK(katui_list_state_handle(list, plain));
  CHECK(katui_list_state_selected(list) == 1);
  float moved = 0;
  KatuiEvent *right = katui_event_press("right", false, false, false, false, false, false);
  CHECK(katui_nudge(0.5f, right, 0.1f, &moved) && moved > 0.5f);
  KatuiFocus *focus = katui_focus_new(3);
  KatuiEvent *tab = katui_event_press("tab", false, false, false, false, false, false);
  CHECK(katui_focus_handle(focus, tab) && katui_focus_is(focus, 1));
  KatuiEvent *paste = katui_event_paste("pasted");
  char *text = katui_event_text(paste);
  CHECK(strcmp(text, "pasted") == 0);
  katui_string_free(text);
  KatuiToasts *toasts = katui_toasts_new();
  katui_toasts_push_for(toasts, "hello", 5);
  char *toast = katui_toasts_text(toasts, 0);
  CHECK(strcmp(toast, "hello") == 0);
  katui_string_free(toast);
  CHECK(katui_toasts_text(toasts, 1) == NULL);
  KatuiCamera camera = {.center = {50, 50}, .focal = 100, .distance = 5, .pixel_aspect = 1};
  KatuiPoint at;
  float scale = 0;
  CHECK(katui_camera_project(&camera, katui_v3(1, 0, 0), &at, &scale) && at.x > 50 && scale > 0);
  CHECK(strlen(katui_version()) > 0);
#ifdef KATUI_EXPECTED_VERSION
  CHECK(strcmp(katui_version(), KATUI_EXPECTED_VERSION) == 0);
#endif
  katui_toasts_free(toasts);
  katui_focus_free(focus);
  katui_list_state_free(list);
  katui_event_free(event);
  katui_event_free(plain);
  katui_event_free(right);
  katui_event_free(tab);
  katui_event_free(paste);
}

static void a_light_takes_pixels_images_and_ui_text(void) {
  KatuiLight *light = katui_light_new(60, 30, 1);
  katui_light_set_pixel(light, 1, 2, (KatuiRgb){0.5f, 0.25f, 1});
  KatuiRgb color = {0};
  CHECK(katui_light_pixel(light, 1, 2, &color) && color.r == 0.5f && color.b == 1);
  CHECK(!katui_light_pixel(light, 60, 0, &color));
  uint8_t red[16];
  for (int at = 0; at < 16; at += 4) {
    memcpy(red + at, (uint8_t[]){255, 0, 0, 255}, 4);
  }
  KatuiImage *sprite = katui_image_from_rgba(2, 2, red, sizeof red);
  katui_light_image(light, (KatuiPoint){10, 10}, sprite);
  float width = katui_light_ui_text_width(light, "Hi", 16);
  CHECK(width > 0 && width < 40);
  katui_light_ui_text(light, (KatuiPoint){30, 4}, "Hi", 16, (KatuiRgb){1, 1, 1});
  KatuiImage *drawn = katui_light_to_image(light);
  KatuiColor pixel = {0};
  CHECK(katui_image_pixel(drawn, 11, 11, &pixel) && pixel.r == 255 && pixel.g == 0);
  katui_image_free(drawn);
  katui_image_free(sprite);
  katui_light_free(light);
}

int main(void) {
  a_light_takes_pixels_images_and_ui_text();
  every_widget_draws_and_the_theme_switches();
  a_link_click_sends_its_href();
  reveal_scrolls_to_an_anchor();
  a_broken_callback_fails_with_a_message();
  null_is_safe_everywhere();
  bad_values_fail_with_a_message();
  events_and_state_answer();
  return 0;
}
