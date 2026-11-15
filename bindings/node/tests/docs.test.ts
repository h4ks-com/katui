// The code in the docs keeps working: every block parses, and a block that
// runs an app or a game type-checks and runs headless here.

import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { stripTypeScriptTypes } from 'node:module'
import { basename, join } from 'node:path'
import { test } from 'node:test'
import { pathToFileURL } from 'node:url'
import { BINDINGS, REPO } from './paths.ts'

const DOCS = [join(REPO, 'docs', 'node.md'), join(BINDINGS, 'README.md')]
const OUT = join(REPO, 'target', 'node-docs')
const RUNS = /\b(run|play)\(/

function blocks(path: string): string[] {
  const found = readFileSync(path, 'utf8').matchAll(/^```(?:ts|js)\n([\s\S]*?)^```/gm)
  return [...found].flatMap((block) => (block[1] ? [block[1]] : []))
}

const CASES = DOCS.flatMap((path) =>
  blocks(path).map((code, index) => ({ name: `${basename(path, '.md')}-${index}`, code })),
)
const RUNNABLE = CASES.filter(({ code }) => RUNS.test(code))

test('the docs have code blocks that run', () => {
  for (const path of DOCS) assert.ok(blocks(path).length > 0, `no code blocks in ${path}`)
  assert.ok(RUNNABLE.length > 0)
})

for (const { name, code } of CASES) {
  test(`${name} parses`, () => {
    stripTypeScriptTypes(code)
  })
}

test('the blocks that run type-check', () => {
  rmSync(OUT, { recursive: true, force: true })
  mkdirSync(OUT, { recursive: true })
  for (const { name, code } of RUNNABLE) writeFileSync(join(OUT, `${name}.ts`), code)
  const config = {
    extends: join(BINDINGS, 'tsconfig.json'),
    compilerOptions: {
      paths: { '@matheusfillipe/katui': [join(BINDINGS, 'katui.d.ts')] },
      typeRoots: [join(BINDINGS, 'node_modules', '@types')],
      noUnusedLocals: false,
    },
    include: ['*.ts'],
  }
  writeFileSync(join(OUT, 'tsconfig.json'), JSON.stringify(config))
  writeFileSync(join(OUT, 'package.json'), JSON.stringify({ type: 'module' }))
  execFileSync(join(BINDINGS, 'node_modules', '.bin', 'tsc'), ['-p', OUT], { stdio: 'inherit' })
})

for (const { name, code } of RUNNABLE) {
  test(`${name} runs headless`, async () => {
    const headless = pathToFileURL(join(BINDINGS, 'tests', 'headless.ts')).href
    const path = join(OUT, `${name}.run.ts`)
    mkdirSync(OUT, { recursive: true })
    writeFileSync(path, code.replaceAll("from '@matheusfillipe/katui'", `from '${headless}'`))
    await import(pathToFileURL(path).href)
  })
}
