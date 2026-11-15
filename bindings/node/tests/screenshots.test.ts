// Every example drawn after a little use, for the screenshots in examples/README.md.

import { test } from 'node:test'
import { Event, frame, Screen } from '@matheusfillipe/katui'
import { Browser, SHOWCASE } from '../examples/browse.ts'
import { Comet } from '../examples/comet.ts'
import { Counter } from '../examples/counter.ts'
import { Help } from '../examples/help.ts'
import { Todo } from '../examples/todo.ts'
import { Tour } from '../examples/widgets.ts'
import { shot } from './paths.ts'

const WIDTH = 900
const HEIGHT = 560

function screenshot(name: string): string {
  return shot(`example-${name}`)
}

test('counter screenshot', () => {
  const app = new Counter()
  for (let i = 0; i < 3; i++) app.add()
  new Screen(app, WIDTH, HEIGHT).render().save(screenshot('counter'))
})

test('todo screenshot', () => {
  const screen = new Screen(new Todo(), WIDTH, HEIGHT)
  for (const task of ['buy bread', 'call the bank']) {
    screen.typeText(task)
    screen.send(Event.press('enter'))
  }
  screen.typeText('walk the dog')
  screen.render().save(screenshot('todo'))
})

test('widgets screenshot', () => {
  const app = new Tour()
  app.size = 0
  app.busy = true
  new Screen(app, WIDTH, HEIGHT).render().save(screenshot('widgets'))
})

test('help screenshot', () => {
  new Screen(new Help(), WIDTH, HEIGHT).render().save(screenshot('help'))
})

test('browse screenshot', () => {
  new Screen(new Browser(SHOWCASE), WIDTH, HEIGHT).render().save(screenshot('browse'))
})

test('comet screenshot', () => {
  const game = new Comet()
  frame(game, { width: 360, height: 224, steps: 40, held: ['right', 'down'] })
  game.press(' ')
  frame(game, { width: 360, height: 224, steps: 20, held: ['right'] }).save(screenshot('comet'))
})
