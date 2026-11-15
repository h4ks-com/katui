// A tiny browser for local HTML pages: links open the next page, the wheel
// and the arrow keys scroll, Backspace goes back.

import { existsSync, readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import {
  type Action,
  App,
  Control,
  column,
  type Event,
  html,
  Image,
  run,
  type Scroll,
  ScrollState,
  Theme,
  text,
} from '@matheusfillipe/katui'

export const SHOWCASE = fileURLToPath(
  new URL('../../../crates/katui-app/examples/pages/showcase.html', import.meta.url),
)

export class Browser extends App<Action> {
  history: string[]
  scroll = new ScrollState()
  images = new Map<string, Image>()

  constructor(start: string) {
    super()
    this.history = [resolve(start)]
  }

  get here(): string {
    return this.history.at(-1) ?? SHOWCASE
  }

  follow(href: string) {
    const target = resolve(dirname(this.here), href)
    if (existsSync(target)) {
      this.history.push(target)
      this.scroll = new ScrollState()
    }
  }

  image(src: string): Image | undefined {
    const path = resolve(dirname(this.here), src)
    if (!this.images.has(path) && existsSync(path)) {
      this.images.set(path, Image.open(path))
    }
    return this.images.get(path)
  }

  scrolled(scroll: Scroll) {
    this.scroll.apply(scroll)
  }

  document() {
    try {
      return html(readFileSync(this.here, 'utf8'), {
        onLink: (href) => this.follow(href),
        images: (src) => this.image(src),
      })
    } catch (error) {
      return text(`Check the path, the page did not open: ${error}`)
    }
  }

  view() {
    const theme = Theme.current()
    return column(this.document())
      .scroll(this.scroll.offset)
      .onScroll((scroll) => this.scrolled(scroll))
      .scrollbar(theme.muted)
      .padding(theme.cellHeight())
      .size('fill', 'fill')
      .background(theme.background ?? theme.text)
  }

  input(event: Event) {
    if (event.key === 'backspace' && this.history.length > 1) {
      this.history.pop()
      return Control.Redraw
    }
    return this.scroll.handle(event, 40) ? Control.Redraw : undefined
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  run(new Browser(process.argv[2] ?? SHOWCASE))
}
