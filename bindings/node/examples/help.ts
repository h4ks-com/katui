// A help screen written in Markdown, its links switching pages.

import { fileURLToPath } from 'node:url'
import { App, hint, markdown, page, run } from '@matheusfillipe/katui'

const PAGES: Record<string, string> = {
  start: `# Help

Press **Ctrl+C** to quit. Read [the keys](keys) or [the credits](credits).

- Arrows move
- Enter opens
`,
  keys: '# Keys\n\n- **Up** and **Down** move\n- **Enter** opens\n\n[Back](start)',
  credits: '# Credits\n\nDrawn by katui in real pixels.\n\n[Back](start)',
}

export class Help extends App {
  current = 'start'

  open(name: string) {
    this.current = name in PAGES ? name : 'start'
  }

  view() {
    return page(
      markdown(PAGES[this.current] ?? '', { onLink: (href) => this.open(href) }),
      hint(`You are on the ${this.current} page`),
    )
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  run(new Help())
}
