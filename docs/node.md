# Node.js

The `katui` npm package runs the same engine as the Rust crates: apps with
widgets, HTML and Markdown documents, and games. It ships its own
TypeScript types. This guide builds a small app and a small game, and shows
how to test both without a terminal.

Install it on Linux or macOS, with Node.js 22.18 or newer:

```sh
npm install @matheusfillipe/katui
```

## An app

An app is a class with a `view` that returns elements. Here is a shopping
list you type into:

```ts
import { App, Control, Edit, type Event, TextField, checkbox, panel, run, textField, title } from '@matheusfillipe/katui'

class Shopping extends App {
  items: { name: string; bought: boolean }[] = []
  draft = new TextField()

  view() {
    return panel(
      title('Shopping'),
      textField(this.draft, 'Add an item'),
      this.items.map((item) =>
        checkbox(item.name, item.bought, () => {
          item.bought = !item.bought
        }),
      ),
    )
  }

  input(event: Event) {
    if (this.draft.handle(event) === Edit.Submitted && this.draft.text.trim()) {
      this.items.push({ name: this.draft.take().trim(), bought: false })
    }
    return Control.Redraw
  }
}

run(new Shopping())
```

Save it as `shopping.ts` and run it with `node shopping.ts`, type an item
and press <kbd>Enter</kbd>. Click an item to tick it.
<kbd>Ctrl</kbd>+<kbd>C</kbd> quits.

katui calls `view` again after every change and sends the terminal only the
parts that changed. Children can be elements, arrays of elements, or
`null`, `undefined` and `false`, which are left out, so
`column(header, done && footer)` works.

`run` returns when the app quits, and until then it keeps the thread busy:
timers, promises and other callbacks wait. To do something on a beat, use
the waker described below.

## Messages

A clickable element takes a message: any JavaScript value. When it is
clicked, katui hands the message to the app's `update`. The default
`update` calls the message when it is a function, so an arrow function
works as a click handler:

```ts
button('Save', () => this.save()) // calls this.save()
button('Save', 'save') // update receives 'save'
```

Pass an arrow function, since a bare `this.save` loses its `this`.

`update` returns what to do next, and nothing redraws once:

| `Control` | What happens |
|---|---|
| `Redraw` | katui calls `view` once and draws the changes |
| `Wait` | nothing is drawn until the next event |
| `Animate` | katui keeps drawing at 30 frames a second |
| `Quit` | the app ends and the terminal is restored |

Widgets that report a value take a function that makes the message from it:
`slider(value, change)` calls `change(value)`, and `tabs`, `radio`,
`select`, `menu` and `table` call theirs with the index picked. Such a
function can do the work itself and return nothing.

`start` receives a `Waker`. `waker.every(seconds, message)` delivers a
message on a steady beat, for a clock or a spinner, and `waker.send(message)`
delivers one right away.

### Typed messages

An app names its message type: `class Editor extends App<Message>`. Every
element carries the type of what it sends, as `Element<M>`, so tsc rejects
a button whose message is not a `Message`, and `update` receives a
`Message`:

```ts
import { App, button, Control, type Element, panel, run } from '@matheusfillipe/katui'

type Message = 'save' | 'quit'

class Editor extends App<Message> {
  view(): Element<Message> {
    return panel(button('Save', 'save'), button('Quit', 'quit'))
  }

  update(message: Message): Control | undefined {
    return message === 'quit' ? Control.Quit : undefined
  }
}

run(new Editor())
```

An app that acts in its callbacks is an `App<Action>`: an `Action` is a
function `update` calls, or nothing when a value callback already did the
work.

## Keys

Keys and pastes go to `input`, which receives an `Event` and returns a
`Control`, or nothing to wait. `event.key` is the key's name: one
character, or `'enter'`, `'tab'`, `'escape'`, `'up'`, `'page_down'`,
`'f1'` and so on.

The widgets that keep state have a state object you store on the app and
pass to `view`. Each one takes events with `handle`:

| State | Widget | What `handle` does |
|---|---|---|
| `TextField` | `textField` | typing, the cursor keys and paste; returns an `Edit` |
| `TextArea` | `textArea` | the same over several lines |
| `ListState` | `listView`, `table` | moves the selection; returns whether it moved |
| `ScrollState` | `scrollView` | scrolls with the arrow, page, home and end keys |
| `TreeState` | `tree` | moves through nodes and opens and closes them |
| `Focus` | | moves the focus between your controls with <kbd>Tab</kbd> |

## Themes

The app's `theme` method picks the theme, and katui asks for it before every
frame, so returning another one switches the look at once:

```ts
class Styled extends App {
  light = false
  zoom = 1

  view() {
    return page(label('Hello'))
  }

  theme(cell: [number, number]) {
    const theme = this.light ? Theme.light(cell) : Theme.dark(cell)
    theme.accent = '#e0559b'
    return theme.scaled(this.zoom)
  }
}
```

Inside `view`, `Theme.current()` gives the theme's colors and sizes for your
own elements, like `label('Saved').color(Theme.current().accent)`.
`theme.columns(n)` and `theme.rows(n)` give a size of `n` terminal cells.

## Layout

`row`, `column` and `stack` place their children with flexbox. Methods on an
element change it and return it, so calls chain:

```ts
column(header, body).gap(8).padding(16).size('fill', 'fill').align('center')
```

A size is a number of pixels, `'fill'` or `'content'`. A color is
`[r, g, b]`, `[r, g, b, a]` or `'#rrggbb'`. Alignment is `'start'`,
`'center'` or `'end'`.

## Documents

`html(source)` and `markdown(source)` return an element. A click on a link
calls `onLink(href)`, or sends the href itself when you leave `onLink` out.
Images come from `images`: an object from `src` to `Image`, or a function
that returns one. `Image.open(path)` reads a PNG, JPEG, GIF or WebP file:

```ts
markdown(source, { onLink: (href) => this.open(href), images: { 'logo.png': logo } })
```

## Games

A game is a class with `step` and `render`. `play` runs it at 60 steps a
second:

```ts
import { Game, type Keys, type Light, play, srgb } from '@matheusfillipe/katui'

class Ball extends Game {
  x = 40

  step(keys: Keys, dt: number) {
    this.x += 60 * dt * (Number(keys.isHeld('right')) - Number(keys.isHeld('left')))
  }

  render(light: Light) {
    light.dim(0.8)
    light.glow([this.x, 30], 6, srgb(255, 140, 60))
  }
}

play(new Ball())
```

`light` is a buffer of light, not of colors: shapes add to what is there,
values above one bloom, and `dim` fades the last frame into a trail.
`srgb` turns an ordinary color into light. Positions are `[x, y]` arrays
in the buffer's pixels. For 3D there are `V3`, `Camera`, `Rng` and `Sparks`.
`press` and `repeat` get key names, and `finished` ends the game when it
returns `true`.

## Tests

`Screen` runs an app without a terminal. `render` returns an `Image`, and
`click`, `wheel`, `hover`, `send` and `typeText` deliver input the way the
terminal would:

```ts
const screen = new Screen(new Counter(), 480, 240)
screen.render().save('counter.png')
screen.click(240, 72)
```

`frame(game, { steps: 10, held: ['right'] })` steps a game and returns its
picture. An exception thrown in your code reaches the caller of `render`,
`click` or `frame` as it was thrown, and the same happens with `run` and
`play`, after the terminal is restored.

## Differences from Rust

The names follow the Rust API in camelCase, with a few changes:

- `switch` is `toggleSwitch`, since `switch` is a JavaScript keyword, and
  `list` is `listView`.
- Enums the Rust API passes as values are strings: split orientation
  (`'horizontal'`), blend modes (`'add'`), key names (`'left'`).
- Points, light colors and `V3` arguments take arrays, and a scroll
  position is a plain `{ offset, max }` object.
- `V3` has `add`, `sub`, `mul` and `equals` in place of operators.
- `Event.press('a', { ctrl: true })` builds a key event, for tests and for
  handing keys to widgets.

The [examples](../bindings/node/examples) include a to-do list, the full
widget tour, a Markdown help screen, an HTML browser and a comet game. Run
one from a clone with `just node-demo widgets`.
