// Rust is the source of truth for the API. Every public widget, game item,
// type method, app hook and variant listed in bindings/rust-api.json has a
// JavaScript counterpart, or an entry in OMITTED saying why JavaScript does
// without it.

import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import * as katui from '@mattfillipe/katui'
import { BINDINGS } from './paths.ts'

interface RustApi {
  version: string
  exports: Record<string, string[]>
  methods: Record<string, string[]>
  fields: Record<string, string[]>
  hooks: Record<string, string[]>
  variants: Record<string, string[]>
}

const API: RustApi = JSON.parse(readFileSync(join(BINDINGS, '..', 'rust-api.json'), 'utf8'))

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

function missing(names: string[], target: object): string[] {
  return names.filter((name) => !(name in OMITTED) && !(camel(RENAMED[name] ?? name) in target))
}

for (const [crate, names] of Object.entries(API.exports)) {
  test(`every export of ${crate} has a JavaScript name`, () => {
    assert.deepEqual(missing(names, katui), [])
  })
}

const CLASSES: Record<string, object> = {
  Element: katui.Element,
  Theme: katui.Theme,
  TextField: katui.TextField,
  TextArea: katui.TextArea,
  ListState: katui.ListState,
  ScrollState: katui.ScrollState,
  TreeNode: katui.TreeNode,
  TreeState: katui.TreeState,
  Focus: katui.Focus,
  Toasts: katui.Toasts,
  Light: katui.Light,
  HeldKeys: katui.Keys,
  V3: katui.V3,
  Camera: katui.Camera,
  Rng: katui.Rng,
  Sparks: katui.Sparks,
}

/** A class's static and instance members, the ones `in` can see. */
function members(type: object): object {
  const prototype = (type as { prototype: object }).prototype
  return new Proxy(type, { has: (target, name) => name in target || name in prototype })
}

for (const [rust, names] of Object.entries(API.methods)) {
  test(`every method of ${rust} has a JavaScript one`, () => {
    const type = CLASSES[rust]
    assert.ok(type, `map ${rust} to its JavaScript class in CLASSES`)
    assert.deepEqual(missing(names, members(type)), [])
  })
}

test('every theme field is a JavaScript property', () => {
  assert.deepEqual(missing(API.fields.Theme ?? [], katui.Theme.dark()), [])
})

const HOOKS: Record<string, { prototype: object }> = { UiApp: katui.App, Game: katui.Game }

for (const [trait, hooks] of Object.entries(API.hooks)) {
  test(`every hook of ${trait} is a JavaScript method`, () => {
    const type = HOOKS[trait]
    assert.ok(type, `map ${trait} to its JavaScript class in HOOKS`)
    assert.deepEqual(missing(hooks, type.prototype), [])
  })
}

const VARIANTS: Record<string, object> = { Control: katui.Control, Edit: katui.Edit }

for (const [name, variants] of Object.entries(API.variants)) {
  test(`every variant of ${name} has a JavaScript value`, () => {
    const values = VARIANTS[name] ?? {}
    assert.deepEqual(
      variants.filter((variant) => !(variant in values)),
      [],
    )
  })
}

test('the package version is the workspace version', () => {
  const manifest = JSON.parse(readFileSync(join(BINDINGS, 'package.json'), 'utf8'))
  assert.equal(katui.VERSION, API.version)
  assert.equal(manifest.version, API.version)
})
