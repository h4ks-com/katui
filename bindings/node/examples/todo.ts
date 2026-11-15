// A to-do list: type a task, press Enter to add it, Tab to tick the selected one.

import { fileURLToPath } from 'node:url'
import {
  App,
  Control,
  checkbox,
  Edit,
  type Event,
  hint,
  ListState,
  listView,
  page,
  panel,
  progress,
  run,
  TextField,
  Theme,
  tabs,
  textField,
  title,
} from '@matheusfillipe/katui'

const FILTERS = ['All', 'Active', 'Done']
const VISIBLE = 10

type Task = { text: string; done: boolean }

export class Todo extends App {
  tasks: Task[] = [
    { text: 'water the plants', done: false },
    { text: 'fix the bike', done: true },
  ]
  draft = new TextField()
  filter = 0
  selection = new ListState(this.tasks.length, VISIBLE)

  shown(): number[] {
    const wanted = [undefined, false, true][this.filter]
    return this.tasks.flatMap((task, index) =>
      wanted === undefined || task.done === wanted ? [index] : [],
    )
  }

  toggle(index: number) {
    const task = this.tasks[index]
    if (task) task.done = !task.done
  }

  pickFilter(index: number) {
    this.filter = index
    this.selection.resize(this.shown().length, VISIBLE)
  }

  view() {
    const shown = this.shown()
    const done = this.tasks.filter((task) => task.done).length
    const row = (position: number) => {
      const index = shown[position] ?? 0
      const task = this.tasks[index] ?? { text: '', done: false }
      return checkbox(task.text, task.done, () => this.toggle(index))
    }
    const body = shown.length ? listView(this.selection, row) : hint('Type a task and press Enter')
    const card = panel(
      title('Tasks'),
      tabs(FILTERS, this.filter, (index) => this.pickFilter(index)),
      textField(this.draft, 'Add a task'),
      body,
      progress(this.tasks.length ? done / this.tasks.length : 0),
      hint('Press Up and Down to pick a task, Tab to toggle it'),
    ).width(Theme.current().columns(60))
    return page(card).align('center')
  }

  input(event: Event) {
    if (this.selection.handle(event)) return Control.Redraw
    const shown = this.shown()
    const selected = shown[this.selection.selected]
    if (event.key === 'tab' && selected !== undefined) {
      this.toggle(selected)
      return Control.Redraw
    }
    const edit = this.draft.handle(event)
    if (edit === Edit.Submitted && this.draft.text.trim()) {
      this.tasks.push({ text: this.draft.take().trim(), done: false })
      this.selection.resize(this.shown().length, VISIBLE)
    }
    return edit === Edit.Ignored ? undefined : Control.Redraw
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  run(new Todo())
}
