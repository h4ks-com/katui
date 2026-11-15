# katui in Node.js and TypeScript

`npm install @mattfillipe/katui` (Node.js 22.18+, Linux or macOS). The
package ships its own types, and Node runs `.ts` files directly
(`node app.ts`). Everything imports from `'@mattfillipe/katui'`. Full
guide: https://github.com/h4ks-com/katui/blob/main/docs/node.md

## An app

```ts
import { App, Control, Edit, type Event, TextField, checkbox, panel, run, textField, title } from '@mattfillipe/katui'

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

Children can be elements, arrays, or `null`, `undefined` and `false` (left
out), so `column(header, done && footer)` works. Hooks on `App`:
`start(waker)`, `view()`, `update(message)`, `input(event)`,
`theme(cell)`. Returning nothing from `update` redraws once; returning
nothing from `input` waits.

`run` blocks the thread until the app quits: timers and promises do not run
meanwhile. Use the `Waker` from `start` for anything on a beat:
`waker.every(seconds, message)` and `waker.send(message)`.

## Messages

The default `update` calls a function message, so
`button('Save', () => this.save())` works in an `App<Action>`. Pass an arrow
function; a bare `this.save` loses its `this`. Typed messages:

```ts
import { App, button, Control, type Element, panel, run } from '@mattfillipe/katui'

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

Value widgets take a function that builds the message: `slider(value,
change)`; `tabs`, `radio`, `select`, `menu` and `table` pass the index
picked.

## Keys and state

`event.key` is one character or a name like `'enter'`, `'tab'`,
`'escape'`, `'up'`, `'page_down'`, `'f1'`. State objects take events with
`handle`: `TextField` (`textField`) and `TextArea` (`textArea`) return an
`Edit`, `ListState` (`listView`, `table`) returns whether it moved, plus
`ScrollState` (`scrollView`), `TreeState` (`tree`) and `Focus`.

## Layout and themes

```ts
column(header, body).gap(8).padding(16).size('fill', 'fill').align('center')
```

A size is pixels, `'fill'` or `'content'`. A color is `[r, g, b]`,
`[r, g, b, a]` or `'#rrggbb'`. Alignment is `'start'`, `'center'`, `'end'`.

```ts
theme(cell: [number, number]) {
  const theme = this.light ? Theme.light(cell) : Theme.dark(cell)
  theme.accent = '#e0559b'
  return theme.scaled(this.zoom)
}
```

`Theme.current()` inside `view`; `theme.columns(n)` and `theme.rows(n)`.

## Documents

```ts
markdown(source, { onLink: (href) => this.open(href), images: { 'logo.png': logo } })
```

`html(source, options)` takes the same options. Without `onLink` a click
sends the href. `Image.open(path)` reads PNG, JPEG, GIF or WebP.

## Games

```ts
import { Game, type Keys, type Light, play, srgb } from '@mattfillipe/katui'

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

Points are `[x, y]` arrays. `press`, `repeat`, `resize` and `finished` are
optional overrides.

## Tests

```ts
import { Event, Screen, frame } from '@mattfillipe/katui'

const screen = new Screen(new Counter(), 480, 240)
screen.render().save('counter.png')
screen.click(240, 72)
screen.send(Event.press('a', { ctrl: true }))
screen.typeText('milk')

const picture = frame(new Ball(), { steps: 10, held: ['right'] })
```

`Screen` also has `wheel`, `hover`, `press`, `drag`, `release` and the
`quit` getter. An exception in your code reaches the caller of `render`,
`click` or `frame` as thrown; `run` and `play` rethrow it after restoring
the terminal. Tests run with `node --test`.

## Differences from Rust

camelCase names. `switch` is `toggleSwitch`, `list` is `listView`. Enums
passed as values are strings. Points, light colors and `V3` arguments are
arrays; a scroll position is `{ offset, max }`. `V3` has `add`, `sub`,
`mul` and `equals`. Examples: https://github.com/h4ks-com/katui/tree/main/bindings/node/examples
