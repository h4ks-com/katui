import * as native from './index.js'

export * from './index.js'

export const row = (...children) => native.Element.row(children)
export const column = (...children) => native.Element.column(children)
export const stack = (...children) => native.Element.stack(children)
export const panel = (...children) => native.panel(children)
export const page = (...children) => native.page(children)
export const text = (content) => native.Element.text(content)
export const rich = (spans) => native.Element.rich(spans)
export const image = (picture) => native.Element.image(picture)
export const spacer = () => native.Element.spacer()

export function defaultUpdate(message) {
  return typeof message === 'function' ? message() : undefined
}

export class App {
  start(_waker) {}

  view() {
    throw new Error('return the view from App.view')
  }

  update(message) {
    return defaultUpdate(message)
  }

  input(_event) {
    return undefined
  }

  theme(cell) {
    return native.Theme.dark(cell)
  }
}

export class Game {
  press(_key) {}

  repeat(_key) {}

  resize(_light) {}

  step(_keys, _dt) {}

  render(_light) {}

  finished() {
    return false
  }
}
