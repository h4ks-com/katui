// The widget tour: a menu, a file tree beside a page of controls, a table,
// a note, toasts, a dialog, and themes and sizes that switch at runtime.

import { fileURLToPath } from 'node:url'
import {
  type Action,
  App,
  button,
  Column,
  Control,
  column,
  dialog,
  Edit,
  type Event,
  Focus,
  heading,
  hint,
  keyHints,
  ListState,
  label,
  menu,
  panel,
  primaryButton,
  radio,
  row,
  run,
  type Scroll,
  ScrollState,
  scrollView,
  select,
  slider,
  spacer,
  spinner,
  split,
  TextArea,
  Theme,
  Toasts,
  TreeNode,
  TreeState,
  table,
  textArea,
  toasts,
  toggleSwitch,
  tooltip,
  tree,
  type Waker,
} from '@mattfillipe/katui'

const THEMES = ['Dusk', 'Forest', 'Paper']
const SIZES = ['Small', 'Medium', 'Large']
const SCALES = [0.85, 1.0, 1.2]
const FILE_MENU = ["Count the note's lines", 'Clear the note', 'Quit']
const TOOLS = [
  ['ripgrep', 'search', '41k'],
  ['fzf', 'finder', '62k'],
  ['bat', 'pager', '47k'],
]
const FILES = [
  new TreeNode(1, 'src', [new TreeNode(2, 'main.ts'), new TreeNode(3, 'widgets.ts')]),
  new TreeNode(4, 'README.md'),
]

function forest(cell: [number, number]): Theme {
  const theme = Theme.dark(cell)
  theme.background = [18, 28, 22]
  theme.raised = [28, 42, 33]
  theme.highlight = [42, 64, 50]
  theme.border = [58, 84, 66]
  theme.accent = [120, 214, 150]
  return theme
}

const PALETTES = [
  (cell: [number, number]) => Theme.dark(cell),
  forest,
  (cell: [number, number]) => Theme.light(cell),
]

export class Tour extends App<Action> {
  popup: string | null = null
  palette = 0
  size = 1
  busy = false
  volume = 0.4
  tip = false
  sidebar = 0.25
  table = new ListState(TOOLS.length, 4)
  note = new TextArea(4)
  page = new ScrollState()
  files = new TreeState()
  toasts = new Toasts()
  focus = new Focus(['note', 'table'])
  started = performance.now()

  constructor() {
    super()
    this.files.toggle(1)
  }

  theme(cell: [number, number]) {
    const make = PALETTES[this.palette] ?? forest
    return make(cell).scaled(SCALES[this.size] ?? 1)
  }

  start(waker: Waker) {
    waker.every(0.25, () => this.tick())
  }

  tick() {
    const expired = this.toasts.expire()
    return this.busy || expired ? Control.Redraw : Control.Wait
  }

  toggle(popup: string) {
    this.popup = this.popup === popup ? null : popup
  }

  file(index: number) {
    this.popup = index === 2 ? 'quit' : null
    if (index === 0) {
      this.toasts.push(`The note has ${this.note.text.split('\n').length} lines`)
    } else if (index === 1) {
      this.note.take()
    }
  }

  pickTheme(index: number) {
    this.palette = index
    this.popup = null
  }

  elapsed() {
    return (performance.now() - this.started) / 1000
  }

  controls() {
    const theme = Theme.current()
    const working = this.busy
      ? row(spinner(this.elapsed()), label('Turn off to stop'))
          .gap(theme.cellWidth())
          .align('center')
      : label('Turn on the switch to start working').color(theme.muted)
    return panel(
      heading('Settings'),
      hint('Theme'),
      select(
        THEMES,
        this.palette,
        this.popup === 'themes',
        () => this.toggle('themes'),
        (index) => this.pickTheme(index),
      ),
      hint('Size'),
      radio(SIZES, this.size, (index) => {
        this.size = index
      }),
      toggleSwitch('Work in the background', this.busy, () => {
        this.busy = !this.busy
      }),
      working,
      hint(`Volume ${Math.round(this.volume * 100)}%`),
      slider(this.volume, (value) => {
        this.volume = value
      }),
      tooltip(
        primaryButton('Count lines', () => this.file(0)),
        'Count the lines in the note',
        this.tip,
        () => {
          this.tip = true
        },
        () => {
          this.tip = false
        },
      ),
    ).width('fill')
  }

  data() {
    return panel(
      heading('Tools'),
      table(
        [new Column('Name', 12), new Column('Kind', 10), new Column('Stars')],
        TOOLS,
        this.table,
        (index) => this.table.select(index),
      ),
      hint('Note'),
      textArea(this.note, 'Write a note', this.focus.is('note')),
    ).width('fill')
  }

  view() {
    const theme = Theme.current()
    const bar = row(
      menu(
        'File',
        FILE_MENU,
        this.popup === 'file',
        () => this.toggle('file'),
        (index) => this.file(index),
      ),
      spacer(),
      keyHints([
        ['Tab', 'switch field'],
        ['Esc', 'close the popup'],
        ['Ctrl+C', 'quit'],
      ]),
    )
      .align('center')
      .paddingXy(theme.cellWidth(), theme.cellHeight() * 0.3)
      .width('fill')
      .background(theme.raised ?? theme.text)
    const sidebar = column(
      tree(
        FILES,
        this.files,
        (id) => this.files.toggle(id),
        (id) => this.files.select(id),
      ),
    )
      .padding(theme.cellHeight() * 0.5)
      .size('fill', 'fill')
      .background(theme.raised ?? theme.text)
    const content = scrollView(this.page, [this.controls(), this.data()], (scroll: Scroll) =>
      this.page.apply(scroll),
    ).padding(theme.cellHeight())
    const body = split(sidebar, content, this.sidebar, (at) => {
      this.sidebar = at
    })
    const quit =
      this.popup === 'quit' &&
      dialog(
        'Quit the tour?',
        label('Quit now to discard the note.').color(theme.muted),
        [button('Stay', () => this.dismiss()), primaryButton('Quit', () => Control.Quit)],
        () => this.dismiss(),
      )
    return column(bar, body, toasts(this.toasts), quit)
      .size('fill', 'fill')
      .background(theme.background ?? theme.text)
  }

  dismiss() {
    this.popup = null
  }

  input(event: Event) {
    if (event.key === 'escape') {
      this.popup = null
      return Control.Redraw
    }
    if (this.focus.handle(event)) return Control.Redraw
    const handled = this.focus.is('note')
      ? this.note.handle(event) !== Edit.Ignored
      : this.table.handle(event)
    return handled || this.page.handle(event, 40) ? Control.Redraw : undefined
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  run(new Tour())
}
