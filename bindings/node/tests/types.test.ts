// Messages are checked at compile time. Each `@ts-expect-error` line must
// fail to type-check, or tsc reports the directive as unused, so a weaker
// type here breaks `just node-check`.

import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import {
  App,
  button,
  type Element,
  Focus,
  focusableButton,
  html,
  label,
  type MessageOf,
  markdown,
  panel,
  row,
  Screen,
  ScrollState,
  scrollView,
  slider,
  TextField,
  tabs,
  textField,
  type Waker,
} from '@mattfillipe/katui'
import { BINDINGS } from './paths.ts'

type Equal<A, B> =
  (<T>() => T extends A ? 1 : 2) extends <T>() => T extends B ? 1 : 2 ? true : false
function same<A, B>(_proof: Equal<A, B>): void {}

type Message = 'save' | 'quit'

class Editor extends App<Message> {
  view(): Element<Message> {
    return panel(label('Notes'), button('Save', 'save'), button('Quit', 'quit'))
  }

  update(message: Message): undefined {
    same<typeof message, Message>(true)
    return undefined
  }
}

test('a widget carries the literal message it sends', () => {
  same<typeof button, typeof button>(true)
  const save = button('Save', 'save')
  same<typeof save, Element<'save'>>(true)
  const leave = focusableButton('Leave', 'leave', true)
  same<typeof leave, Element<'leave'>>(true)
  // @ts-expect-error a focusable button sends only its own message
  const stray: Element<'save'> = focusableButton('Leave', 'leave', true)
  assert.ok(stray)
  same<ReturnType<typeof label>, Element<never>>(true)
})

test('a container sends what its children send', () => {
  const mixed = panel(label('a'), button('b', 'save'), [button('c', 'quit'), null, false])
  same<typeof mixed, Element<'save' | 'quit'>>(true)
  same<MessageOf<[Element<1>, Element<2>[]]>, 1 | 2>(true)
  const scrolled = scrollView(new ScrollState(), [button('x', 'save')], (scroll) => scroll.offset)
  same<typeof scrolled, Element<'save' | number>>(true)
})

test('an app only shows elements that send its own messages', () => {
  class Wrong extends App<Message> {
    view(): Element<Message> {
      // @ts-expect-error 'delete' is not a Message
      return panel(button('Delete', 'delete'))
    }
  }
  assert.ok(Wrong)
  assert.ok(new Screen(new Editor()))
})

test('handlers widen the element and callbacks see their argument types', () => {
  const clicked = label('x').onClick('save').onHover(7)
  same<typeof clicked, Element<'save' | 7>>(true)
  // @ts-expect-error the slider hands over a number, not a string
  slider(0.5, (value: string) => value)
  // @ts-expect-error the tabs hand over an index
  tabs(['a'], 0, (index: boolean) => index)
  const picked = tabs(['a', 'b'], 0, (index) => ({ tab: index }))
  same<typeof picked, Element<{ tab: number }>>(true)
})

test('options keep their literal types, and the engine rejects them at runtime too', () => {
  // @ts-expect-error alignment is start, center or end
  assert.throws(() => label('x').align('centre'))
  // @ts-expect-error a length is pixels, fill or content
  assert.throws(() => label('x').width('full'))
  // @ts-expect-error a color is a string or channels
  assert.throws(() => label('x').background(255))
  // @ts-expect-error a text field takes a TextField
  assert.throws(() => textField('text'))
  assert.ok(textField(new TextField()))
})

test('documents send hrefs unless onLink makes another message', () => {
  same<typeof html, typeof html>(true)
  const page = markdown('[a](b)')
  same<typeof page, Element<string>>(true)
  const routed = html('<a href="x">x</a>', { onLink: (href) => ({ open: href }) })
  same<typeof routed, Element<{ open: string }>>(true)
})

test("a waker and focus only take the app's own values", () => {
  const send = (waker: Waker<Message>) => {
    waker.send('save')
    // @ts-expect-error 'stop' is not a Message
    waker.send('stop')
  }
  assert.ok(send)
  const focus = new Focus(['note', 'table'] as const)
  same<ReturnType<typeof focus.current>, 'note' | 'table'>(true)
  // @ts-expect-error 'list' is not one of the controls
  focus.is('list')
})

test('rows and columns union their children like containers', () => {
  const bar = row(button('a', 1), button('b', 'two'))
  same<typeof bar, Element<1 | 'two'>>(true)
})

/**
 * Every generated declaration that touches a message, through `unknown` or a
 * type that carries messages, has a typed one in katui.d.ts.
 */
test('no generated api reaches users with untyped messages', () => {
  const generated = readFileSync(join(BINDINGS, 'index.d.ts'), 'utf8')
  const typed = readFileSync(join(BINDINGS, 'katui.d.ts'), 'utf8')
  const code = generated.replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '')
  const declarations = code.split(/\n(?=export )/)
  const untyped = declarations
    .filter((block) =>
      /\b(unknown|AppLike|Child|Element|Waker|DocumentOptions|Focus)\b/.test(block),
    )
    .map(
      (block) => block.match(/^export (?:declare )?(?:function|class|interface|type) (\w+)/)?.[1],
    )
    .filter((name): name is string => name !== undefined)
  const missing = untyped.filter(
    (name) =>
      !new RegExp(
        `^export (?:declare )?(?:function|class|abstract class|interface|type|const) ${name}\\b`,
        'm',
      ).test(typed),
  )
  assert.deepEqual(missing, [])
})
