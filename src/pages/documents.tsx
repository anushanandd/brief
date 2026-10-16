import { useQueryClient } from '@tanstack/react-query'
import { Link, useNavigate, useSearch } from '@tanstack/react-router'
import { open as choose } from '@tauri-apps/plugin-dialog'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { toast } from 'sonner'

import { AutofillField, acceptAutofill, autofillCompletion } from '../components/autofill'
import { PageError, PageLoading } from '../components/data-state'
import { FilterSelect } from '../components/filter-select'
import {
  Banknote,
  Check,
  Eye,
  FileText,
  FolderPlus,
  Invoice,
  Landmark,
  Plus,
  RotateCcw,
  ScrollText,
  Search,
  Sparkles,
  Trash2,
  X,
} from '../components/icons'
import { SelectorStrip } from '../components/selector-strip'
import { SettingsLink } from '../components/settings-dialog'
import { Button, Card, EmptyState, SectionHeading } from '../components/ui'
import { WorkspaceHeader } from '../components/workspace-header'
import { financeQueryKey, useFinance } from '../hooks/use-finance'
import { useFinanceEdits } from '../hooks/use-finance-edits'
import { useGraphAccountShortcuts } from '../hooks/use-graph-window-shortcuts'
import { useSearchShortcuts } from '../hooks/use-search-shortcuts'
import { accountDisplayName } from '../lib/account-names'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import {
  addDocumentHistory,
  deleteDocuments,
  documentFile,
  importDocuments,
  isTauri,
  openDocument,
  rereadDocument,
  updateDocument,
  type DocumentChange,
  type DocumentImport,
} from '../lib/api'
import { documentKindNames as kindNames, documentTitle as title } from '../lib/documents'
import { errorMessage } from '../lib/errors'
import { financeSettings } from '../lib/finance-settings'
import { countOf, formatCurrency, formatDay, formatShortDay } from '../lib/format'
import { documentKinds } from '../lib/schema'
import type { BriefDocument, DocumentField, DocumentInsights, FinanceSnapshot } from '../lib/schema'

type YearInsight = DocumentInsights['years'][number]

/** The list's groups, in the order a tax year reads them. */
const groups: { kinds: BriefDocument['kind'][]; title: string }[] = [
  { kinds: ['taxForm', 'taxReturn'], title: 'Tax forms' },
  { kinds: ['payslip'], title: 'Pay statements' },
  { kinds: ['statement', 'export'], title: 'Statements' },
  { kinds: ['other'], title: 'Other' },
]

/** Each kind's color and mark, so a kind reads the same in the list, the panel, and the year. */
const kindStyle: Record<BriefDocument['kind'], { color: string; icon: typeof FileText }> = {
  taxForm: { color: 'var(--data-1)', icon: Invoice },
  taxReturn: { color: 'var(--data-1)', icon: Invoice },
  payslip: { color: 'var(--data-2)', icon: Banknote },
  statement: { color: 'var(--data-3)', icon: Landmark },
  export: { color: 'var(--data-3)', icon: ScrollText },
  other: { color: 'var(--data-5)', icon: FileText },
}
const monthInitials = ['J', 'F', 'M', 'A', 'M', 'J', 'J', 'A', 'S', 'O', 'N', 'D']
const monthNames = Array.from({ length: 12 }, (_, month) =>
  new Date(Date.UTC(2000, month, 1)).toLocaleString('en-US', { month: 'long', timeZone: 'UTC' }),
)

/** A group's documents, newest first, as the list shows them. */
const grouped = (documents: BriefDocument[], kinds: BriefDocument['kind'][]) =>
  documents
    .filter((document) => kinds.includes(document.kind))
    .toSorted((left, right) => (right.date ?? '').localeCompare(left.date ?? ''))

/** A tile in the kind's color: a tax form shows its number (W-2, 1099), anything else its icon. */
function DocumentMark({ document }: { document: BriefDocument }) {
  const { color, icon: Icon } = kindStyle[document.kind]
  const form =
    (document.kind === 'taxForm' || document.kind === 'taxReturn') && document.form
      ? document.form.length <= 4
        ? document.form
        : document.form.split('-')[0]
      : undefined
  return (
    <span className="documents-mark" style={{ '--kind-color': color }} aria-hidden="true">
      {form && form.length <= 4 ? form : <Icon size={15} />}
    </span>
  )
}

/** The tax years the strip offers: this year, last year, and any a document names. */
function taxYears(data: FinanceSnapshot | undefined) {
  const today = data?.insights?.today ?? new Date().toISOString().slice(0, 10)
  const thisYear = Number(today.slice(0, 4))
  const years = [
    ...new Set([
      thisYear,
      thisYear - 1,
      ...(data?.documents ?? [])
        .map((document) => document.taxYear)
        .filter((year): year is number => !!year),
    ]),
  ].toSorted((left, right) => right - left)
  return { today, thisYear, years }
}

/** The one figure a row leads with: wages, net pay, or the ending balance. */
function headline(document: BriefDocument) {
  const amount = (key: string) => document.fields.find((field) => field.key === key)?.amount
  switch (document.kind) {
    case 'payslip':
      return amount('net')
    case 'taxForm':
      return (
        amount('box1') ??
        amount('box10') ??
        document.fields.find((field) => field.amount != null)?.amount
      )
    case 'statement':
      return amount('endingBalance')
    default:
      return undefined
  }
}

const toCheck = (document: BriefDocument) =>
  document.reviewed
    ? 0
    : document.fields.filter((field) => field.uncertain && !field.edited).length +
      document.checks.filter((check) => !check.passed).length

/** Where a document stands, and the tone that says it at a glance. */
function standing(document: BriefDocument): {
  label: string
  tone: 'done' | 'attention' | 'quiet'
} {
  if (document.unreadable) return { label: document.unreadable, tone: 'attention' }
  const open = toCheck(document)
  if (document.reviewed) return { label: 'Checked', tone: 'done' }
  if (open) return { label: `${open} to check`, tone: 'attention' }
  if (document.statementFileId) return { label: 'In history', tone: 'done' }
  if (document.statementRows) return { label: `${document.statementRows} rows`, tone: 'quiet' }
  if (document.fields.length) return { label: 'Read', tone: 'quiet' }
  return { label: document.fileName, tone: 'quiet' }
}

/** Everything a document says that a search can find: its names, dates, summary, and figures. */
function searchText(document: BriefDocument, accountName: (id: string) => string) {
  return [
    title(document),
    document.fileName,
    kindNames[document.kind],
    document.date,
    document.taxYear,
    document.summary,
    document.accountId ? accountName(document.accountId) : '',
    ...document.fields.flatMap((field) => [
      field.label,
      field.text,
      field.amount?.toFixed(2),
      field.ytd?.toFixed(2),
    ]),
  ]
    .filter((part) => part != null && part !== '')
    .join(' ')
    .toLowerCase()
}

/** Each word of the query, with amounts typed as `$2,937.35` matching `2937.35`. */
function searchTerms(query: string) {
  return query
    .toLowerCase()
    .split(/\s+/)
    .map((term) => (/\d/.test(term) ? term.replace(/[$,]/g, '') : term))
    .filter(Boolean)
}

function useCommit() {
  const client = useQueryClient()
  return (snapshot: FinanceSnapshot) => client.setQueryData(financeQueryKey, snapshot)
}

/** Reads files and folders, from a picker or dropped on the window, reporting progress. */
function useImport() {
  const commit = useCommit()
  const [progress, setProgress] = useState<{ done: number; total: number }>()
  const [result, setResult] = useState<DocumentImport>()
  useEffect(() => {
    if (!isTauri()) return undefined
    let stop: (() => void) | undefined
    void import('@tauri-apps/api/event').then(({ listen }) =>
      listen<{ done: number; total: number }>('documents-progress', ({ payload }) =>
        setProgress(payload.done < payload.total ? payload : undefined),
      ).then((unlisten) => {
        stop = unlisten
      }),
    )
    return () => stop?.()
  }, [])
  const run = async (paths: string[]) => {
    if (!paths.length) return
    setProgress({ done: 0, total: paths.length })
    try {
      const imported = await importDocuments(paths)
      commit(imported.snapshot)
      setResult(imported)
      const parts = [
        `${imported.added.length} added`,
        imported.already.length ? `${imported.already.length} already in Brief` : '',
        imported.failed.length ? `${imported.failed.length} couldn’t be kept` : '',
      ].filter(Boolean)
      toast.success(parts.join(' · '))
    } catch (error) {
      toast.error('Could not add documents', { description: errorMessage(error) })
    } finally {
      setProgress(undefined)
    }
  }
  return { run, progress, result, dismiss: () => setResult(undefined) }
}

/** Reads again, one at a time, the documents a newer reader reads better. */
function useRereadAll() {
  const commit = useCommit()
  const [left, setLeft] = useState<number>()
  const run = async (ids: string[]) => {
    for (const [index, id] of ids.entries()) {
      setLeft(ids.length - index)
      try {
        commit(await rereadDocument(id))
      } catch (error) {
        toast.error('Could not read a document again', { description: errorMessage(error) })
        break
      }
    }
    setLeft(undefined)
  }
  return { run, left }
}

export function DocumentsPage() {
  const query = useFinance()
  const navigate = useNavigate()
  const search = useSearch({ from: '/documents' })
  const importer = useImport()
  const reread = useRereadAll()
  const [dragging, setDragging] = useState(false)
  const [find, setFind] = useState('')
  const findRef = useSearchShortcuts(find, setFind)
  const [viewing, setViewing] = useState(false)
  const closeViewer = useCallback(() => setViewing(false), [])
  const open = search.document

  useEffect(() => {
    if (!isTauri()) return undefined
    let stop: (() => void) | undefined
    void import('@tauri-apps/api/webview').then(({ getCurrentWebview }) =>
      getCurrentWebview()
        .onDragDropEvent(({ payload }) => {
          setDragging(payload.type === 'enter' || payload.type === 'over')
          if (payload.type === 'drop') void importer.run(payload.paths)
        })
        .then((unlisten) => {
          stop = unlisten
        }),
    )
    return () => stop?.()
    // The importer's run is stable enough for the listener's lifetime.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const addFiles = async (directory: boolean) => {
    const picked = await choose({
      multiple: true,
      directory,
      filters: directory
        ? undefined
        : [{ name: 'Documents', extensions: ['pdf', 'jpg', 'jpeg', 'png', 'heic', 'csv', 'xlsx'] }],
    })
    const paths = Array.isArray(picked) ? picked : picked ? [picked] : []
    await importer.run(paths)
  }

  useRegisterActions(
    useMemo<AppAction[]>(
      () => [
        {
          id: 'documents.add-files',
          title: 'Add documents…',
          search: 'import upload pay statement w-2 1099 pdf photo statement',
          group: 'Actions',
          scope: 'page',
          icon: Plus,
          bindings: [{ sequence: ['Meta+o'], label: '⌘O' }],
          run: () => void addFiles(false),
        },
        {
          id: 'documents.add-folder',
          title: 'Add a folder of documents…',
          search: 'import folder documents',
          group: 'Actions',
          scope: 'page',
          icon: FolderPlus,
          // Shift arrives in the key itself, so ⇧⌘O is Meta+O.
          bindings: [{ sequence: ['Meta+O'], label: '⇧⌘O' }],
          run: () => void addFiles(true),
        },
        {
          id: 'documents.close',
          title: 'Close document',
          group: 'View',
          scope: 'page',
          bindings: [{ sequence: ['Escape'], label: 'Esc' }],
          run: () => void navigate({ to: '/documents', search: { year: search.year } }),
        },
        ...(open
          ? [
              {
                id: 'documents.view',
                title: 'View the original',
                search: 'quick look preview show file pdf photo',
                group: 'View',
                scope: 'page' as const,
                icon: Eye,
                bindings: [{ sequence: ['Space'], label: 'Space' }],
                run: () => setViewing((current) => !current),
              },
            ]
          : []),
        // These act on what the page shows: statements ready to add, the year's income, and
        // documents a newer reader reads better.
        ...(
          [
            ['documents.add-history', 'Add statements to history', 'statement history import'],
            ['documents.use-income', 'Use document income in tax estimate', 'tax profile income'],
            ['documents.read-all', 'Read documents again', 'reread improved reader'],
          ] as const
        ).map(([id, name, terms]) => ({
          id,
          title: name,
          search: terms,
          group: 'Actions',
          scope: 'page' as const,
          run: () =>
            globalThis.document.querySelector<HTMLElement>(`[data-command="${id}"]`)?.click(),
        })),
      ],
      // eslint-disable-next-line react-hooks/exhaustive-deps
      [search.year, open],
    ),
  )
  // ⌘← and ⌘→ step through the tax years, newest first, as the strip lists them.
  useGraphAccountShortcuts(
    useCallback(
      (direction: -1 | 1) => {
        const { thisYear, years } = taxYears(query.data)
        const current = years.indexOf(search.year ?? thisYear)
        const next = years[Math.max(0, current) + direction]
        if (next) void navigate({ to: '/documents', search: { year: next } })
      },
      [navigate, query.data, search.year],
    ),
  )

  // A different document closes the one being viewed.
  useEffect(() => setViewing(false), [open])

  if (query.isLoading) return <PageLoading />
  if (query.isError || !query.data) return <PageError />
  const data = query.data
  const documents = data.documents ?? []
  const insights = data.insights?.documents
  const { today, thisYear, years } = taxYears(data)
  const year = search.year && years.includes(search.year) ? search.year : thisYear
  const selected = documents.find((document) => document.id === search.document)
  const names = financeSettings(data).accountNames
  const accountName = (id: string) =>
    accountDisplayName(
      id,
      data.accounts.find((account) => account.id === id)?.name ?? 'Account',
      names,
    )
  // A search looks through every year; otherwise the list is the year's.
  const terms = searchTerms(find)
  const inYear = documents.filter((document) => (document.taxYear ?? thisYear) === year)
  const shown = terms.length
    ? documents.filter((document) => {
        const text = searchText(document, accountName)
        return terms.every((term) => text.includes(term))
      })
    : inYear
  const completion = autofillCompletion(find, [
    ...new Set(documents.flatMap((document) => [title(document), document.issuer ?? ''])),
  ])
  const yearInsight = insights?.years.find((item) => item.year === year)
  const outdated = insights?.outdated ?? []
  const busy = !!importer.progress || reread.left != null
  const unchecked = shown.reduce((total, document) => total + toCheck(document), 0)

  return (
    <div className={`page documents-page${dragging ? ' is-dropping' : ''}`}>
      <WorkspaceHeader
        title="Documents"
        status={
          importer.progress ? (
            <span className="documents-progress" role="status">
              Reading {importer.progress.done + 1} of {importer.progress.total}…
            </span>
          ) : reread.left != null ? (
            <span className="documents-progress" role="status">
              Reading again · {reread.left} left
            </span>
          ) : undefined
        }
        actions={
          <>
            <Button
              icon={Plus}
              size="compact"
              disabled={!isTauri() || busy}
              data-command="documents.add-files"
              onClick={() => void addFiles(false)}
            >
              Add files…
            </Button>
            <Button
              icon={FolderPlus}
              size="compact"
              disabled={!isTauri() || busy}
              aria-keyshortcuts="Meta+Shift+O"
              data-command="documents.add-folder"
              onClick={() => void addFiles(true)}
            >
              Add folder…
            </Button>
          </>
        }
      />
      <SelectorStrip
        label="Tax years"
        items={years.map((item) => {
          const income = insights?.years.find((entry) => entry.year === item)?.income
          const count = documents.filter(
            (document) => (document.taxYear ?? thisYear) === item,
          ).length
          return {
            id: String(item),
            label: String(item),
            // The year's mark counts its documents.
            mark: (
              <span className="documents-year-mark" aria-label={countOf(count, 'document')}>
                {count}
              </span>
            ),
            link: { to: '/documents', search: { year: item } },
            current: item === year,
            value: income ? income.wages + income.selfEmployment : null,
          }
        })}
      />
      <div className="documents-toolbar">
        <label className="documents-search">
          <Search size={15} aria-hidden="true" />
          <AutofillField query={find} completion={completion}>
            <input
              ref={findRef}
              type="text"
              aria-label="Search documents"
              placeholder="Search all years"
              autoComplete="off"
              autoCorrect="off"
              autoCapitalize="off"
              spellCheck={false}
              data-command="search.focus"
              value={find}
              onChange={(event) => setFind(event.target.value)}
              onKeyDown={(event) => acceptAutofill(event, completion, setFind)}
            />
          </AutofillField>
          {find ? (
            <Button
              size="icon-compact"
              variant="ghost"
              aria-label="Clear search"
              data-command="search.clear"
              onClick={() => setFind('')}
            >
              <X size={12} aria-hidden="true" />
            </Button>
          ) : (
            <kbd aria-hidden="true">/</kbd>
          )}
        </label>
        <p className="documents-count" role="status">
          {terms.length
            ? `${shown.length} ${shown.length === 1 ? 'match' : 'matches'}`
            : `${shown.length} in ${year}`}
          {unchecked ? <span className="is-attention">{`${unchecked} to check`}</span> : null}
        </p>
        {outdated.length ? (
          <Button
            icon={RotateCcw}
            size="compact"
            disabled={busy}
            title="The reader improved since these were read"
            data-command="documents.read-all"
            onClick={() => void reread.run(outdated)}
          >
            Reread {outdated.length}
          </Button>
        ) : null}
      </div>
      {importer.result?.failed.length ? (
        <Card aria-label="Files Brief couldn’t keep" className="documents-section">
          <SectionHeading
            title={`Not added · ${importer.result.failed.length}`}
            action={
              <Button size="compact" data-command="documents.close" onClick={importer.dismiss}>
                Dismiss
              </Button>
            }
          />
          <ul className="documents-rows">
            {importer.result.failed.map((failure) => (
              <li className="documents-row" key={failure.name}>
                <span className="documents-row-mark" aria-hidden="true">
                  <FileText size={15} />
                </span>
                <span className="documents-row-copy">
                  <strong>{failure.name}</strong>
                  <small className="is-attention">{failure.reason}</small>
                </span>
              </li>
            ))}
          </ul>
        </Card>
      ) : null}
      <div
        className={`documents-layout${selected || inYear.length || yearInsight?.income ? '' : ' is-single'}`}
      >
        <DocumentList
          data={data}
          documents={shown}
          year={year}
          searching={terms.length > 0}
          selected={selected?.id}
        />
        {selected ? (
          <DocumentPanel
            key={selected.id}
            data={data}
            document={selected}
            insights={insights}
            onView={() => setViewing(true)}
          />
        ) : inYear.length || yearInsight?.income ? (
          <YearSummary
            data={data}
            year={year}
            today={today}
            documents={inYear}
            insight={yearInsight}
          />
        ) : null}
      </div>
      {selected && viewing ? <DocumentViewer document={selected} onClose={closeViewer} /> : null}
      {dragging ? (
        <div className="documents-drop" aria-hidden="true">
          <FolderPlus size={16} />
          <span>Drop to add to Documents</span>
        </div>
      ) : null}
    </div>
  )
}

/**
 * The original, shown in Brief: a PDF in the system's own PDF view, a photo as itself, an
 * export as text. It's decrypted in memory and never written to disk. Escape or Space closes
 * it first, before anything else on the page.
 */
function DocumentViewer({ document, onClose }: { document: BriefDocument; onClose: () => void }) {
  const [shown, setShown] = useState<{ url?: string; text?: string; error?: string }>({})
  const spreadsheet = document.mediaType.includes('spreadsheet')
  useEffect(() => {
    if (spreadsheet) return undefined
    let live = true
    let url: string | undefined
    documentFile(document.id)
      .then((buffer) => {
        if (!live) return
        if (document.mediaType.startsWith('text/')) {
          setShown({ text: new TextDecoder().decode(buffer) })
        } else {
          url = URL.createObjectURL(new Blob([buffer], { type: document.mediaType }))
          setShown({ url })
        }
      })
      .catch((error) => {
        if (live) setShown({ error: errorMessage(error) })
      })
    return () => {
      live = false
      if (url) URL.revokeObjectURL(url)
    }
  }, [document.id, document.mediaType, spreadsheet])
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const typing = event.target instanceof HTMLInputElement
      if (event.key !== 'Escape' && !(event.key === ' ' && !typing)) return
      event.preventDefault()
      event.stopImmediatePropagation()
      onClose()
    }
    window.addEventListener('keydown', onKeyDown, true)
    return () => window.removeEventListener('keydown', onKeyDown, true)
  }, [onClose])
  return (
    <div className="documents-viewer" role="dialog" aria-modal="true" aria-label={title(document)}>
      <div className="documents-viewer-backdrop" aria-hidden="true" onClick={onClose} />
      <section className="documents-viewer-sheet">
        <header className="documents-viewer-header">
          <span>
            <strong>{title(document)}</strong>
            <small>{document.fileName}</small>
          </span>
          <Button
            size="compact"
            data-command="documents.open-original"
            onClick={() =>
              void openDocument(document.id).catch((error) =>
                toast.error('Could not open the document', { description: errorMessage(error) }),
              )
            }
          >
            Open in Preview
          </Button>
          <Button
            icon={X}
            size="icon-compact"
            variant="ghost"
            aria-label="Close the original"
            data-command="documents.view"
            onClick={onClose}
          />
        </header>
        <div className="documents-viewer-body sensitive-value">
          {spreadsheet ? (
            <EmptyState>Brief can’t show a spreadsheet; open it in Preview or Numbers.</EmptyState>
          ) : shown.error ? (
            <EmptyState>{shown.error}</EmptyState>
          ) : shown.text != null ? (
            <pre className="documents-viewer-text">{shown.text}</pre>
          ) : shown.url && document.mediaType === 'application/pdf' ? (
            <iframe src={shown.url} title={`${title(document)}, the original`} />
          ) : shown.url ? (
            <img src={shown.url} alt={`${title(document)}, the original`} />
          ) : (
            <EmptyState>Opening…</EmptyState>
          )}
        </div>
      </section>
    </div>
  )
}

function DocumentList({
  data,
  documents,
  year,
  searching,
  selected,
}: {
  data: FinanceSnapshot
  documents: BriefDocument[]
  year: number
  /** The list holds a search's matches from every year. */
  searching: boolean
  selected?: string
}) {
  const commit = useCommit()
  const navigate = useNavigate()
  const names = financeSettings(data).accountNames
  const accountName = (id: string) =>
    accountDisplayName(
      id,
      data.accounts.find((account) => account.id === id)?.name ?? 'Account',
      names,
    )
  // ↑ and ↓ step through every group in the order shown, opening each document beside the list.
  const order = groups.flatMap(({ kinds }) => grouped(documents, kinds))
  const step = useCallback(
    (direction: -1 | 1) => {
      const index = order.findIndex((document) => document.id === selected)
      const next =
        index < 0 ? order[direction > 0 ? 0 : order.length - 1] : order[index + direction]
      if (next)
        void navigate({
          to: '/documents',
          search: { year: next.taxYear ?? year, document: next.id },
          replace: true,
        })
    },
    [navigate, order, selected, year],
  )
  useRegisterActions(
    useMemo<AppAction[]>(
      () =>
        (
          [
            ['next', 'Next document', 'ArrowDown', '↓', 'j', 1],
            ['previous', 'Previous document', 'ArrowUp', '↑', 'k', -1],
          ] as const
        ).map(([id, name, arrow, arrowLabel, letter, direction]) => ({
          id: `documents.${id}`,
          title: name,
          search: 'document list move open details',
          group: 'View',
          scope: 'page',
          bindings: [
            { sequence: [arrow], label: arrowLabel },
            { sequence: [letter], label: letter.toUpperCase() },
          ],
          repeatable: true,
          disabled: !order.length,
          disabledReason: 'No documents shown',
          run: () => step(direction),
        })),
      [order.length, step],
    ),
  )
  // The open row stays in view as the arrows move through a long list.
  useEffect(() => {
    globalThis.document
      ?.querySelector('.documents-row.is-selected')
      ?.scrollIntoView({ block: 'nearest' })
  }, [selected])
  if (!documents.length && searching)
    return (
      <Card aria-label="Documents" className="documents-section">
        <SectionHeading title="No matches" />
        <EmptyState>No document in any year says that.</EmptyState>
      </Card>
    )
  if (!documents.length)
    return (
      <Card aria-label="Documents" className="documents-section">
        <SectionHeading title={`${year}`} />
        <EmptyState>
          Drop pay statements, W-2s, 1099s, or bank statements here. They’re read on this Mac and
          kept encrypted.
        </EmptyState>
      </Card>
    )
  const addHistory = (ids: string[], accountId: string) =>
    void addDocumentHistory(ids, accountId)
      .then((snapshot) => {
        commit(snapshot)
        toast.success(
          `Added ${ids.length} ${ids.length === 1 ? 'statement' : 'statements'} to history`,
        )
      })
      .catch((error) =>
        toast.error('Could not add the history', { description: errorMessage(error) }),
      )
  return (
    <div className="documents-list">
      {groups.map((group) => {
        const members = grouped(documents, group.kinds)
        if (!members.length) return null
        // Statements an account can take, not added yet, offered together.
        const ready = members.filter(
          (document) =>
            document.accountId &&
            !document.statementFileId &&
            (document.statementCsv || document.kind === 'export'),
        )
        const account = ready[0]?.accountId
        return (
          <Card key={group.title} aria-label={group.title} className="documents-section">
            <SectionHeading
              title={
                <span className="documents-group-title">
                  {group.title}
                  <span className="documents-group-count">{members.length}</span>
                </span>
              }
              action={
                !searching &&
                account &&
                ready.every((document) => document.accountId === account) ? (
                  <Button
                    size="compact"
                    data-command="documents.add-history"
                    onClick={() =>
                      addHistory(
                        ready.map((document) => document.id),
                        account,
                      )
                    }
                  >
                    Add {ready.length} to {accountName(account)}
                  </Button>
                ) : undefined
              }
            />
            {/* Not a keyboard region: the page's ↑ and ↓ move through every group at once. */}
            <ul className="documents-rows" aria-label={group.title}>
              {members.map((document) => {
                const amount = headline(document)
                const state = standing(document)
                return (
                  <li key={document.id}>
                    <Link
                      className={`documents-row${document.id === selected ? ' is-selected' : ''}`}
                      data-keyboard-row
                      data-keyboard-open
                      to="/documents"
                      search={{ year: document.taxYear ?? year, document: document.id }}
                    >
                      <DocumentMark document={document} />
                      <span className="documents-row-copy">
                        <strong>{title(document)}</strong>
                        <small>
                          {[
                            document.date
                              ? (searching ? formatDay : formatShortDay)(document.date)
                              : null,
                            document.accountId ? accountName(document.accountId) : null,
                            state.tone === 'quiet' ? state.label : null,
                          ]
                            .filter(Boolean)
                            .join(' · ')}
                        </small>
                      </span>
                      <span className="documents-row-end">
                        {amount != null ? (
                          <span className="documents-row-amount sensitive-value">
                            {formatCurrency(amount)}
                          </span>
                        ) : null}
                        {state.tone === 'quiet' ? null : (
                          <span className={`documents-badge is-${state.tone}`} title={state.label}>
                            {state.tone === 'done' ? <Check size={12} aria-hidden="true" /> : null}
                            {state.label}
                          </span>
                        )}
                      </span>
                    </Link>
                  </li>
                )
              })}
            </ul>
          </Card>
        )
      })}
    </div>
  )
}

/** One figure, edited in place; a typed value is kept over any later reading. */
function FieldRow({
  field,
  payslip,
  onChange,
}: {
  field: DocumentField
  payslip: boolean
  onChange: (field: DocumentField) => void
}) {
  const [draft, setDraft] = useState<{ amount?: string; ytd?: string }>({})
  const commitValue = (key: 'amount' | 'ytd') => {
    const text = draft[key]
    setDraft((current) => ({ ...current, [key]: undefined }))
    if (text === undefined) return
    const value = text.trim() === '' ? undefined : Number(text.replace(/[$,\s]/g, ''))
    if (value !== undefined && !Number.isFinite(value)) {
      toast.error('Enter an amount')
      return
    }
    onChange({ ...field, [key]: value, edited: true, uncertain: false })
  }
  const doubtful = field.uncertain && !field.edited
  const input = (key: 'amount' | 'ytd', label: string) => (
    <input
      className="documents-amount sensitive-value"
      inputMode="decimal"
      aria-label={`${field.label} ${label}`}
      aria-invalid={doubtful || undefined}
      value={draft[key] ?? (field[key] != null ? field[key].toFixed(2) : '')}
      placeholder="—"
      onChange={(event) => setDraft((current) => ({ ...current, [key]: event.target.value }))}
      onBlur={() => commitValue(key)}
      onKeyDown={(event) => {
        if (event.key === 'Enter') commitValue(key)
      }}
    />
  )
  return (
    <li className={`documents-field${doubtful ? ' is-doubtful' : ''}${payslip ? ' has-ytd' : ''}`}>
      <span className="documents-field-label">
        {field.label}
        {doubtful ? <small title="Compare with the original">Check</small> : null}
        {field.edited ? <small>Yours</small> : null}
      </span>
      {field.text != null ? (
        <span className="documents-field-text">{field.text}</span>
      ) : (
        input('amount', payslip ? 'this period' : 'amount')
      )}
      {payslip ? input('ytd', 'year to date') : null}
    </li>
  )
}

function DocumentPanel({
  data,
  document,
  insights,
  onView,
}: {
  data: FinanceSnapshot
  document: BriefDocument
  insights?: DocumentInsights
  onView: () => void
}) {
  const commit = useCommit()
  const navigate = useNavigate()
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [busy, setBusy] = useState<string>()
  const names = financeSettings(data).accountNames
  const accountName = (id: string) =>
    accountDisplayName(
      id,
      data.accounts.find((account) => account.id === id)?.name ?? 'Account',
      names,
    )
  const keep = insights?.keep.find((item) => item.documentId === document.id)
  const pay = insights?.pay.filter((link) => link.documentId === document.id) ?? []
  const prediction = insights?.years
    .flatMap((year) => year.predictions)
    .find((item) => item.w2 === document.id)
  const run = async (what: string, task: () => Promise<FinanceSnapshot | void>) => {
    setBusy(what)
    try {
      const snapshot = await task()
      if (snapshot) commit(snapshot)
    } catch (error) {
      toast.error(`Could not ${what}`, { description: errorMessage(error) })
    } finally {
      setBusy(undefined)
    }
  }
  const change = (next: DocumentChange) =>
    void run('save the document', () => updateDocument(document.id, next))
  const payslip = document.kind === 'payslip'
  const open = toCheck(document)

  useRegisterActions(
    useMemo<AppAction[]>(
      () => [
        {
          id: 'documents.open-original',
          title: 'Open the original in Preview',
          group: 'Actions',
          scope: 'page',
          bindings: [{ sequence: ['Meta+Enter'], label: '⌘↩' }],
          run: () => void run('open the document', () => openDocument(document.id)),
        },
        {
          id: 'documents.reviewed',
          title: document.reviewed ? 'Mark not checked' : 'Mark checked',
          group: 'Actions',
          scope: 'page',
          // Shift arrives in the key itself, so ⇧⌘C is Meta+C.
          bindings: [{ sequence: ['Meta+C'], label: '⇧⌘C' }],
          run: () => change({ reviewed: !document.reviewed }),
        },
        {
          id: 'documents.reread',
          title: 'Read the document again',
          search: 'reread scan ocr again',
          group: 'Actions',
          scope: 'page',
          bindings: [{ sequence: ['r'], label: 'R' }],
          run: () => void run('read the document again', () => rereadDocument(document.id)),
        },
        {
          id: 'documents.delete',
          title: 'Delete document…',
          group: 'Actions',
          scope: 'page',
          // Bare, so it pauses while a field has focus; it only asks first.
          bindings: [{ sequence: ['Backspace'], label: '⌫' }],
          run: () => setConfirmDelete(true),
        },
        ...(['kind', 'account'] as const).map((field) => ({
          id: `documents.${field}`,
          title: field === 'kind' ? 'Change what the document is' : 'Change its account',
          group: 'Actions',
          scope: 'page' as const,
          run: () =>
            globalThis.document
              .querySelector<HTMLElement>(`[data-command="documents.${field}"]`)
              ?.click(),
        })),
      ],
      // eslint-disable-next-line react-hooks/exhaustive-deps
      [document.id, document.reviewed],
    ),
  )

  return (
    <Card aria-label={title(document)} className="documents-section documents-panel">
      <SectionHeading
        title={title(document)}
        action={
          <Button
            size="icon-compact"
            variant="ghost"
            icon={X}
            aria-keyshortcuts="Escape"
            data-command="documents.close"
            onClick={() => void navigate({ to: '/documents', search: {} })}
          >
            Close
          </Button>
        }
      />
      <p className="documents-meta">
        <span className="documents-kind" style={{ '--kind-color': kindStyle[document.kind].color }}>
          {kindNames[document.kind]}
        </span>
        {document.date ? <span>{formatDay(document.date)}</span> : null}
        <span title={document.fileName}>{document.fileName}</span>
        {keep ? (
          <span>{keep.until ? `Keep until ${formatDay(keep.until)}` : keep.reason}</span>
        ) : null}
      </p>
      {open && !document.reviewed ? (
        <p className="documents-callout" role="status">
          {countOf(open, 'figure')} to compare with the original
        </p>
      ) : null}
      {document.summary ? (
        <section className="documents-summary-note" aria-label="Summary">
          <p>{document.summary}</p>
          <small>
            <Sparkles size={12} aria-hidden="true" />
            Apple Intelligence, on this Mac · figures from the document
          </small>
        </section>
      ) : null}
      <div className="documents-actions">
        <Button
          icon={Eye}
          size="compact"
          disabled={!isTauri()}
          aria-keyshortcuts="Space"
          data-command="documents.view"
          onClick={onView}
        >
          View
        </Button>
        <Button
          icon={FileText}
          size="compact"
          disabled={!!busy}
          aria-keyshortcuts="Meta+Enter"
          data-command="documents.open-original"
          onClick={() => void run('open the document', () => openDocument(document.id))}
        >
          Open in Preview
        </Button>
        <Button
          icon={Check}
          size="compact"
          variant={document.reviewed ? 'secondary' : 'positive'}
          disabled={!!busy}
          aria-pressed={document.reviewed ?? false}
          aria-keyshortcuts="Meta+Shift+C"
          data-command="documents.reviewed"
          onClick={() => change({ reviewed: !document.reviewed })}
        >
          {document.reviewed ? 'Checked' : 'Mark checked'}
        </Button>
        <Button
          icon={RotateCcw}
          size="compact"
          variant="ghost"
          disabled={!!busy}
          aria-busy={busy === 'read the document again' || undefined}
          aria-keyshortcuts="r"
          data-command="documents.reread"
          onClick={() => void run('read the document again', () => rereadDocument(document.id))}
        >
          {busy === 'read the document again' ? 'Reading…' : 'Reread'}
        </Button>
        <span className="documents-actions-end">
          {confirmDelete ? (
            <>
              <Button
                size="compact"
                data-command="documents.delete"
                onClick={() => setConfirmDelete(false)}
                autoFocus
              >
                Cancel
              </Button>
              <Button
                size="compact"
                variant="destructive"
                disabled={!!busy}
                aria-busy={busy === 'delete the document' || undefined}
                data-command="documents.delete"
                onClick={() =>
                  void run('delete the document', async () => {
                    const snapshot = await deleteDocuments([document.id])
                    await navigate({ to: '/documents', search: {} })
                    return snapshot
                  })
                }
              >
                {busy === 'delete the document' ? 'Deleting…' : 'Delete'}
              </Button>
            </>
          ) : (
            <Button
              size="icon-compact"
              variant="ghost"
              icon={Trash2}
              aria-keyshortcuts="Backspace"
              data-command="documents.delete"
              onClick={() => setConfirmDelete(true)}
            >
              Delete document…
            </Button>
          )}
        </span>
      </div>
      <div className="documents-details">
        <label>
          <span>Kind</span>
          <FilterSelect
            label="Kind of document"
            command="documents.kind"
            value={document.kind}
            options={Object.entries(kindNames).map(([value, label]) => ({ value, label }))}
            onValueChange={(value) => {
              const kind = documentKinds.find((known) => known === value)
              if (kind) change({ kind })
            }}
          />
        </label>
        <label>
          <span>Tax year</span>
          <input
            className="documents-input"
            inputMode="numeric"
            aria-label="Tax year"
            defaultValue={document.taxYear ?? ''}
            onBlur={(event) => {
              const year = Number(event.target.value)
              if (event.target.value && year !== document.taxYear) change({ taxYear: year })
            }}
          />
        </label>
        <label>
          <span>From</span>
          <input
            className="documents-input"
            aria-label="Issuer"
            defaultValue={document.issuer ?? ''}
            onBlur={(event) => {
              if (event.target.value !== (document.issuer ?? ''))
                change({ issuer: event.target.value })
            }}
          />
        </label>
        <label>
          <span>Account</span>
          <FilterSelect
            label="Account"
            command="documents.account"
            value={document.accountId ?? ''}
            options={[
              { value: '', label: document.mask ? `None · ends ${document.mask}` : 'None' },
              ...data.accounts
                .filter((account) => account.type !== 'combined')
                .map((account) => ({
                  value: account.id,
                  label: `${accountName(account.id)}${account.mask ? ` ··${account.mask}` : ''}`,
                })),
            ]}
            onValueChange={(accountId) => change({ accountId })}
          />
        </label>
      </div>
      {document.fields.length ? (
        <section className="documents-block" aria-label="What it says">
          <header className={`documents-field documents-field-head${payslip ? ' has-ytd' : ''}`}>
            <span>Figures</span>
            <span>{payslip ? 'This period' : 'Amount'}</span>
            {payslip ? <span>Year to date</span> : null}
          </header>
          <ul className="documents-fields">
            {document.fields.map((field) => (
              <FieldRow
                key={field.key}
                field={field}
                payslip={payslip}
                onChange={(next) =>
                  change({
                    fields: document.fields.map((item) => (item.key === next.key ? next : item)),
                  })
                }
              />
            ))}
          </ul>
        </section>
      ) : null}
      {document.checks.length ? (
        <ul className="documents-checks" aria-label="Checks">
          {document.checks.map((check) => (
            <li key={check.label} className={check.passed ? 'is-done' : 'is-attention'}>
              <span aria-hidden="true">{check.passed ? '✓' : '!'}</span>
              {check.label}
            </li>
          ))}
        </ul>
      ) : null}
      {pay.length ? (
        <section className="documents-block" aria-label="Deposits">
          <h3 className="documents-block-title">Deposits</h3>
          <ul className="documents-checks">
            {pay.map((link) => (
              <li
                key={`${link.mask}:${link.amount}`}
                className={link.transactionId ? 'is-done' : link.accountId ? 'is-attention' : ''}
              >
                <span aria-hidden="true">
                  {link.transactionId ? '✓' : link.accountId ? '!' : '·'}
                </span>
                <span className="sensitive-value">{formatCurrency(link.amount)}</span>
                {link.transactionId ? (
                  <Link
                    to="/activities"
                    search={{ open: link.transactionId }}
                    data-command="navigation.open./activities"
                  >
                    In {link.accountId ? accountName(link.accountId) : 'your account'}
                  </Link>
                ) : link.accountId ? (
                  `Not yet in ${accountName(link.accountId)}`
                ) : (
                  `Untracked account${link.mask ? ` ··${link.mask}` : ''}`
                )}
              </li>
            ))}
          </ul>
        </section>
      ) : null}
      {prediction ? (
        <section className="documents-block" aria-label="Against your pay statements">
          <header className="documents-field documents-field-head has-ytd">
            <span>Against pay statements</span>
            <span>Pay</span>
            <span>W-2</span>
          </header>
          <ul className="documents-fields">
            {prediction.boxes.map((item) => {
              const differs = item.actual != null && Math.abs(item.actual - item.predicted) > 1
              return (
                <li
                  key={item.key}
                  className={`documents-field has-ytd${differs ? ' is-doubtful' : ''}`}
                >
                  <span className="documents-field-label">{item.label}</span>
                  <span className="documents-field-text sensitive-value">
                    {formatCurrency(item.predicted)}
                  </span>
                  <span className="documents-field-text sensitive-value">
                    {item.actual == null
                      ? 'Not read'
                      : differs
                        ? formatCurrency(item.actual)
                        : 'Matches'}
                  </span>
                </li>
              )
            })}
          </ul>
        </section>
      ) : null}
      {document.statementCsv || document.kind === 'export' ? (
        <div className="documents-history">
          <span>
            {document.statementFileId
              ? `In ${document.accountId ? accountName(document.accountId) : 'an account'}’s history`
              : document.statementRows
                ? `${countOf(document.statementRows, 'balanced row')} ready`
                : 'No rows'}
          </span>
          {!document.statementFileId && document.accountId && document.statementRows ? (
            <Button
              size="compact"
              data-command="documents.add-history"
              disabled={!!busy}
              onClick={() =>
                void run('add the history', () =>
                  addDocumentHistory([document.id], document.accountId ?? ''),
                )
              }
            >
              Add to {accountName(document.accountId)}
            </Button>
          ) : null}
        </div>
      ) : null}
    </Card>
  )
}

/** The year's monthly documents in rows, a cell per month, for spotting the month that's missing. */
const coverageLanes: { title: string; kinds: BriefDocument['kind'][]; noun: string }[] = [
  { title: 'Pay', kinds: ['payslip'], noun: 'pay statement' },
  { title: 'Statements', kinds: ['statement', 'export'], noun: 'statement' },
]

function Coverage({
  documents,
  year,
  today,
}: {
  documents: BriefDocument[]
  year: number
  today: string
}) {
  // Months of this year still ahead aren't missing yet.
  const elapsed =
    year < Number(today.slice(0, 4))
      ? 12
      : year > Number(today.slice(0, 4))
        ? 0
        : Number(today.slice(5, 7))
  const lanes = coverageLanes.flatMap((lane) => {
    const members = documents.filter(
      (document) => lane.kinds.includes(document.kind) && document.date?.startsWith(`${year}-`),
    )
    if (!members.length) return []
    const byMonth = Array.from(
      { length: 12 },
      (_, month) =>
        members.filter((document) => Number(document.date?.slice(5, 7)) === month + 1).length,
    )
    return [{ ...lane, byMonth, color: kindStyle[lane.kinds[0]].color }]
  })
  const forms = documents.filter(
    (document) => document.kind === 'taxForm' || document.kind === 'taxReturn',
  )
  if (!lanes.length && !forms.length) return null
  return (
    <Card aria-label={`${year} coverage`} className="documents-section">
      <SectionHeading title="Coverage" />
      {lanes.length ? (
        <div className="documents-coverage" role="table" aria-label="Documents by month">
          <div className="documents-coverage-row is-head" role="row">
            <span role="columnheader" />
            {monthInitials.map((initial, month) => (
              <span key={month} role="columnheader" aria-label={monthNames[month]}>
                {initial}
              </span>
            ))}
            <span role="columnheader" />
          </div>
          {lanes.map((lane) => {
            const filled = lane.byMonth.filter((count, month) => count && month < elapsed).length
            return (
              <div
                key={lane.title}
                className="documents-coverage-row"
                role="row"
                style={{ '--kind-color': lane.color }}
              >
                <span role="rowheader">{lane.title}</span>
                {lane.byMonth.map((count, month) => (
                  <span
                    key={month}
                    role="cell"
                    className={`documents-cell${count ? ' is-filled' : month < elapsed ? ' is-missing' : ''}`}
                    title={`${monthNames[month]} · ${count ? countOf(count, lane.noun) : 'none'}`}
                    aria-label={`${monthNames[month]}: ${count ? countOf(count, lane.noun) : 'none'}`}
                  />
                ))}
                <span role="cell" className="documents-coverage-total">
                  {elapsed ? `${filled}/${elapsed}` : null}
                </span>
              </div>
            )
          })}
        </div>
      ) : null}
      {forms.length ? (
        <ul className="documents-forms" aria-label="Tax forms received">
          {forms.map((document) => (
            <li key={document.id}>
              <Link
                to="/documents"
                search={{ year, document: document.id }}
                className="documents-form-chip"
                title={title(document)}
              >
                <DocumentMark document={document} />
                <span>{document.issuer ? displayIssuer(document) : kindNames[document.kind]}</span>
              </Link>
            </li>
          ))}
        </ul>
      ) : null}
    </Card>
  )
}

/** The issuer alone, as the list titles it after the form. */
const displayIssuer = (document: BriefDocument) => title(document).split(' · ').slice(1).join(' · ')

function YearSummary({
  data,
  year,
  today,
  documents,
  insight,
}: {
  data: FinanceSnapshot
  year: number
  today: string
  documents: BriefDocument[]
  insight?: YearInsight
}) {
  const { updateSettings } = useFinanceEdits()
  const profile = financeSettings(data).taxProfile
  const income = insight?.income
  return (
    <div className="documents-summary">
      <Coverage documents={documents} year={year} today={today} />
      {income ? (
        <Card aria-label={`Income ${year} from documents`} className="documents-section">
          <SectionHeading title="Income" />
          <dl className="documents-ledger">
            <div>
              <dt>
                Wages
                <small>{income.wagesFrom === 'w2' ? 'W-2s' : 'Pay statements'}</small>
              </dt>
              <dd className="sensitive-value">{formatCurrency(income.wages)}</dd>
            </div>
            {income.selfEmployment > 0 ? (
              <>
                <div>
                  <dt>Contractor pay</dt>
                  <dd className="sensitive-value">{formatCurrency(income.selfEmployment)}</dd>
                </div>
                <div>
                  <dt>
                    Self-employment tax
                    <small>Nothing withheld</small>
                  </dt>
                  <dd className="sensitive-value negative">
                    {formatCurrency(income.selfEmploymentTax)}
                  </dd>
                </div>
              </>
            ) : null}
            {income.taxableIncome != null ? (
              <div className="is-total">
                <dt>
                  Taxable income
                  <small>After the standard deduction</small>
                </dt>
                <dd className="sensitive-value">{formatCurrency(income.taxableIncome)}</dd>
              </div>
            ) : null}
          </dl>
          {income.taxableIncome != null &&
          profile &&
          year === Number(data.insights?.today.slice(0, 4)) ? (
            profile.taxableIncome === income.taxableIncome ? (
              <p className="documents-note">Your tax estimate uses this.</p>
            ) : (
              <Button
                size="compact"
                data-command="documents.use-income"
                onClick={() =>
                  void updateSettings((current) => ({
                    ...current,
                    taxProfile: current.taxProfile
                      ? { ...current.taxProfile, taxableIncome: income.taxableIncome ?? 0 }
                      : current.taxProfile,
                  })).catch((error) =>
                    toast.error('Could not update the tax profile', {
                      description: errorMessage(error),
                    }),
                  )
                }
              >
                Use in tax estimate
              </Button>
            )
          ) : !profile ? (
            <p className="documents-note">
              <SettingsLink target="tax-filing-status">Add your filing status</SettingsLink> to see
              taxable income.
            </p>
          ) : null}
        </Card>
      ) : null}
    </div>
  )
}
