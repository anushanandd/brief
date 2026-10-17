// @vitest-environment jsdom
import { readdirSync, readFileSync } from 'node:fs'

import { act, useMemo } from 'react'
import { createRoot } from 'react-dom/client'
import { expect, it, vi } from 'vitest'

import { ActionProvider, useQueueAction, useRegisterActions, type AppAction } from './action-system'
import { objectVerbs, verbLink, verbs } from './verbs'

const sources = (directory: string): string[] =>
  readdirSync(directory, { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory()
      ? sources(`${directory}/${entry.name}`)
      : /\.tsx?$/.test(entry.name) && !entry.name.includes('.test.')
        ? [readFileSync(`${directory}/${entry.name}`, 'utf8')]
        : [],
  )

it('names only verbs some page registers, each on a page or an object', () => {
  const code = sources('src').join('\n')
  const ids = new Set(verbs.map(({ id }) => id))
  expect(ids.size).toBe(verbs.length)
  for (const verb of verbs) {
    expect(code, verb.id).toContain(`id: '${verb.id}'`)
    expect(Boolean(verb.page) !== Boolean(verb.object), verb.id).toBe(true)
  }
  expect(objectVerbs('document').map(({ id }) => id)).toContain('documents.reread')
})

it('runs a link only for a catalog verb on its own kind of object', () => {
  expect(verbLink('/do/documents.reread?object=document:abc')).toMatchObject({
    verb: { id: 'documents.reread' },
    object: 'document:abc',
  })
  expect(verbLink('/do/calendar.today')).toMatchObject({ verb: { id: 'calendar.today' } })
  expect(verbLink('/do/documents.reread?object=account:abc')).toBeUndefined()
  expect(verbLink('/do/documents.reread')).toBeUndefined()
  expect(verbLink('/do/finance.erase')).toBeUndefined()
  expect(verbLink('/documents')).toBeUndefined()
})

it('runs a queued action once its page registers it enabled', async () => {
  const run = vi.fn()
  let queue: ((id: string) => void) | undefined
  function Queue() {
    queue = useQueueAction()
    return null
  }
  function Page({ disabled }: { disabled: boolean }) {
    useRegisterActions(
      useMemo<AppAction[]>(
        () => [{ id: 'documents.reread', title: 'Read again', scope: 'page', disabled, run }],
        [disabled],
      ),
    )
    return null
  }
  const container = document.createElement('div')
  const root = createRoot(container)
  await act(async () =>
    root.render(
      <ActionProvider>
        <Queue />
      </ActionProvider>,
    ),
  )
  await act(async () => queue?.('documents.reread'))
  expect(run).not.toHaveBeenCalled()
  // The page opens, then its data loads and enables the action.
  await act(async () =>
    root.render(
      <ActionProvider>
        <Queue />
        <Page disabled />
      </ActionProvider>,
    ),
  )
  expect(run).not.toHaveBeenCalled()
  await act(async () =>
    root.render(
      <ActionProvider>
        <Queue />
        <Page disabled={false} />
      </ActionProvider>,
    ),
  )
  expect(run).toHaveBeenCalledTimes(1)
  await act(async () =>
    root.render(
      <ActionProvider>
        <Queue />
        <Page disabled />
      </ActionProvider>,
    ),
  )
  await act(async () =>
    root.render(
      <ActionProvider>
        <Queue />
        <Page disabled={false} />
      </ActionProvider>,
    ),
  )
  expect(run).toHaveBeenCalledTimes(1)
  await act(async () => root.unmount())
})
