// A click counter, the smallest katui program.

import { fileURLToPath } from 'node:url'
import { type Action, App, label, panel, primaryButton, run } from '@matheusfillipe/katui'

export class Counter extends App<Action> {
  count = 0

  view() {
    return panel(
      label(`Clicked ${this.count} times`),
      primaryButton('Add one', () => this.add()),
    ).align('center')
  }

  add() {
    this.count += 1
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  run(new Counter())
}
