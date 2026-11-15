// The bindings' behavior: messages, widgets, themes, documents, errors.

import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  App,
  anchorId,
  button,
  Camera,
  Control,
  column,
  Edit,
  type Element,
  Event,
  Focus,
  frame,
  Game,
  html,
  Image,
  type Keys,
  type Light,
  ListState,
  label,
  markdown,
  page,
  panel,
  Rng,
  Screen,
  slider,
  srgb,
  TextField,
  Theme,
  Toasts,
  TreeNode,
  TreeState,
  tabs,
  V3,
} from '@matheusfillipe/katui'

class Recorder extends App {
  messages: unknown[] = []
  build: (app: Recorder) => Element

  constructor(build: (app: Recorder) => Element) {
    super()
    this.build = build
  }

  view() {
    return this.build(this)
  }

  update(message: unknown) {
    this.messages.push(message)
    return undefined
  }
}

test('a button hands its message to update as is', () => {
  const app = new Recorder(() => panel(button('Go', ['go', 1])))
  const screen = new Screen(app, 300, 120)
  screen.render()
  screen.click(40, 40)
  assert.deepEqual(app.messages, [['go', 1]])
})

test('a slider calls its function with the value', () => {
  const app = new Recorder(() => page(slider(0.5, (value) => ['volume', value])))
  const screen = new Screen(app, 400, 100)
  screen.render()
  screen.click(300, 30)
  const [name, value] = app.messages[0] as [string, number]
  assert.equal(name, 'volume')
  assert.ok(value > 0.5)
})

test('tabs send the index picked', () => {
  const app = new Recorder(() => page(tabs(['One', 'Two'], 0, (index) => index)))
  const screen = new Screen(app, 400, 100)
  screen.render()
  screen.click(110, 30)
  assert.deepEqual(app.messages, [1])
})

test('the default update calls a function message', () => {
  const calls: number[] = []
  class Plain extends App {
    view() {
      return panel(button('Go', () => calls.push(1)))
    }
  }
  const screen = new Screen(new Plain(), 300, 120)
  screen.render()
  screen.click(40, 40)
  assert.deepEqual(calls, [1])
})

test('a plain object without update runs its function messages', () => {
  let clicks = 0
  const app = {
    view: () =>
      panel(
        button('Go', () => {
          clicks += 1
        }),
      ),
  }
  const screen = new Screen(app, 300, 120)
  screen.render()
  screen.click(40, 40)
  assert.equal(clicks, 1)
})

test('widgets draw with the apps theme', () => {
  class Paper extends App {
    view() {
      return page(label(String(Theme.current().cell)))
    }

    theme(cell: [number, number]) {
      return Theme.light(cell)
    }
  }
  const image = new Screen(new Paper(), 200, 100).render()
  assert.deepEqual(image.pixel(199, 99), Theme.light().background)
})

test('a theme takes colors as arrays or hex', () => {
  const theme = Theme.dark()
  theme.accent = '#ff8800'
  theme.raised = [1, 2, 3]
  assert.deepEqual(theme.accent, [255, 136, 0, 255])
  assert.deepEqual(theme.raised, [1, 2, 3, 255])
  assert.deepEqual(theme.scaled(2).cell, [20, 40])
  assert.throws(() => {
    theme.accent = '#12'
  }, /rrggbb/)
})

test('an error in view reaches the caller as thrown', () => {
  class Broken extends App {
    view(): Element {
      throw new RangeError('view broke')
    }
  }
  assert.throws(() => new Screen(new Broken()).render(), RangeError)
})

test('an error in update reaches the caller as thrown', () => {
  class Broken extends App {
    view() {
      return panel(button('Go', 'go'))
    }

    update(message: unknown): Control {
      throw new TypeError(`no ${message}`)
    }
  }
  const screen = new Screen(new Broken(), 300, 120)
  screen.render()
  assert.throws(() => screen.click(40, 40), { name: 'TypeError', message: 'no go' })
})

test('a wrong return value from update is an error', () => {
  const app = { view: () => panel(button('Go', 'go')), update: () => 'redraw' }
  const screen = new Screen(app as never, 300, 120)
  screen.render()
  assert.throws(() => screen.click(40, 40), /return a Control/)
})

test('a Control return quits', () => {
  class Quitter extends App {
    view() {
      return panel(button('Quit', () => Control.Quit))
    }
  }
  const screen = new Screen(new Quitter(), 300, 120)
  screen.render()
  screen.click(40, 40)
  assert.ok(screen.quit)
})

test('a markdown link sends its href by default', () => {
  const app = new Recorder(() => page(markdown('[open the keys](keys)')))
  const screen = new Screen(app, 400, 120)
  screen.render()
  for (let x = 20; x < 120 && app.messages.length === 0; x += 10) screen.click(x, 30)
  assert.deepEqual(app.messages.slice(0, 1), ['keys'])
})

test('html takes images from an object or a function', () => {
  const red = Image.fromRgba(2, 2, new Uint8Array(Array(4).fill([255, 0, 0, 255]).flat()))
  const source = '<img src="red.png" style="width: 40px; height: 40px">'
  for (const images of [{ 'red.png': red }, () => red]) {
    const app = new Recorder(() => column(html(source, { images })))
    const image = new Screen(app, 100, 100).render()
    assert.deepEqual(image.pixel(10, 10), [255, 0, 0, 255])
  }
})

test('reveal scrolls to a heading by its anchor and reports the offset', () => {
  const source = `${'filler\n\n'.repeat(20)}## The target\n\nend`
  const app = new Recorder(() =>
    column(markdown(source))
      .scroll(0)
      .reveal(anchorId('#the-target'))
      .onScroll((scroll) => scroll.offset)
      .size('fill', 'fill'),
  )
  new Screen(app, 200, 100).render()
  assert.equal(app.messages.length, 1)
  assert.ok((app.messages[0] as number) > 0)
  assert.equal(anchorId('#the-target'), anchorId('the-target'))
})

test('children skip nothing and open arrays', () => {
  const app = new Recorder(() => column(null, [label('a'), [label('b')]], false, label('c')))
  new Screen(app, 100, 100).render()
  assert.throws(() => column('a' as never), /give an Element/)
})

test('an element keeps its tree while JavaScript holds it', () => {
  const header = label('kept')
  const app = new Recorder(() => column(header, label('fresh')))
  const screen = new Screen(app, 200, 100)
  const first = screen.render()
  for (let frame = 0; frame < 50; frame++) screen.render()
  assert.deepEqual(screen.render().rgba(), first.rgba())
})

test('text fields and lists take events', () => {
  const field = new TextField('ab')
  assert.equal(field.handle(Event.press('c')), Edit.Changed)
  assert.equal(field.handle(Event.press('enter')), Edit.Submitted)
  assert.equal(field.take(), 'abc')
  const rows = new ListState(5, 3)
  assert.ok(rows.handle(Event.press('down')))
  assert.equal(rows.selected, 1)
  assert.deepEqual(rows.inView(), [0, 3])
  assert.equal(Event.press('a', { ctrl: true }).ctrl, true)
  assert.throws(() => Event.press('nothing'), /name a key/)
})

test('focus moves between any values', () => {
  const focus = new Focus(['note', 'table'])
  assert.ok(focus.is('note'))
  assert.ok(focus.handle(Event.press('tab')))
  assert.equal(focus.current(), 'table')
  assert.throws(() => new Focus([]), /at least one/)
})

test('trees, toasts and events keep their state', () => {
  const roots = [new TreeNode(1, 'src', [new TreeNode(2, 'main.ts')])]
  const state = new TreeState()
  state.handle(Event.press('down'), roots)
  assert.equal(state.selected, 1)
  const toasts = new Toasts()
  toasts.push('saved', 0)
  assert.ok(toasts.expire())
  assert.ok(toasts.isEmpty())
  const paste = Event.paste('hello')
  assert.deepEqual([paste.kind, paste.text, paste.key], ['paste', 'hello', null])
})

test('games draw through frame', () => {
  class Dot extends Game {
    x = 10

    step(keys: Keys, dt: number) {
      if (keys.isHeld('right')) this.x += 100 * dt
    }

    render(light: Light) {
      light.rect([this.x, 10], 6, 6, srgb(255, 255, 255))
    }
  }
  const dot = new Dot()
  const image = frame(dot, { width: 80, height: 40, steps: 6, held: ['right'] })
  assert.ok(Math.abs(dot.x - 20) < 1e-3)
  assert.notDeepEqual(image.pixel(22, 12), image.pixel(70, 30))
})

test('an error in a game reaches the caller as thrown', () => {
  class Broken extends Game {
    render(): void {
      throw new SyntaxError('render broke')
    }
  }
  assert.throws(() => frame(new Broken()), SyntaxError)
})

test('world math matches the engine', () => {
  assert.ok(new V3(1, 2, 3).add([1, 1, 1]).equals(new V3(2, 3, 4)))
  assert.ok(new V3(1, 0, 0).cross([0, 1, 0]).equals([0, 0, 1]))
  const unit = new V3(3, 0, 4).normalized().toArray()
  assert.ok(Math.abs((unit[0] ?? 0) - 0.6) < 1e-6 && Math.abs((unit[2] ?? 0) - 0.8) < 1e-6)
  const camera = new Camera([50, 50], 100, 10)
  assert.deepEqual(camera.project([0, 0, 0]), { at: [50, 50], scale: 10 })
  assert.equal(camera.project([0, 0, -20]), null)
  assert.equal(new Rng(3).range(0, 1), new Rng(3).range(0, 1))
})
