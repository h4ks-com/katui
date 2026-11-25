/*
 * katui: real pixels in the terminal, from C.
 *
 * Ownership
 *
 * - A function that returns a pointer to a katui type hands it to the
 *   caller, who frees it with the matching katui_*_free function.
 * - A function that takes elements (children, a dialog's body, a split's
 *   panes) takes them over: the caller does not use or free them again.
 *   NULL entries in a list of children are left out.
 * - A view callback hands its element to katui, and a theme callback its
 *   theme.
 * - Every other pointer argument is borrowed for the length of the call.
 *   Strings are UTF-8 and NUL-terminated; katui copies what it keeps.
 * - A string katui returns as char * belongs to the caller, who frees it
 *   with katui_string_free. A const char * stays katui's.
 * - Images are shared: an element that shows an image keeps its own
 *   reference, so the caller can free the image at once.
 * - Pointers handed to callbacks (messages, events, keys, lights) live
 *   during the call only.
 *
 * Safety
 *
 * Every function accepts NULL for any pointer: getters return zero, false
 * or NULL, builders return NULL or an empty element, and functions that
 * return a KatuiStatus return KATUI_STATUS_NULL_ARGUMENT. Any other
 * pointer has to point at a live value of its type. katui never unwinds
 * into C: a failure returns a status or NULL, and katui_last_error() gives
 * its message.
 *
 * Threads
 *
 * katui calls every callback on the thread that called katui_run,
 * katui_play or a katui_screen function. katui_waker_send and
 * katui_waker_every work from any thread.
 */


#ifndef KATUI_H
#define KATUI_H

/* Generated from bindings/c/src by cbindgen. Run `just c-header` after a change there. */

#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
/* The children of an element as the pointer and count katui takes:
 * katui_panel(KATUI_LIST(katui_title("Tasks"), katui_label("None yet"))). */
#define KATUI_LIST(...) \
  (KatuiElement *[]){__VA_ARGS__}, \
  (sizeof((KatuiElement *[]){__VA_ARGS__}) / sizeof(KatuiElement *))


/**
 * The first kitty image id games use.
 */
#define KATUI_BASE_IMAGE 900

/**
 * How much smaller games draw under a terminal multiplexer.
 */
#define KATUI_MULTIPLEXED_SHRINK 2

/**
 * One node of a view. Its methods change it and return it, so calls nest.
 */
typedef struct KatuiElement KatuiElement;

/**
 * A key press, a release, a mouse event, a paste or a focus change.
 */
typedef struct KatuiEvent KatuiEvent;

/**
 * Which of `count` controls, numbered from 0, has the keyboard; Tab and
 * Shift+Tab move it.
 */
typedef struct KatuiFocus KatuiFocus;

/**
 * Pixels to show, decoded once and shared, so an unchanged image costs
 * nothing to draw again.
 */
typedef struct KatuiImage KatuiImage;

/**
 * The keys held down right now.
 */
typedef struct KatuiKeys KatuiKeys;

/**
 * A floating-point framebuffer games add light to.
 */
typedef struct KatuiLight KatuiLight;

/**
 * The selected row of a list and the rows in view.
 */
typedef struct KatuiListState KatuiListState;

/**
 * A small deterministic generator, so games replay the same in tests.
 */
typedef struct KatuiRng KatuiRng;

/**
 * An app drawn into pixels, with clicks and keys sent by hand.
 */
typedef struct KatuiScreen KatuiScreen;

/**
 * How far a scroll view is scrolled.
 */
typedef struct KatuiScrollState KatuiScrollState;

/**
 * Every live spark, aged, moved and drawn together.
 */
typedef struct KatuiSparks KatuiSparks;

/**
 * A multi-line text input that scrolls to keep its cursor in view.
 */
typedef struct KatuiTextArea KatuiTextArea;

/**
 * A one-line text input's text and cursor.
 */
typedef struct KatuiTextField KatuiTextField;

/**
 * The colors and sizes every widget draws with. A gradient color reads as
 * transparent.
 */
typedef struct KatuiTheme KatuiTheme;

/**
 * Short messages that go away on their own.
 */
typedef struct KatuiToasts KatuiToasts;

/**
 * One node of a tree.
 */
typedef struct KatuiTreeNode KatuiTreeNode;

/**
 * Which tree nodes are open and which one is selected.
 */
typedef struct KatuiTreeState KatuiTreeState;

/**
 * Sends messages to a running app from any thread.
 */
typedef struct KatuiWaker KatuiWaker;

/**
 * What a function that does something returns: `KATUI_STATUS_OK`, or the
 * kind of failure, with `katui_last_error()` giving the message.
 */
typedef int32_t KatuiStatus;

/**
 * What to do after a message or an event.
 */
typedef uint32_t KatuiControl;

/**
 * Which field of a `KatuiMessage` holds the payload.
 */
typedef uint32_t KatuiPayload;

/**
 * Where a scrolling element stands, in pixels.
 */
typedef struct KatuiScroll {
  /**
   * How far the content is scrolled.
   */
  float offset;
  /**
   * The furthest the content can scroll.
   */
  float max;
} KatuiScroll;

/**
 * A message as `update` receives it. Only the field `payload` names is
 * set; the others are zero. katui owns `link`, valid during the call.
 */
typedef struct KatuiMessage {
  /**
   * The key the element was built with.
   */
  uint64_t key;
  KatuiPayload payload;
  size_t index;
  uint64_t id;
  float value;
  float x;
  float y;
  struct KatuiScroll scroll;
  const char *link;
} KatuiMessage;

/**
 * A pixel UI program as function pointers over your state at `user`.
 * `view` and `update` are required; leave the others NULL for their
 * defaults. katui calls them all on the thread that runs the app.
 */
typedef struct KatuiApp {
  /**
   * Handed to every callback as is.
   */
  void *user;
  /**
   * Keep `waker` to send messages from other threads. The app owns it
   * and frees it with `katui_waker_free`.
   */
  void (*start)(void *user, struct KatuiWaker *waker);
  /**
   * The view of the current state. katui takes over the element; NULL
   * ends the app with `KATUI_STATUS_CALLBACK`.
   */
  struct KatuiElement *(*view)(void *user);
  /**
   * Apply `message`, which katui owns, and say what to do next.
   */
  KatuiControl (*update)(void *user, const struct KatuiMessage *message);
  /**
   * React to a key or a paste; `event` lives during the call. NULL
   * waits for the next event.
   */
  KatuiControl (*input)(void *user, const struct KatuiEvent *event);
  /**
   * The theme for cells of `cell_width` by `cell_height` pixels, asked
   * before every frame. katui takes over the theme; NULL, or a NULL
   * callback, means the dark theme.
   */
  struct KatuiTheme *(*theme)(void *user, uint16_t cell_width, uint16_t cell_height);
} KatuiApp;

/**
 * How a length is measured.
 */
typedef uint32_t KatuiLengthKind;

/**
 * A width or a height.
 */
typedef struct KatuiLength {
  KatuiLengthKind kind;
  float pixels;
} KatuiLength;

/**
 * A straight-alpha color.
 */
typedef struct KatuiColor {
  uint8_t r;
  uint8_t g;
  uint8_t b;
  uint8_t a;
} KatuiColor;

/**
 * A run of text with its own look, for `katui_rich`. A color with alpha 0
 * leaves the text's color or background as it is.
 */
typedef struct KatuiSpan {
  const char *text;
  struct KatuiColor color;
  struct KatuiColor background;
  bool bold;
  bool italic;
  bool underline;
  bool strike;
} KatuiSpan;

/**
 * Where children sit along an axis.
 */
typedef uint32_t KatuiAlign;

/**
 * Light, in linear channels; values above 1 bloom.
 */
typedef struct KatuiRgb {
  float r;
  float g;
  float b;
} KatuiRgb;

/**
 * The light of the pixel at `x`, `y`.
 */
typedef struct KatuiRgb (*KatuiShade)(void *user, size_t x, size_t y);

/**
 * A point in the light buffer's pixels.
 */
typedef struct KatuiPoint {
  float x;
  float y;
} KatuiPoint;

/**
 * How a shape meets the light under it.
 */
typedef uint32_t KatuiBlend;

/**
 * A point or direction in world space.
 */
typedef struct KatuiV3 {
  float x;
  float y;
  float z;
} KatuiV3;

/**
 * A pinhole camera looking down +z, with y up.
 */
typedef struct KatuiCamera {
  /**
   * Where the optical axis meets the screen.
   */
  struct KatuiPoint center;
  /**
   * Pixels of screen height per unit of world height at unit depth.
   */
  float focal;
  /**
   * How far the eye sits behind z = 0.
   */
  float distance;
  /**
   * One pixel's width over its height.
   */
  float pixel_aspect;
} KatuiCamera;

/**
 * One glowing particle.
 */
typedef struct KatuiSpark {
  struct KatuiV3 at;
  /**
   * In world units per second.
   */
  struct KatuiV3 velocity;
  struct KatuiRgb color;
  /**
   * Seconds lived.
   */
  float age;
  /**
   * Seconds lived when it dies.
   */
  float life;
  /**
   * Line width in pixels.
   */
  float width;
} KatuiSpark;

/**
 * Write where the spark at `at` shows on screen into `on_screen`; false
 * skips it.
 */
typedef bool (*KatuiPlace)(void *user, struct KatuiV3 at, struct KatuiPoint *on_screen);

/**
 * A game as function pointers over your state at `user`. `step` and
 * `render` are required; leave the others NULL. Key names are strings
 * like `"a"`, `" "`, `"enter"` and `"left"`, owned by katui and valid
 * during the call; so are `keys` and `light`.
 */
typedef struct KatuiGame {
  /**
   * Handed to every callback as is.
   */
  void *user;
  /**
   * `key` went down.
   */
  void (*press)(void *user, const char *key);
  /**
   * `key` auto-repeated while held.
   */
  void (*repeat)(void *user, const char *key);
  /**
   * The screen became the size of `light`, so baked art needs redoing.
   */
  void (*resize)(void *user, const struct KatuiLight *light);
  /**
   * Advance the world by `dt` seconds, a fixed step.
   */
  void (*step)(void *user, const struct KatuiKeys *keys, float dt);
  /**
   * Draw the world into `light`.
   */
  void (*render)(void *user, struct KatuiLight *light);
  /**
   * Whether the game is over, so `katui_play` ends after this frame.
   */
  bool (*finished)(void *user);
  /**
   * The mouse did `event` at `at`, in the light buffer's pixels: a
   * press, release, drag, move or wheel turn.
   */
  void (*mouse)(void *user, const struct KatuiEvent *event, struct KatuiPoint at);
  /**
   * Whether the next frame differs from the last one drawn; NULL means
   * always. While it returns false, `katui_play` draws nothing and steps
   * ten times a second.
   */
  bool (*changed)(void *user);
} KatuiGame;

/**
 * What links and images in a document do, and how its text looks. A zeroed
 * struct, or NULL, takes every default.
 */
typedef struct KatuiDocument {
  /**
   * A click on a link sends this key with a link payload.
   */
  uint64_t link_key;
  /**
   * The image for an `src`, which stays the caller's, or NULL to leave
   * it out. katui calls it during `katui_html` or `katui_markdown`.
   */
  const struct KatuiImage *(*image)(void *user, const char *src);
  /**
   * Handed to `image` as is.
   */
  void *user;
  /**
   * The body text size in pixels, or 0 for the theme's.
   */
  float font_size;
  /**
   * The text color, or alpha 0 for the theme's.
   */
  struct KatuiColor color;
  /**
   * The link color, or alpha 0 for the theme's accent.
   */
  struct KatuiColor link;
  /**
   * The color of quotes and rules, or alpha 0 for the theme's muted one.
   */
  struct KatuiColor tint;
} KatuiDocument;

/**
 * What kind of event a `KatuiEvent` is.
 */
typedef uint32_t KatuiEventKind;

/**
 * What the mouse did.
 */
typedef uint32_t KatuiMouseAction;

/**
 * Which mouse button.
 */
typedef uint32_t KatuiButton;

/**
 * What an event did to a text field.
 */
typedef uint32_t KatuiEdit;

/**
 * Builds the row at `index` of a list, returning an element katui takes
 * over, or NULL for an empty row.
 */
typedef struct KatuiElement *(*KatuiRow)(void *user, size_t index, bool selected);

/**
 * One key and what it does, for `katui_key_hints`.
 */
typedef struct KatuiKeyHint {
  const char *key;
  const char *action;
} KatuiKeyHint;

/**
 * How `katui_split` lays out its panes.
 */
typedef uint32_t KatuiOrientation;

/**
 * A table column: its title and its width in cells, or 0 to fill the rest.
 */
typedef struct KatuiColumn {
  const char *title;
  float cells;
} KatuiColumn;

/**
 * Sleep until something happens.
 */
#define KATUI_CONTROL_WAIT 0

/**
 * Draw once.
 */
#define KATUI_CONTROL_REDRAW 1

/**
 * Keep drawing at the frame rate.
 */
#define KATUI_CONTROL_ANIMATE 2

/**
 * End the app.
 */
#define KATUI_CONTROL_QUIT 3

/**
 * `pixels` pixels.
 */
#define KATUI_LENGTH_PX 0

/**
 * All the space the parent leaves.
 */
#define KATUI_LENGTH_FILL 1

/**
 * As much as the content needs.
 */
#define KATUI_LENGTH_CONTENT 2

#define KATUI_ALIGN_START 0

#define KATUI_ALIGN_CENTER 1

#define KATUI_ALIGN_END 2

#define KATUI_STATUS_OK 0

/**
 * A required pointer was NULL.
 */
#define KATUI_STATUS_NULL_ARGUMENT 1

/**
 * A value was out of range or could not be parsed.
 */
#define KATUI_STATUS_INVALID_ARGUMENT 2

/**
 * Reading, writing or the terminal failed.
 */
#define KATUI_STATUS_IO 3

/**
 * A callback broke its contract, like a view that returned NULL.
 */
#define KATUI_STATUS_CALLBACK 4

/**
 * katui hit a bug and stopped it at the boundary.
 */
#define KATUI_STATUS_PANIC 5

/**
 * Cover what is there.
 */
#define KATUI_BLEND_OVER 0

/**
 * Add to what is there.
 */
#define KATUI_BLEND_ADD 1

/**
 * Only the key: buttons, checkboxes, `katui_waker_send`.
 */
#define KATUI_PAYLOAD_NONE 0

/**
 * `index`: tabs, radio, select, menu and table rows.
 */
#define KATUI_PAYLOAD_INDEX 1

/**
 * `id`: tree nodes.
 */
#define KATUI_PAYLOAD_ID 2

/**
 * `value`: sliders and split dividers.
 */
#define KATUI_PAYLOAD_FLOAT 3

/**
 * `x` and `y`, as fractions of the element's box: presses and drags.
 */
#define KATUI_PAYLOAD_POINT 4

/**
 * `scroll`: scroll views and elements with `on_scroll`.
 */
#define KATUI_PAYLOAD_SCROLL 5

/**
 * `link`: a link clicked in a document.
 */
#define KATUI_PAYLOAD_LINK 6

#define KATUI_EVENT_OTHER 0

#define KATUI_EVENT_KEY 1

#define KATUI_EVENT_KEY_UP 2

#define KATUI_EVENT_MOUSE 3

#define KATUI_EVENT_PASTE 4

#define KATUI_EVENT_FOCUS 5

#define KATUI_EVENT_KEY_REPEAT 6

#define KATUI_MOUSE_NONE 0

#define KATUI_MOUSE_PRESS 1

#define KATUI_MOUSE_RELEASE 2

#define KATUI_MOUSE_DRAG 3

#define KATUI_MOUSE_MOVE 4

#define KATUI_MOUSE_WHEEL_UP 5

#define KATUI_MOUSE_WHEEL_DOWN 6

#define KATUI_MOUSE_WHEEL_LEFT 7

#define KATUI_MOUSE_WHEEL_RIGHT 8

#define KATUI_BUTTON_NONE 0

#define KATUI_BUTTON_LEFT 1

#define KATUI_BUTTON_MIDDLE 2

#define KATUI_BUTTON_RIGHT 3

#define KATUI_BUTTON_BACK 4

#define KATUI_BUTTON_FORWARD 5

#define KATUI_EDIT_CHANGED 0

#define KATUI_EDIT_MOVED 1

#define KATUI_EDIT_SUBMITTED 2

#define KATUI_EDIT_IGNORED 3

/**
 * Side by side.
 */
#define KATUI_ORIENTATION_HORIZONTAL 0

/**
 * One above the other.
 */
#define KATUI_ORIENTATION_VERTICAL 1

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/**
 * Hand `key` to the app's `update`, from any thread. False once the app
 * has quit.
 */
bool katui_waker_send(const struct KatuiWaker *waker, uint64_t key);

/**
 * Hand `key` to `update` every `seconds` until the app quits.
 */
void katui_waker_every(const struct KatuiWaker *waker, double seconds, uint64_t key);

/**
 * Free a waker. NULL does nothing.
 */
void katui_waker_free(struct KatuiWaker *waker);

/**
 * Run `app` on the terminal until it quits. `app` is copied; `user` has
 * to live until this returns.
 */
KatuiStatus katui_run(const struct KatuiApp *app);

/**
 * A length of `pixels` pixels.
 */
struct KatuiLength katui_px(float pixels);

/**
 * A length that fills the space the parent leaves.
 */
struct KatuiLength katui_fill(void);

/**
 * A length that fits the content.
 */
struct KatuiLength katui_content(void);

/**
 * Free an element katui did not take over. NULL does nothing.
 */
void katui_element_free(struct KatuiElement *element);

/**
 * Children side by side. Takes over every child; NULL ones are left out.
 */
struct KatuiElement *katui_row(struct KatuiElement *const *children, size_t count);

/**
 * Children top to bottom. Takes over every child; NULL ones are left out.
 */
struct KatuiElement *katui_column(struct KatuiElement *const *children, size_t count);

/**
 * Children on top of each other. Takes over every child; NULL ones are
 * left out.
 */
struct KatuiElement *katui_stack(struct KatuiElement *const *children, size_t count);

/**
 * A line of text in the current theme's look.
 */
struct KatuiElement *katui_text(const char *text);

/**
 * Text made of `count` spans, each with its own look.
 */
struct KatuiElement *katui_rich(const struct KatuiSpan *spans, size_t count);

/**
 * An element that shows `image`. The image stays the caller's; the
 * element keeps its own reference to the pixels.
 */
struct KatuiElement *katui_image(const struct KatuiImage *image);

/**
 * Empty space that grows to fill what the others leave.
 */
struct KatuiElement *katui_spacer(void);

/**
 * Draw this subtree into its own layer, kept under `key` between frames.
 */
struct KatuiElement *katui_element_layer(struct KatuiElement *element, uint64_t key);

/**
 * Scroll the children by `offset` pixels.
 */
struct KatuiElement *katui_element_scroll(struct KatuiElement *element, float offset);

/**
 * A click sends `key`.
 */
struct KatuiElement *katui_element_on_click(struct KatuiElement *element, uint64_t key);

/**
 * A click on the span at `span` of a rich text sends `key`.
 */
struct KatuiElement *katui_element_on_span_click(struct KatuiElement *element,
                                                 size_t span,
                                                 uint64_t key);

/**
 * The pointer entering sends `key`.
 */
struct KatuiElement *katui_element_on_hover(struct KatuiElement *element, uint64_t key);

/**
 * The pointer leaving sends `key`.
 */
struct KatuiElement *katui_element_on_leave(struct KatuiElement *element, uint64_t key);

/**
 * Each new scroll position sends `key` with a scroll payload.
 */
struct KatuiElement *katui_element_on_scroll(struct KatuiElement *element, uint64_t key);

/**
 * The press and each move until the release send `key` with a point
 * payload, as fractions of the element's box.
 */
struct KatuiElement *katui_element_on_press(struct KatuiElement *element, uint64_t key);

/**
 * Like `on_press`, for presses on a child marked with `grip`.
 */
struct KatuiElement *katui_element_on_grip_press(struct KatuiElement *element, uint64_t key);

/**
 * Mark this element as the handle of its parent's `on_grip_press`.
 */
struct KatuiElement *katui_element_grip(struct KatuiElement *element);

/**
 * Draw above everything, at `x`, `y` pixels from its parent's corner.
 */
struct KatuiElement *katui_element_popup(struct KatuiElement *element, float x, float y);

/**
 * Catch clicks over its whole box.
 */
struct KatuiElement *katui_element_opaque(struct KatuiElement *element);

/**
 * Show a scrollbar in `color`.
 */
struct KatuiElement *katui_element_scrollbar(struct KatuiElement *element, struct KatuiColor color);

/**
 * Name the element, so it keeps its state when the tree changes around it.
 */
struct KatuiElement *katui_element_id(struct KatuiElement *element, uint64_t id);

/**
 * Scroll the scrolling element so its descendant with `id` starts at the
 * top; its `on_scroll` key receives the new position.
 */
struct KatuiElement *katui_element_reveal(struct KatuiElement *element, uint64_t id);

struct KatuiElement *katui_element_padding(struct KatuiElement *element, float all);

struct KatuiElement *katui_element_padding_xy(struct KatuiElement *element, float x, float y);

/**
 * Space between children, in pixels.
 */
struct KatuiElement *katui_element_gap(struct KatuiElement *element, float gap);

struct KatuiElement *katui_element_width(struct KatuiElement *element, struct KatuiLength width);

struct KatuiElement *katui_element_height(struct KatuiElement *element, struct KatuiLength height);

struct KatuiElement *katui_element_size(struct KatuiElement *element,
                                        struct KatuiLength width,
                                        struct KatuiLength height);

/**
 * How much of the free space this element takes, against its siblings.
 */
struct KatuiElement *katui_element_grow(struct KatuiElement *element, float grow);

/**
 * Where the children sit across the main axis.
 */
struct KatuiElement *katui_element_align(struct KatuiElement *element, KatuiAlign to);

/**
 * Where the children sit along the main axis.
 */
struct KatuiElement *katui_element_justify(struct KatuiElement *element, KatuiAlign to);

struct KatuiElement *katui_element_background(struct KatuiElement *element,
                                              struct KatuiColor color);

/**
 * Round the corners by `radius` pixels.
 */
struct KatuiElement *katui_element_radius(struct KatuiElement *element, float radius);

struct KatuiElement *katui_element_border(struct KatuiElement *element,
                                          float width,
                                          struct KatuiColor color);

/**
 * The text color.
 */
struct KatuiElement *katui_element_color(struct KatuiElement *element, struct KatuiColor color);

struct KatuiElement *katui_element_font_size(struct KatuiElement *element, float size);

/**
 * The font weight, from 100 to 900; 400 is regular and 700 bold.
 */
struct KatuiElement *katui_element_weight(struct KatuiElement *element, uint16_t weight);

/**
 * An image from `length` bytes of straight-alpha RGBA, four per pixel,
 * copied. NULL when the length does not match.
 */
struct KatuiImage *katui_image_from_rgba(uint32_t width,
                                         uint32_t height,
                                         const uint8_t *pixels,
                                         size_t length);

/**
 * An image decoded from `length` bytes of PNG, JPEG, GIF or WebP. NULL
 * when they do not decode.
 */
struct KatuiImage *katui_image_decode(const uint8_t *data, size_t length);

/**
 * An image read from a PNG, JPEG, GIF or WebP file. NULL when it cannot
 * be read or decoded.
 */
struct KatuiImage *katui_image_open(const char *path);

/**
 * Free an image. Elements that show it keep their own reference. NULL
 * does nothing.
 */
void katui_image_free(struct KatuiImage *image);

uint32_t katui_image_width(const struct KatuiImage *image);

uint32_t katui_image_height(const struct KatuiImage *image);

/**
 * The RGBA bytes, four per pixel, row by row, with their count in
 * `length`. The image owns them; they live as long as it does.
 */
const uint8_t *katui_image_rgba(const struct KatuiImage *image, size_t *length);

/**
 * Write the color at `x`, `y` into `color`. False outside the image.
 */
bool katui_image_pixel(const struct KatuiImage *image,
                       uint32_t x,
                       uint32_t y,
                       struct KatuiColor *color);

/**
 * Write the image to `path` as a PNG.
 */
KatuiStatus katui_image_save(const struct KatuiImage *image, const char *path);

/**
 * The message of this thread's last error, or NULL before the first one.
 * katui owns the string; it stays valid until the next error on this
 * thread.
 */
const char *katui_last_error(void);

/**
 * Free a string katui returned. NULL does nothing.
 */
void katui_string_free(char *text);

/**
 * The version of katui, like "0.1.0". katui owns the string.
 */
const char *katui_version(void);

/**
 * The light that shows as the sRGB color `red`, `green`, `blue`.
 */
struct KatuiRgb katui_srgb(uint8_t red, uint8_t green, uint8_t blue);

/**
 * `color` with every channel times `factor`.
 */
struct KatuiRgb katui_scaled(struct KatuiRgb color, float factor);

/**
 * The color `amount` of the way from `start` to `end`.
 */
struct KatuiRgb katui_lerp3(struct KatuiRgb start, struct KatuiRgb end, float amount);

struct KatuiLight *katui_light_new(uint32_t width, uint32_t height, float pixel_aspect);

/**
 * Free a light the caller made. NULL does nothing.
 */
void katui_light_free(struct KatuiLight *light);

uint32_t katui_light_width(const struct KatuiLight *light);

uint32_t katui_light_height(const struct KatuiLight *light);

/**
 * One pixel's width over its height.
 */
float katui_light_pixel_aspect(const struct KatuiLight *light);

/**
 * Copy the pixels of `other`, a light of the same size, into `light`.
 */
void katui_light_copy_from(struct KatuiLight *light, const struct KatuiLight *other);

/**
 * Fill every pixel with `shade(user, x, y)`, for backdrops baked once.
 */
void katui_light_paint(struct KatuiLight *light, KatuiShade shade, void *user);

/**
 * Fade the last frame by `factor`, 0 for black and 1 to keep it.
 */
void katui_light_dim(struct KatuiLight *light, float factor);

/**
 * Write the light at `x`, `y` into `color`. False outside the buffer.
 */
bool katui_light_pixel(const struct KatuiLight *light,
                       uint32_t x,
                       uint32_t y,
                       struct KatuiRgb *color);

/**
 * Set the light at `x`, `y`; a spot outside the buffer is ignored.
 */
void katui_light_set_pixel(struct KatuiLight *light, uint32_t x, uint32_t y, struct KatuiRgb color);

/**
 * Draw `image` with its top left at `at`, blended by its alpha.
 */
void katui_light_image(struct KatuiLight *light,
                       struct KatuiPoint at,
                       const struct KatuiImage *image);

/**
 * Text in the UI font, `size` pixels tall, with its top left at `at`.
 */
void katui_light_ui_text(struct KatuiLight *light,
                         struct KatuiPoint at,
                         const char *text,
                         float size,
                         struct KatuiRgb color);

float katui_light_ui_text_width(const struct KatuiLight *light, const char *text, float size);

void katui_light_rect(struct KatuiLight *light,
                      struct KatuiPoint at,
                      float width,
                      float height,
                      struct KatuiRgb color,
                      KatuiBlend mode);

/**
 * A filled polygon through `count` points.
 */
void katui_light_polygon(struct KatuiLight *light,
                         const struct KatuiPoint *points,
                         size_t count,
                         struct KatuiRgb color,
                         KatuiBlend mode);

void katui_light_line(struct KatuiLight *light,
                      struct KatuiPoint from,
                      struct KatuiPoint to,
                      float width,
                      struct KatuiRgb color);

/**
 * A soft disc of light that fades out at `radius`.
 */
void katui_light_glow(struct KatuiLight *light,
                      struct KatuiPoint center,
                      float radius,
                      struct KatuiRgb color);

void katui_light_ring(struct KatuiLight *light,
                      struct KatuiPoint center,
                      float radius,
                      float width,
                      struct KatuiRgb color);

/**
 * Pixel-font text with its top left at `at`, each font pixel `scale`
 * pixels wide.
 */
void katui_light_text(struct KatuiLight *light,
                      struct KatuiPoint at,
                      const char *text,
                      uint32_t scale,
                      struct KatuiRgb color);

void katui_light_blended_text(struct KatuiLight *light,
                              struct KatuiPoint at,
                              const char *text,
                              uint32_t scale,
                              struct KatuiRgb color,
                              KatuiBlend mode);

/**
 * Pixel-font text centered on `center_x`, its top at `y`.
 */
void katui_light_centered_text(struct KatuiLight *light,
                               float center_x,
                               float y,
                               const char *text,
                               uint32_t scale,
                               struct KatuiRgb color);

float katui_light_text_width(const struct KatuiLight *light, const char *text, uint32_t scale);

float katui_light_text_height(uint32_t scale);

/**
 * Spread light above `threshold` into a glow of `strength`.
 */
void katui_light_bloom(struct KatuiLight *light, float threshold, float strength);

/**
 * The buffer tone-mapped into an image the caller owns.
 */
struct KatuiImage *katui_light_to_image(const struct KatuiLight *light);

/**
 * Whether the key named `name`, like `"left"` or `"a"`, is held.
 */
bool katui_keys_is_held(const struct KatuiKeys *keys, const char *name);

/**
 * Whether any of the `count` keys at `names` is held.
 */
bool katui_keys_any_held(const struct KatuiKeys *keys, const char *const *names, size_t count);

struct KatuiV3 katui_v3(float x, float y, float z);

struct KatuiV3 katui_v3_add(struct KatuiV3 a, struct KatuiV3 b);

struct KatuiV3 katui_v3_sub(struct KatuiV3 a, struct KatuiV3 b);

struct KatuiV3 katui_v3_mul(struct KatuiV3 a, float scale);

float katui_v3_dot(struct KatuiV3 a, struct KatuiV3 b);

struct KatuiV3 katui_v3_cross(struct KatuiV3 a, struct KatuiV3 b);

struct KatuiV3 katui_v3_normalized(struct KatuiV3 a);

struct KatuiV3 katui_v3_rotated_x(struct KatuiV3 a, float angle);

struct KatuiV3 katui_v3_rotated_y(struct KatuiV3 a, float angle);

struct KatuiV3 katui_v3_rotated_z(struct KatuiV3 a, float angle);

/**
 * Write where `at` lands on screen into `on_screen`, and how many pixels
 * one world unit spans there into `scale`. False behind the eye.
 */
bool katui_camera_project(const struct KatuiCamera *camera,
                          struct KatuiV3 at,
                          struct KatuiPoint *on_screen,
                          float *scale);

struct KatuiRng *katui_rng_new(uint32_t seed);

void katui_rng_free(struct KatuiRng *rng);

uint32_t katui_rng_next_u32(struct KatuiRng *rng);

/**
 * A value from `low` up to `high`.
 */
float katui_rng_range(struct KatuiRng *rng, float low, float high);

/**
 * A random direction of unit length.
 */
struct KatuiV3 katui_rng_direction(struct KatuiRng *rng);

struct KatuiSparks *katui_sparks_new(void);

void katui_sparks_free(struct KatuiSparks *sparks);

void katui_sparks_add(struct KatuiSparks *sparks, struct KatuiSpark spark);

/**
 * Age and move every spark by `dt` seconds under `gravity`.
 */
void katui_sparks_step(struct KatuiSparks *sparks, float dt, struct KatuiV3 gravity);

/**
 * Draw every spark as a fading streak, placed by `place(user, at, out)`.
 */
void katui_sparks_draw(const struct KatuiSparks *sparks,
                       struct KatuiLight *light,
                       KatuiPlace place,
                       void *user);

size_t katui_sparks_len(const struct KatuiSparks *sparks);

bool katui_sparks_is_empty(const struct KatuiSparks *sparks);

/**
 * Run `game` on the terminal until the player quits or it finishes.
 * `game` is copied; `user` has to live until this returns.
 */
KatuiStatus katui_play(const struct KatuiGame *game);

/**
 * `game` drawn off-terminal at `width` by `height` after `steps` steps of
 * `dt` seconds, with the `held_count` keys named at `held` held down, for
 * tests and screenshots. The caller owns the image; NULL on failure.
 */
struct KatuiImage *katui_frame(const struct KatuiGame *game,
                               uint32_t width,
                               uint32_t height,
                               uint32_t steps,
                               const char *const *held,
                               size_t held_count,
                               float dt);

/**
 * The element id a link to `#fragment` reaches, for `katui_element_reveal`.
 */
uint64_t katui_anchor_id(const char *fragment);

/**
 * An HTML document as an element.
 */
struct KatuiElement *katui_html(const char *source, const struct KatuiDocument *document);

/**
 * A Markdown document as an element.
 */
struct KatuiElement *katui_markdown(const char *source, const struct KatuiDocument *document);

/**
 * A press of the key `name`, for tests and for handing keys to widgets.
 * NULL when `name` names no key.
 */
struct KatuiEvent *katui_event_press(const char *name,
                                     bool ctrl,
                                     bool alt,
                                     bool shift,
                                     bool super_key,
                                     bool hyper,
                                     bool meta);

/**
 * A paste of `text`.
 */
struct KatuiEvent *katui_event_paste(const char *text);

/**
 * A copy of `event` the caller owns.
 */
struct KatuiEvent *katui_event_clone(const struct KatuiEvent *event);

/**
 * Free an event the caller owns. NULL does nothing.
 */
void katui_event_free(struct KatuiEvent *event);

KatuiEventKind katui_event_kind(const struct KatuiEvent *event);

/**
 * The key's name: one character, `"enter"`, `"up"`, `"f1"` and so on, or
 * NULL for an event without a key. The caller frees it with
 * `katui_string_free`.
 */
char *katui_event_key(const struct KatuiEvent *event);

bool katui_event_ctrl(const struct KatuiEvent *event);

bool katui_event_alt(const struct KatuiEvent *event);

bool katui_event_shift(const struct KatuiEvent *event);

bool katui_event_super(const struct KatuiEvent *event);

bool katui_event_hyper(const struct KatuiEvent *event);

bool katui_event_meta(const struct KatuiEvent *event);

/**
 * The pasted text, or NULL for another event. The caller frees it with
 * `katui_string_free`.
 */
char *katui_event_text(const struct KatuiEvent *event);

/**
 * Whether the window has the focus, for a focus event.
 */
bool katui_event_focused(const struct KatuiEvent *event);

/**
 * Write the cell under the pointer into `column` and `row`. False for an
 * event without a pointer.
 */
bool katui_event_position(const struct KatuiEvent *event, uint32_t *column, uint32_t *row);

KatuiMouseAction katui_event_action(const struct KatuiEvent *event);

/**
 * The button of a press, release or drag.
 */
KatuiButton katui_event_button(const struct KatuiEvent *event);

/**
 * A field holding `text`, the cursor at its end. NULL starts empty.
 */
struct KatuiTextField *katui_text_field_new(const char *text);

void katui_text_field_free(struct KatuiTextField *field);

/**
 * The text, which the caller frees with `katui_string_free`.
 */
char *katui_text_field_text(const struct KatuiTextField *field);

void katui_text_field_set_text(struct KatuiTextField *field, const char *text);

/**
 * The text before the cursor into `before` and after it into `after`,
 * each freed by the caller with `katui_string_free`.
 */
void katui_text_field_split(const struct KatuiTextField *field, char **before, char **after);

/**
 * Empty the field and return what it held, freed by the caller with
 * `katui_string_free`.
 */
char *katui_text_field_take(struct KatuiTextField *field);

/**
 * Typing, the cursor keys and paste.
 */
KatuiEdit katui_text_field_handle(struct KatuiTextField *field, const struct KatuiEvent *event);

/**
 * An empty area showing `rows` lines.
 */
struct KatuiTextArea *katui_text_area_new(size_t rows);

void katui_text_area_free(struct KatuiTextArea *area);

/**
 * The text, which the caller frees with `katui_string_free`.
 */
char *katui_text_area_text(const struct KatuiTextArea *area);

void katui_text_area_set_text(struct KatuiTextArea *area, const char *text);

/**
 * Empty the area and return what it held, freed by the caller with
 * `katui_string_free`.
 */
char *katui_text_area_take(struct KatuiTextArea *area);

size_t katui_text_area_rows(const struct KatuiTextArea *area);

size_t katui_text_area_cursor_line(const struct KatuiTextArea *area);

KatuiEdit katui_text_area_handle(struct KatuiTextArea *area, const struct KatuiEvent *event);

struct KatuiListState *katui_list_state_new(size_t length, size_t visible);

void katui_list_state_free(struct KatuiListState *state);

void katui_list_state_resize(struct KatuiListState *state, size_t length, size_t visible);

size_t katui_list_state_selected(const struct KatuiListState *state);

void katui_list_state_select(struct KatuiListState *state, size_t row);

/**
 * The rows in view, from `start` up to but not including `end`.
 */
void katui_list_state_in_view(const struct KatuiListState *state, size_t *start, size_t *end);

/**
 * Move the selection with the arrow, page, home and end keys. True when
 * it moved.
 */
bool katui_list_state_handle(struct KatuiListState *state, const struct KatuiEvent *event);

struct KatuiScrollState *katui_scroll_state_new(void);

void katui_scroll_state_free(struct KatuiScrollState *state);

float katui_scroll_state_offset(const struct KatuiScrollState *state);

float katui_scroll_state_max(const struct KatuiScrollState *state);

/**
 * Take the position a scroll view reported.
 */
void katui_scroll_state_apply(struct KatuiScrollState *state, struct KatuiScroll scroll);

void katui_scroll_state_scroll_by(struct KatuiScrollState *state, float pixels);

/**
 * Move with the arrow, page, home and end keys, `line` pixels a step.
 * True when it moved.
 */
bool katui_scroll_state_handle(struct KatuiScrollState *state,
                               const struct KatuiEvent *event,
                               float line);

/**
 * A node without children.
 */
struct KatuiTreeNode *katui_tree_node_leaf(uint64_t id, const char *text);

/**
 * A node with `count` children under it. Takes over every child; NULL
 * ones are left out.
 */
struct KatuiTreeNode *katui_tree_node_branch(uint64_t id,
                                             const char *text,
                                             struct KatuiTreeNode *const *children,
                                             size_t count);

void katui_tree_node_free(struct KatuiTreeNode *node);

struct KatuiTreeState *katui_tree_state_new(void);

void katui_tree_state_free(struct KatuiTreeState *state);

/**
 * Write the selected node's id into `id`. False when none is selected.
 */
bool katui_tree_state_selected(const struct KatuiTreeState *state, uint64_t *id);

void katui_tree_state_select(struct KatuiTreeState *state, uint64_t id);

bool katui_tree_state_is_open(const struct KatuiTreeState *state, uint64_t id);

void katui_tree_state_toggle(struct KatuiTreeState *state, uint64_t id);

/**
 * Move through the visible nodes of `roots` and open and close them.
 * True when something changed.
 */
bool katui_tree_state_handle(struct KatuiTreeState *state,
                             const struct KatuiEvent *event,
                             const struct KatuiTreeNode *const *roots,
                             size_t count);

/**
 * Focus over controls 0 to `count - 1`, the first one focused. NULL when
 * `count` is 0.
 */
struct KatuiFocus *katui_focus_new(size_t count);

void katui_focus_free(struct KatuiFocus *focus);

size_t katui_focus_current(const struct KatuiFocus *focus);

bool katui_focus_is(const struct KatuiFocus *focus, size_t control);

/**
 * Focus `control`. False when it is not one of the controls.
 */
bool katui_focus_set(struct KatuiFocus *focus, size_t control);

void katui_focus_next(struct KatuiFocus *focus);

void katui_focus_previous(struct KatuiFocus *focus);

/**
 * Move the focus on Tab and Shift+Tab. True when it moved.
 */
bool katui_focus_handle(struct KatuiFocus *focus, const struct KatuiEvent *event);

struct KatuiToasts *katui_toasts_new(void);

void katui_toasts_free(struct KatuiToasts *toasts);

/**
 * Show `text` for the default time.
 */
void katui_toasts_push(struct KatuiToasts *toasts, const char *text);

/**
 * Show `text` for `seconds`.
 */
void katui_toasts_push_for(struct KatuiToasts *toasts, const char *text, double seconds);

/**
 * Drop the toasts whose time is up. True when any are left.
 */
bool katui_toasts_expire(struct KatuiToasts *toasts);

/**
 * Write the seconds until the next toast leaves into `seconds`. False when
 * there is none.
 */
bool katui_toasts_next_expiry(const struct KatuiToasts *toasts, double *seconds);

bool katui_toasts_is_empty(const struct KatuiToasts *toasts);

/**
 * The text of the toast at `index`, oldest first, or NULL past the last.
 * The caller frees it with `katui_string_free`.
 */
char *katui_toasts_text(const struct KatuiToasts *toasts, size_t index);

/**
 * `app` on a screen of `width` by `height` pixels in cells of
 * `cell_width` by `cell_height`. `app` is copied; `user` has to live as
 * long as the screen. NULL when `app` lacks `view` or `update`.
 */
struct KatuiScreen *katui_screen_new(const struct KatuiApp *app,
                                     uint32_t width,
                                     uint32_t height,
                                     uint16_t cell_width,
                                     uint16_t cell_height);

/**
 * Free a screen. NULL does nothing.
 */
void katui_screen_free(struct KatuiScreen *screen);

/**
 * Draw the app's view and return the picture, which the caller owns. NULL
 * when a callback failed.
 */
struct KatuiImage *katui_screen_render(struct KatuiScreen *screen);

/**
 * Click at `x`, `y` pixels and deliver what the click sends.
 */
KatuiStatus katui_screen_click(struct KatuiScreen *screen, float x, float y);

/**
 * Press at `x`, `y` and keep holding, for a drag.
 */
KatuiStatus katui_screen_press(struct KatuiScreen *screen, float x, float y);

/**
 * Move a held press to `x`, `y`.
 */
KatuiStatus katui_screen_drag(struct KatuiScreen *screen, float x, float y);

/**
 * Let go of a held press.
 */
KatuiStatus katui_screen_release(struct KatuiScreen *screen);

/**
 * Turn the wheel once at `x`, `y`, down or up.
 */
KatuiStatus katui_screen_wheel(struct KatuiScreen *screen, float x, float y, bool down);

/**
 * Move the pointer to `x`, `y`: the element it left gets its leave
 * message, the one it entered its hover message.
 */
KatuiStatus katui_screen_hover(struct KatuiScreen *screen, float x, float y);

/**
 * Hand `event`, which stays the caller's, to the app's `input`.
 */
KatuiStatus katui_screen_send(struct KatuiScreen *screen, const struct KatuiEvent *event);

/**
 * Type `text`, one key press per character.
 */
KatuiStatus katui_screen_type_text(struct KatuiScreen *screen, const char *text);

/**
 * Whether the app asked to quit.
 */
bool katui_screen_quit(const struct KatuiScreen *screen);

/**
 * The dark theme for cells of `cell_width` by `cell_height` pixels.
 */
struct KatuiTheme *katui_theme_dark(uint16_t cell_width, uint16_t cell_height);

/**
 * The light theme for cells of `cell_width` by `cell_height` pixels.
 */
struct KatuiTheme *katui_theme_light(uint16_t cell_width, uint16_t cell_height);

/**
 * A copy of the theme the widgets draw with right now: the app's, inside
 * a view.
 */
struct KatuiTheme *katui_theme_current(void);

/**
 * A new theme, `theme` with every size times `factor`.
 */
struct KatuiTheme *katui_theme_scaled(const struct KatuiTheme *theme, float factor);

void katui_theme_free(struct KatuiTheme *theme);

/**
 * Write the cell size the sizes follow into `width` and `height`.
 */
void katui_theme_cell(const struct KatuiTheme *theme, uint16_t *width, uint16_t *height);

float katui_theme_cell_width(const struct KatuiTheme *theme);

float katui_theme_cell_height(const struct KatuiTheme *theme);

/**
 * The body text size, in pixels.
 */
float katui_theme_body(const struct KatuiTheme *theme);

float katui_theme_small(const struct KatuiTheme *theme);

float katui_theme_heading(const struct KatuiTheme *theme);

float katui_theme_title(const struct KatuiTheme *theme);

/**
 * The corner radius, in pixels.
 */
float katui_theme_radius(const struct KatuiTheme *theme);

/**
 * `count` cells across, in pixels.
 */
float katui_theme_columns(const struct KatuiTheme *theme, float count);

/**
 * `count` cells down, in pixels.
 */
float katui_theme_rows(const struct KatuiTheme *theme, float count);

/**
 * The accent color while `active`, the border color otherwise.
 */
struct KatuiColor katui_theme_outline(const struct KatuiTheme *theme, bool active);

/**
 * The page behind everything.
 */
struct KatuiColor katui_theme_background(const struct KatuiTheme *theme);

void katui_theme_set_background(struct KatuiTheme *theme, struct KatuiColor color);

/**
 * Panels and fields.
 */
struct KatuiColor katui_theme_raised(const struct KatuiTheme *theme);

void katui_theme_set_raised(struct KatuiTheme *theme, struct KatuiColor color);

/**
 * Hovered or selected rows.
 */
struct KatuiColor katui_theme_highlight(const struct KatuiTheme *theme);

void katui_theme_set_highlight(struct KatuiTheme *theme, struct KatuiColor color);

/**
 * Hairline borders.
 */
struct KatuiColor katui_theme_border(const struct KatuiTheme *theme);

void katui_theme_set_border(struct KatuiTheme *theme, struct KatuiColor color);

/**
 * Body text.
 */
struct KatuiColor katui_theme_text(const struct KatuiTheme *theme);

void katui_theme_set_text(struct KatuiTheme *theme, struct KatuiColor color);

/**
 * Hints, placeholders and secondary text.
 */
struct KatuiColor katui_theme_muted(const struct KatuiTheme *theme);

void katui_theme_set_muted(struct KatuiTheme *theme, struct KatuiColor color);

/**
 * The color of the action that matters most.
 */
struct KatuiColor katui_theme_accent(const struct KatuiTheme *theme);

void katui_theme_set_accent(struct KatuiTheme *theme, struct KatuiColor color);

/**
 * Text on the accent color.
 */
struct KatuiColor katui_theme_on_accent(const struct KatuiTheme *theme);

void katui_theme_set_on_accent(struct KatuiTheme *theme, struct KatuiColor color);

/**
 * Errors.
 */
struct KatuiColor katui_theme_danger(const struct KatuiTheme *theme);

void katui_theme_set_danger(struct KatuiTheme *theme, struct KatuiColor color);

struct KatuiElement *katui_label(const char *text);

struct KatuiElement *katui_title(const char *text);

struct KatuiElement *katui_heading(const char *text);

struct KatuiElement *katui_hint(const char *text);

/**
 * A button that sends `key` when clicked.
 */
struct KatuiElement *katui_button(const char *text, uint64_t key);

/**
 * A button that sends `key` when clicked; `focused` rings it in the
 * accent color to show where the keys act.
 */
struct KatuiElement *katui_focusable_button(const char *text, uint64_t key, bool focused);

/**
 * A button in the accent color that sends `key` when clicked.
 */
struct KatuiElement *katui_primary_button(const char *text, uint64_t key);

/**
 * A raised card around `count` children. Takes over every child; NULL ones
 * are left out.
 */
struct KatuiElement *katui_panel(struct KatuiElement *const *children, size_t count);

/**
 * The theme's background filling the screen, around `count` children.
 * Takes over every child; NULL ones are left out.
 */
struct KatuiElement *katui_page(struct KatuiElement *const *children, size_t count);

struct KatuiElement *katui_text_field(const struct KatuiTextField *field,
                                      const char *placeholder,
                                      bool focused);

struct KatuiElement *katui_text_area(const struct KatuiTextArea *area,
                                     const char *placeholder,
                                     bool focused);

/**
 * The rows of `state` in view, each built by `row(user, index, selected)`
 * during this call.
 */
struct KatuiElement *katui_list(const struct KatuiListState *state, KatuiRow row, void *user);

/**
 * A checkbox that sends `key` when clicked.
 */
struct KatuiElement *katui_checkbox(const char *text, bool checked, uint64_t key);

/**
 * A switch that sends `key` when clicked.
 */
struct KatuiElement *katui_switch(const char *text, bool on, uint64_t key);

/**
 * A bar filled to `fraction`, from 0 to 1.
 */
struct KatuiElement *katui_progress(float fraction);

/**
 * A row of `count` tabs; a click sends `key` with the tab's index.
 */
struct KatuiElement *katui_tabs(const char *const *titles,
                                size_t count,
                                size_t selected,
                                uint64_t key);

/**
 * `count` options, one picked; a click sends `key` with the option's index.
 */
struct KatuiElement *katui_radio(const char *const *options,
                                 size_t count,
                                 size_t selected,
                                 uint64_t key);

/**
 * A slider at `value`, from 0 to 1; a drag sends `key` with each new value.
 */
struct KatuiElement *katui_slider(float value, uint64_t key);

/**
 * Write `value` moved by `step` for the arrow keys into `moved`, between 0
 * and 1. False when `event` is not an arrow key.
 */
bool katui_nudge(float value, const struct KatuiEvent *event, float step, float *moved);

/**
 * Three dots with one lit, moving every quarter second of `elapsed`
 * seconds.
 */
struct KatuiElement *katui_spinner(double elapsed);

struct KatuiElement *katui_toasts(const struct KatuiToasts *toasts);

/**
 * A line of `count` keys and what they do.
 */
struct KatuiElement *katui_key_hints(const struct KatuiKeyHint *hints, size_t count);

/**
 * A card over a dimmed screen; a click outside it sends `dismiss_key`.
 * Takes over `body` and every action.
 */
struct KatuiElement *katui_dialog(const char *title,
                                  struct KatuiElement *body,
                                  struct KatuiElement *const *actions,
                                  size_t count,
                                  uint64_t dismiss_key);

/**
 * A field showing the option at `selected` that opens a list of `count`
 * options. A click on the field sends `toggle_key`, one on an option
 * `choose_key` with its index.
 */
struct KatuiElement *katui_select(const char *const *options,
                                  size_t count,
                                  size_t selected,
                                  bool open,
                                  uint64_t toggle_key,
                                  uint64_t choose_key);

/**
 * A menu title that opens `count` items. A click on the title sends
 * `toggle_key`, one on an item `choose_key` with its index.
 */
struct KatuiElement *katui_menu(const char *title,
                                const char *const *items,
                                size_t count,
                                bool open,
                                uint64_t toggle_key,
                                uint64_t choose_key);

/**
 * `anchor` with `text` beside it while `shown`. The pointer entering sends
 * `show_key`, leaving `hide_key`. Takes over `anchor`.
 */
struct KatuiElement *katui_tooltip(struct KatuiElement *anchor,
                                   const char *text,
                                   bool shown,
                                   uint64_t show_key,
                                   uint64_t hide_key);

/**
 * `count` children in a box that scrolls; each new position sends `key`
 * with a scroll payload. Takes over every child.
 */
struct KatuiElement *katui_scroll_view(const struct KatuiScrollState *state,
                                       struct KatuiElement *const *content,
                                       size_t count,
                                       uint64_t key);

/**
 * Two panes and a divider, `first` taking `fraction` of the space. A drag
 * on the divider sends `key` with the new fraction. Takes over both panes.
 */
struct KatuiElement *katui_split(struct KatuiElement *first,
                                 struct KatuiElement *second,
                                 float fraction,
                                 KatuiOrientation orientation,
                                 uint64_t key);

/**
 * The rows of `state` in view under a header of `column_count` columns.
 * `cells` holds `row_count` rows of `column_count` strings each, row by
 * row. A click on a row sends `key` with its index.
 */
struct KatuiElement *katui_table(const struct KatuiColumn *columns,
                                 size_t column_count,
                                 const char *const *cells,
                                 size_t row_count,
                                 const struct KatuiListState *state,
                                 uint64_t key);

/**
 * The visible nodes of the `count` trees at `roots`, indented by depth. A
 * click on a node's marker sends `toggle_key` with its id, one on the node
 * `select_key` with its id. The nodes stay the caller's.
 */
struct KatuiElement *katui_tree(const struct KatuiTreeNode *const *roots,
                                size_t count,
                                const struct KatuiTreeState *state,
                                uint64_t toggle_key,
                                uint64_t select_key);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* KATUI_H */
