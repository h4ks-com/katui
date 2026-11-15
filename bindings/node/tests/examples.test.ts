// Every example renders headless and answers its clicks and keys.

import assert from 'node:assert/strict'
import { test } from 'node:test'
import { Control, Event, frame, type Image, Screen } from 'katui'
import { Browser, SHOWCASE } from '../examples/browse.ts'
import { Comet } from '../examples/comet.ts'
import { Counter } from '../examples/counter.ts'
import { Help } from '../examples/help.ts'
import { Todo } from '../examples/todo.ts'
import { Tour } from '../examples/widgets.ts'
import { shot } from './paths.ts'

function lit(image: Image): number {
  return image.rgba().filter((value, index) => index % 4 === 0 && value > 40).length
}

test('the counter counts clicks', () => {
  const app = new Counter()
  const screen = new Screen(app, 480, 240)
  screen.render().save(shot('counter'))
  screen.click(240, 72)
  screen.click(240, 72)
  assert.equal(app.count, 2)
})

test('todo adds typed tasks and filters them', () => {
  const app = new Todo()
  const screen = new Screen(app, 900, 480)
  screen.render()
  screen.typeText('buy bread')
  screen.send(Event.press('enter'))
  assert.deepEqual(app.tasks.at(-1), { text: 'buy bread', done: false })
  app.pickFilter(2)
  assert.deepEqual(app.shown(), [1])
  screen.render().save(shot('todo'))
})

test('the tour switches theme and size', () => {
  const app = new Tour()
  const screen = new Screen(app, 1200, 700)
  const dark = screen.render()
  app.pickTheme(2)
  app.size = 2
  const light = screen.render()
  light.save(shot('widgets'))
  assert.notDeepEqual(dark.pixel(600, 600), light.pixel(600, 600))
})

test('the tour quits through its dialog', () => {
  const app = new Tour()
  app.file(2)
  assert.equal(app.popup, 'quit')
  const screen = new Screen(app, 1200, 700)
  screen.render().save(shot('widgets-dialog'))
  assert.equal(
    app.update(() => Control.Quit),
    Control.Quit,
  )
})

test('help links switch pages', () => {
  const app = new Help()
  const screen = new Screen(app, 700, 400)
  screen.render().save(shot('help'))
  app.open('keys')
  assert.equal(app.current, 'keys')
  screen.render().save(shot('help-keys'))
})

test('the browser opens the showcase and scrolls', () => {
  const app = new Browser(SHOWCASE)
  const screen = new Screen(app, 1000, 700)
  screen.render().save(shot('browse'))
  screen.wheel(500, 350)
  screen.render()
  assert.ok(app.scroll.offset > 0)
})

test('the comet glows and bursts', () => {
  const game = new Comet()
  const still = frame(game, { width: 200, height: 120 })
  game.press(' ')
  const moved = frame(game, { width: 200, height: 120, steps: 20, held: ['right'] })
  moved.save(shot('comet'))
  assert.ok(lit(moved) > 0)
  assert.ok(game.at[0] > 60)
  assert.notEqual(still.pixel(0, 0), null)
})
