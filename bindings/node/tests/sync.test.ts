// Rust is the source of truth for the API. Every public widget, game item,
// type method and app hook in the Rust crates has a JavaScript counterpart,
// or an entry in OMITTED saying why JavaScript does without it. Adding
// something public in Rust fails this test until the binding or the reason
// is written.

import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import * as katui from '@matheusfillipe/katui'
import { BINDINGS, REPO } from './paths.ts'

const CRATES = join(REPO, 'crates')

const RENAMED: Record<string, string> = {
  list: 'listView',
  markdown_view: 'markdown',
  switch: 'toggleSwitch',
  HeldKeys: 'Keys',
  v3: 'V3',
  new: 'constructor',
  with_text: 'constructor',
  leaf: 'constructor',
  branch: 'constructor',
  len: 'length',
  push_for: 'push',
  to_surface: 'toImage',
  set_text: 'text',
}

const OMITTED: Record<string, string> = {
  Orientation: "split takes the strings 'horizontal' and 'vertical'",
  Blend: "drawing calls take the strings 'over' and 'add'",
  P2: 'points are [x, y] arrays',
  Rgb: 'light colors are [r, g, b] arrays',
  Key: "keys are named by strings like 'a', 'enter' and 'up'",
  Scroll: 'a scroll position is a plain { offset, max } object',
  HtmlDefaults: 'html and markdown take fontSize, color, link and tint options',
  HtmlHost: 'html and markdown take onLink and images options',
  scope: 'the app runner and Screen set the theme around every view',
  tick: 'the game loop feeds the held keys',
  press: 'the game loop feeds the held keys',
  release: 'the game loop feeds the held keys',
  clear: 'the game loop feeds the held keys',
}

function camel(name: string): string {
  return name.replace(/_([a-z0-9])/g, (_, letter: string) => letter.toUpperCase())
}

function source(path: string): string {
  return readFileSync(join(CRATES, path), 'utf8')
}

/** The names a crate's lib.rs re-exports with `pub use`. */
function exports(lib: string): string[] {
  const names: string[] = []
  for (const match of source(lib).matchAll(/^pub use [\w:]+::(\{[^}]*\}|\w+);/gm)) {
    names.push(...(match[1]?.match(/\w+/g) ?? []))
  }
  return names
}

/** The `pub fn` names in the inherent impl blocks of `type`. */
function publicFns(path: string, type: string): string[] {
  const text = source(path)
  const names: string[] = []
  const start = new RegExp(`^impl(?:<[^{]*?>)?\\s+${type}(?:<[^>]*>)?\\s*\\{`, 'gm')
  for (const found of text.matchAll(start)) {
    let depth = 0
    let at = found.index + found[0].length - 1
    for (; at < text.length; at++) {
      depth += text[at] === '{' ? 1 : text[at] === '}' ? -1 : 0
      if (depth === 0) break
    }
    const body = text.slice(found.index + found[0].length, at)
    for (const method of body.matchAll(/^ {4}pub (?:const )?fn (\w+)/gm)) {
      if (method[1]) names.push(method[1])
    }
  }
  return names
}

function traitFns(path: string, trait: string): string[] {
  const block = source(path).match(new RegExp(`^pub trait ${trait}\\b[\\s\\S]*?^\\}`, 'm'))
  assert.ok(block, `trait ${trait} not found in ${path}`)
  return [...block[0].matchAll(/^ {4}fn (\w+)/gm)].flatMap((found) => (found[1] ? [found[1]] : []))
}

function missing(names: string[], target: object): string[] {
  return names.filter((name) => !(name in OMITTED) && !(camel(RENAMED[name] ?? name) in target))
}

for (const lib of [
  'katui-widgets/src/lib.rs',
  'katui-game/src/lib.rs',
  'katui-content/src/lib.rs',
]) {
  test(`every export of ${lib} has a JavaScript name`, () => {
    const names = exports(lib)
    assert.ok(names.length > 0, `no exports found in ${lib}; fix the parser or the path`)
    assert.deepEqual(missing(names, katui), [])
  })
}

const METHODS: [string, string, object][] = [
  ['katui-ui/src/element.rs', 'Element', katui.Element],
  ['katui-widgets/src/theme.rs', 'Theme', katui.Theme],
  ['katui-widgets/src/field.rs', 'TextField', katui.TextField],
  ['katui-widgets/src/area.rs', 'TextArea', katui.TextArea],
  ['katui-widgets/src/list.rs', 'ListState', katui.ListState],
  ['katui-widgets/src/scroll.rs', 'ScrollState', katui.ScrollState],
  ['katui-widgets/src/tree.rs', 'TreeNode', katui.TreeNode],
  ['katui-widgets/src/tree.rs', 'TreeState', katui.TreeState],
  ['katui-widgets/src/focus.rs', 'Focus', katui.Focus],
  ['katui-widgets/src/feedback.rs', 'Toasts', katui.Toasts],
  ['katui-game/src/light.rs', 'Light', katui.Light],
  ['katui-game/src/keys.rs', 'HeldKeys', katui.Keys],
  ['katui-game/src/space.rs', 'V3', katui.V3],
  ['katui-game/src/space.rs', 'Camera', katui.Camera],
  ['katui-game/src/space.rs', 'Rng', katui.Rng],
  ['katui-game/src/space.rs', 'Sparks', katui.Sparks],
]

/** A class's static and instance members, the ones `in` can see. */
function members(type: object): object {
  const prototype = (type as { prototype: object }).prototype
  return new Proxy(type, { has: (target, name) => name in target || name in prototype })
}

for (const [path, rust, type] of METHODS) {
  test(`every method of ${rust} has a JavaScript one`, () => {
    const names = publicFns(path, rust)
    assert.ok(names.length > 0, `no public methods of ${rust} found; fix the parser or the path`)
    assert.deepEqual(missing(names, members(type)), [])
  })
}

test('every theme field is a JavaScript property', () => {
  const struct = source('katui-widgets/src/theme.rs').match(/pub struct Theme \{([\s\S]*?)\n\}/)
  assert.ok(struct?.[1])
  const fields = [...struct[1].matchAll(/^ {4}pub (\w+):/gm)].flatMap((found) =>
    found[1] ? [found[1]] : [],
  )
  assert.deepEqual(missing(fields, katui.Theme.dark()), [])
})

for (const [path, trait, type] of [
  ['katui-app/src/lib.rs', 'UiApp', katui.App],
  ['katui-game/src/arcade.rs', 'Game', katui.Game],
] as const) {
  test(`every hook of ${trait} is a JavaScript method`, () => {
    assert.deepEqual(missing(traitFns(path, trait), type.prototype), [])
  })
}

for (const [path, name, values] of [
  ['katui-rt/src/app.rs', 'Control', katui.Control],
  ['katui-widgets/src/field.rs', 'Edit', katui.Edit],
] as const) {
  test(`every variant of ${name} has a JavaScript value`, () => {
    const block = source(path).match(new RegExp(`pub enum ${name} \\{([\\s\\S]*?)\\n\\}`))
    assert.ok(block?.[1])
    const variants = [...block[1].matchAll(/^ {4}(\w+),/gm)].flatMap((found) =>
      found[1] ? [found[1]] : [],
    )
    assert.deepEqual(
      variants.filter((variant) => !(variant in values)),
      [],
    )
  })
}

test('the package version is the workspace version', () => {
  const workspace = readFileSync(join(REPO, 'Cargo.toml'), 'utf8').match(/^version = "([^"]+)"/m)
  const manifest = JSON.parse(readFileSync(join(BINDINGS, 'package.json'), 'utf8'))
  assert.equal(katui.VERSION, workspace?.[1])
  assert.equal(manifest.version, workspace?.[1])
})
