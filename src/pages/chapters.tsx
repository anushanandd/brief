import { useNavigate, useSearch } from '@tanstack/react-router'
import { useCallback, useMemo, useRef, useState, type FormEvent } from 'react'

import { Check, Plus, Trash2, X } from '../components/icons'
import { useShowObject } from '../components/object-details'
import { Button, Card, EmptyState, Metric, SectionHeading } from '../components/ui'
import { useFinanceEdits } from '../hooks/use-finance-edits'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import type { ChapterChoice } from '../lib/bindings/ChapterChoice'
import type { PlannedChapter } from '../lib/bindings/PlannedChapter'
import { errorMessage } from '../lib/errors'
import {
  chapterKinds,
  chapterSpan,
  evidenceOf,
  nowLine,
  tierLabels,
  type Chapter,
  type ChapterKind,
} from '../lib/evidence'
import { formatCurrency, formatDay } from '../lib/format'
import type { FinanceSnapshot } from '../lib/schema'

const LANES: ChapterKind[] = ['work', 'home', 'milestone', 'shift', 'planned']

const chapterSearch = (chapter?: Pick<Chapter, 'id'>) =>
  chapter ? { view: 'chapters' as const, chapter: chapter.id } : { view: 'chapters' as const }

/** Whose word a chapter carries: yours once confirmed or planned, else a possibility. */
const standing = (chapter: Chapter) =>
  chapter.kind === 'planned'
    ? 'Planned'
    : chapter.confirmed
      ? 'Confirmed'
      : tierLabels.suggested.label

/**
 * Insights' Chapters view: spans of your life with a cause — jobs, homes, accounts opened
 * together, step changes in spending — detected by Rust from what Brief observed, and the
 * chapters you plan ahead. Detected chapters are possibilities until you confirm them; a
 * confirmed chapter starts a new normal for typical spending and usual amounts.
 */
export function ChaptersView({ data }: { data: FinanceSnapshot }) {
  const search = useSearch({ from: '/insights' })
  const navigate = useNavigate({ from: '/insights' })
  const evidence = evidenceOf(data)
  // With none chosen, the latest chapter still open is shown, so the detail is never blank.
  const chapter =
    evidence.chapters.find((candidate) => candidate.id === search.chapter) ??
    evidence.chapters.findLast((candidate) => !candidate.end) ??
    evidence.chapters.at(-1)
  const [planning, setPlanning] = useState(false)
  const stopPlanning = useCallback(() => setPlanning(false), [])

  useRegisterActions(
    useMemo<AppAction[]>(
      () => [
        {
          id: 'chapters.plan',
          title: 'Plan a chapter',
          search: 'chapter plan moving out future costs rent',
          group: 'Here',
          scope: 'page',
          bindings: [{ sequence: ['n'], label: 'N' }],
          run: () => setPlanning(true),
        },
        ...evidence.chapters.map((item) => ({
          id: `chapters.open.${item.id}`,
          title: `Open ${item.title}`,
          search: `chapter ${chapterKinds[item.kind].label} ${item.title}`,
          group: 'Here',
          scope: 'page' as const,
          run: () => void navigate({ search: chapterSearch(item) }),
        })),
      ],
      [evidence.chapters, navigate],
    ),
  )

  if (!data.insights) {
    return (
      <Card className="chapters-card">
        <EmptyState>Chapters are unavailable for this snapshot.</EmptyState>
      </Card>
    )
  }
  return (
    <div className="trips-main">
      <Timeline
        data={data}
        current={chapter?.id}
        onOpen={(item) => void navigate({ search: chapterSearch(item) })}
      />
      <div className="chapters-grid">
        <ChapterList
          chapters={evidence.chapters}
          current={chapter?.id}
          onOpen={(item) => void navigate({ search: chapterSearch(item) })}
          onPlan={() => setPlanning(true)}
        />
        {planning ? (
          <PlanForm data={data} onDone={stopPlanning} />
        ) : chapter ? (
          <ChapterDetail key={chapter.id} data={data} chapter={chapter} />
        ) : null}
      </div>
    </div>
  )
}

/** A date as milliseconds, for placing bars on the timeline. */
const day = (date: string) => new Date(`${date}T00:00:00Z`).getTime()

/**
 * A lane per kind, each chapter a bar across the span of your history, over a year axis. Its
 * heading says where you are now; where you lived each year is on Taxes, beside the return it
 * shapes.
 */
function Timeline({
  data,
  current,
  onOpen,
}: {
  data: FinanceSnapshot
  current?: string
  onOpen: (chapter: Chapter) => void
}) {
  const evidence = evidenceOf(data)
  const { chapters } = evidence
  const today = data.insights?.today ?? data.updatedAt.slice(0, 10)
  const line = nowLine(evidence)
  const baselineSince = data.insights?.plan.baselineSince
  if (!chapters.length) return null
  const first = Math.min(...chapters.map((chapter) => day(chapter.start)))
  const last = Math.max(day(today), ...chapters.map((chapter) => day(chapter.end ?? today)))
  const span = Math.max(1, last - first)
  const at = (date: string) => ((day(date) - first) / span) * 100
  const firstYear = new Date(first).getUTCFullYear()
  const years = Array.from(
    { length: new Date(last).getUTCFullYear() - firstYear },
    (_, index) => firstYear + index + 1,
  )
  return (
    <Card className="chapters-card">
      <SectionHeading
        title="Timeline"
        detail={line || 'No open chapter says where you work or live.'}
      />
      <div className="chapters-timeline" role="list" aria-label="Chapters over time">
        {LANES.filter((lane) => chapters.some((chapter) => chapter.kind === lane)).map((lane) => (
          <div key={lane} className="chapters-lane" role="listitem">
            <span className="chapters-lane-label">{chapterKinds[lane].lane}</span>
            <div className="chapters-lane-track">
              {chapters
                .filter((chapter) => chapter.kind === lane)
                .map((chapter) => {
                  const left = at(chapter.start)
                  const width = Math.max(0.8, at(chapter.end ?? today) - left)
                  return (
                    <button
                      key={chapter.id}
                      type="button"
                      className={`chapters-bar chapters-bar-${chapter.kind}${chapter.confirmed || chapter.kind === 'planned' ? ' is-confirmed' : ''}`}
                      aria-current={chapter.id === current || undefined}
                      style={{ left: `${left}%`, width: `${width}%` }}
                      title={`${chapter.title} · ${chapterSpan(chapter)} · ${standing(chapter)}`}
                      data-command={`chapters.open.${chapter.id}`}
                      onClick={() => onOpen(chapter)}
                    >
                      <span>{chapter.title}</span>
                    </button>
                  )
                })}
            </div>
          </div>
        ))}
        <div className="chapters-lane chapters-axis" aria-hidden="true">
          <span />
          <div className="chapters-lane-track">
            {years.map((year) => (
              <span key={year} style={{ left: `${at(`${year}-01-01`)}%` }}>
                {year}
              </span>
            ))}
          </div>
        </div>
      </div>
      {baselineSince ? (
        <p className="chapters-note">
          Typical spending counts from {formatDay(baselineSince)}, when a chapter you confirmed
          began.
        </p>
      ) : null}
    </Card>
  )
}

function ChapterList({
  chapters,
  current,
  onOpen,
  onPlan,
}: {
  chapters: Chapter[]
  current?: string
  onOpen: (chapter: Chapter) => void
  onPlan: () => void
}) {
  return (
    <Card className="chapters-card">
      <SectionHeading
        title="Chapters"
        action={
          <Button size="compact" icon={Plus} data-command="chapters.plan" onClick={onPlan}>
            Plan a chapter
          </Button>
        }
      />
      {chapters.length ? (
        <ul className="chapters-list" data-keyboard-region aria-label="Chapters">
          {chapters.toReversed().map((chapter) => (
            <li key={chapter.id}>
              <button
                type="button"
                className="chapters-row"
                data-keyboard-row
                data-keyboard-open
                aria-current={chapter.id === current || undefined}
                onClick={() => onOpen(chapter)}
              >
                <span className="chapters-row-copy">
                  <strong>{chapter.title}</strong>
                  <small>
                    {chapterKinds[chapter.kind].label} · {chapterSpan(chapter)}
                  </small>
                </span>
                <span
                  className={`tier-label tier-${chapter.confirmed || chapter.kind === 'planned' ? 'observed' : 'suggested'}`}
                >
                  {standing(chapter)}
                </span>
              </button>
            </li>
          ))}
        </ul>
      ) : (
        <EmptyState>
          No chapters yet. Pay statements, W-2s, and in-person purchases let Brief find jobs and
          homes.
        </EmptyState>
      )}
    </Card>
  )
}

/** One chapter: what defines it, what belongs to it, what it changed, and what its end changes. */
function ChapterDetail({ data, chapter }: { data: FinanceSnapshot; chapter: Chapter }) {
  const { updateSettings } = useFinanceEdits()
  const showObject = useShowObject()
  const navigate = useNavigate({ from: '/insights' })
  const [title, setTitle] = useState(chapter.title)
  const [start, setStart] = useState(chapter.start)
  const [end, setEnd] = useState(chapter.end ?? '')
  const [editing, setEditing] = useState(false)
  const [error, setError] = useState<string>()
  const programs = evidenceOf(data).programs
  const planned = chapter.kind === 'planned'

  const choose = async (choice: Partial<ChapterChoice> & Pick<ChapterChoice, 'status'>) => {
    setError(undefined)
    try {
      await updateSettings((current) => ({
        ...current,
        chapters: [
          ...(current.chapters ?? []).filter((known) => known.id !== chapter.id),
          { id: chapter.id, ...choice },
        ],
      }))
      setEditing(false)
      if (choice.status === 'dismissed') await navigate({ search: chapterSearch() })
    } catch (caught) {
      setError(errorMessage(caught))
    }
  }
  const confirmEdits = () =>
    void choose({
      status: 'confirmed',
      ...(title.trim() && title.trim() !== chapter.title ? { title: title.trim() } : {}),
      ...(start !== chapter.start ? { start } : {}),
      ...(end && end !== chapter.end ? { end } : {}),
    })
  const removePlan = async () => {
    try {
      await updateSettings((current) => ({
        ...current,
        plannedChapters: (current.plannedChapters ?? []).filter((plan) => plan.id !== chapter.id),
      }))
      await navigate({ search: chapterSearch() })
    } catch (caught) {
      setError(errorMessage(caught))
    }
  }
  const latest = useRef({ confirmEdits, choose, removePlan })
  latest.current = { confirmEdits, choose, removePlan }
  useRegisterActions(
    useMemo<AppAction[]>(
      () =>
        planned
          ? [
              {
                id: 'chapter.remove',
                title: 'Remove this planned chapter',
                search: 'chapter plan remove delete',
                group: 'Here',
                scope: 'page',
                run: () => void latest.current.removePlan(),
              },
            ]
          : [
              {
                id: 'chapter.confirm',
                title: 'Confirm this chapter',
                search: 'chapter confirm save title dates',
                group: 'Here',
                scope: 'page',
                // C starts an account jump, so this takes ⇧C.
                bindings: [{ sequence: ['C'], label: '⇧C' }],
                run: () => latest.current.confirmEdits(),
              },
              {
                id: 'chapter.dismiss',
                title: 'Not a chapter',
                search: 'chapter dismiss not wrong',
                group: 'Here',
                scope: 'page',
                run: () => void latest.current.choose({ status: 'dismissed' }),
              },
              {
                id: 'chapter.edit',
                title: 'Edit this chapter’s title and dates',
                search: 'chapter edit rename title dates began ended',
                group: 'Here',
                scope: 'page',
                bindings: [{ sequence: ['e'], label: 'E' }],
                run: () => setEditing((current) => !current),
              },
            ],
      [planned],
    ),
  )

  return (
    <Card className="chapters-card chapter-detail">
      <SectionHeading
        title={chapter.title}
        detail={`${chapterKinds[chapter.kind].label} · ${chapterSpan(chapter)}`}
        action={
          planned ? (
            <Button
              size="compact"
              variant="ghost"
              icon={Trash2}
              data-command="chapter.remove"
              onClick={() => void removePlan()}
            >
              Remove plan
            </Button>
          ) : (
            <div className="chapter-actions">
              <span className={`tier-label tier-${chapter.confirmed ? 'observed' : 'suggested'}`}>
                {standing(chapter)}
              </span>
              {chapter.confirmed ? null : (
                <>
                  <Button
                    size="compact"
                    variant="primary"
                    icon={Check}
                    data-command="chapter.confirm"
                    onClick={confirmEdits}
                  >
                    Confirm
                  </Button>
                  <Button
                    size="compact"
                    icon={X}
                    data-command="chapter.dismiss"
                    onClick={() => void choose({ status: 'dismissed' })}
                  >
                    Not a chapter
                  </Button>
                </>
              )}
              <Button
                size="compact"
                variant="ghost"
                aria-pressed={editing}
                data-command="chapter.edit"
                onClick={() => setEditing((current) => !current)}
              >
                Edit
              </Button>
            </div>
          )
        }
      />
      {editing && !planned ? (
        <form
          className="chapter-edit chapter-edit-inline"
          onSubmit={(event) => {
            event.preventDefault()
            confirmEdits()
          }}
        >
          <label>
            <span>Title</span>
            <input
              value={title}
              maxLength={80}
              onChange={(event) => setTitle(event.target.value)}
            />
          </label>
          <label>
            <span>Began</span>
            <input type="date" value={start} onChange={(event) => setStart(event.target.value)} />
          </label>
          <label>
            <span>Ended</span>
            <input type="date" value={end} onChange={(event) => setEnd(event.target.value)} />
          </label>
          <div className="chapter-actions">
            <Button type="submit" size="compact" variant="primary" icon={Check}>
              {chapter.confirmed ? 'Save' : 'Save and confirm'}
            </Button>
            <Button type="button" size="compact" onClick={() => setEditing(false)}>
              Cancel
            </Button>
          </div>
        </form>
      ) : null}
      {chapter.effects.length ? (
        <div className="chapter-effects">
          {chapter.effects.map((effect) => (
            <Metric
              key={effect.measure}
              label={`Monthly ${effect.measure}, 3 months before → after`}
              value={
                <span className="sensitive-value">
                  {formatCurrency(effect.before)} → {formatCurrency(effect.after)}
                </span>
              }
            />
          ))}
        </div>
      ) : null}
      {chapter.plannedCosts?.length ? (
        <section>
          <h3>Expected monthly costs</h3>
          <ul className="chapter-facts">
            {chapter.plannedCosts.map((cost) => (
              <li key={cost.label}>
                <span>{cost.label}</span>
                <span className="sensitive-value">{formatCurrency(-cost.monthly)}</span>
              </li>
            ))}
          </ul>
          <p className="transaction-edit-note">Cash ahead and retirement coverage count these.</p>
        </section>
      ) : null}
      {chapter.anchors.length ? (
        <section>
          <h3>Why Brief sees it</h3>
          <ul className="chapter-facts">
            {chapter.anchors.map((anchor) => (
              <li key={anchor.says} data-keyboard-row>
                <span>{anchor.says}</span>
                {anchor.evidence[0] ? (
                  <button
                    type="button"
                    className="object-link"
                    onClick={() => showObject(anchor.evidence[0])}
                  >
                    Source
                  </button>
                ) : null}
              </li>
            ))}
          </ul>
        </section>
      ) : null}
      {chapter.members.length ? (
        <section>
          <h3>Belongs to it</h3>
          <ul className="chapter-facts">
            {chapter.members.map((member) => (
              <li key={member.id} data-keyboard-row>
                <button type="button" className="object-link" onClick={() => showObject(member.id)}>
                  {member.says}
                </button>
                <span className={`tier-label tier-${member.tier}`}>
                  {tierLabels[member.tier].label}
                </span>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
      {chapter.programs.length ? (
        <section>
          <h3>Rules it enables</h3>
          <ul className="chapter-facts">
            {chapter.programs.map((id) => (
              <li key={id} data-keyboard-row>
                <button
                  type="button"
                  className="object-link"
                  onClick={() => showObject(`program:${id}`)}
                >
                  {programs.find((program) => program.id === id)?.name ?? id}
                </button>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
      {chapter.checklist.length ? (
        <section>
          <h3>When it ends</h3>
          <ul className="chapter-checklist">
            {chapter.checklist.map((item) => (
              <li key={item}>{item}</li>
            ))}
          </ul>
        </section>
      ) : null}
      {error ? (
        <p role="alert" className="transaction-edit-note">
          {error}
        </p>
      ) : null}
    </Card>
  )
}

type Cost = { label: string; monthly: string }

/** A chapter you plan ahead, such as moving out, with what it will cost each month. */
function PlanForm({ data, onDone }: { data: FinanceSnapshot; onDone: () => void }) {
  const { updateSettings } = useFinanceEdits()
  const navigate = useNavigate({ from: '/insights' })
  const today = data.insights?.today ?? data.updatedAt.slice(0, 10)
  const [title, setTitle] = useState('')
  const [start, setStart] = useState(today)
  const [end, setEnd] = useState('')
  const [costs, setCosts] = useState<Cost[]>([{ label: 'Rent', monthly: '' }])
  const [error, setError] = useState<string>()

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    const plan: PlannedChapter = {
      id: `planned:${crypto.randomUUID()}`,
      title: title.trim(),
      start,
      ...(end ? { end } : {}),
      costs: costs
        .filter((cost) => cost.label.trim() && Number(cost.monthly) > 0)
        .map((cost) => ({ label: cost.label.trim(), monthly: Number(cost.monthly) })),
    }
    if (!plan.title) return setError('Name the chapter')
    try {
      await updateSettings((current) => ({
        ...current,
        plannedChapters: [...(current.plannedChapters ?? []), plan],
      }))
      onDone()
      await navigate({ search: chapterSearch(plan) })
    } catch (caught) {
      setError(errorMessage(caught))
    }
  }

  const form = useRef<HTMLFormElement>(null)
  useRegisterActions(
    useMemo<AppAction[]>(
      () => [
        {
          id: 'chapter.plan.save',
          title: 'Save the planned chapter',
          search: 'plan chapter save',
          group: 'Here',
          scope: 'page',
          run: () => form.current?.requestSubmit(),
        },
        {
          id: 'chapter.plan.cost',
          title: 'Add a monthly cost',
          search: 'plan chapter cost rent add',
          group: 'Here',
          scope: 'page',
          run: () => setCosts((current) => [...current, { label: '', monthly: '' }]),
        },
        {
          id: 'chapter.plan.cancel',
          title: 'Cancel planning',
          search: 'plan chapter cancel',
          group: 'Here',
          scope: 'page',
          run: onDone,
        },
      ],
      [onDone],
    ),
  )

  return (
    <Card className="chapters-card chapter-detail">
      <SectionHeading
        title="Plan a chapter"
        detail="Its costs join cash ahead and retirement coverage."
      />
      <form ref={form} className="chapter-edit" onSubmit={(event) => void submit(event)}>
        <label>
          <span>Title</span>
          <input
            autoFocus
            value={title}
            maxLength={80}
            placeholder="Moving out"
            onChange={(event) => setTitle(event.target.value)}
          />
        </label>
        <label>
          <span>Begins</span>
          <input type="date" value={start} onChange={(event) => setStart(event.target.value)} />
        </label>
        <label>
          <span>Ends</span>
          <input type="date" value={end} onChange={(event) => setEnd(event.target.value)} />
        </label>
        <fieldset className="chapter-costs">
          <legend>Monthly costs</legend>
          {costs.map((cost, index) => (
            <div key={index} className="chapter-cost" data-keyboard-row>
              <input
                aria-label="Cost"
                value={cost.label}
                maxLength={80}
                onChange={(event) =>
                  setCosts(
                    costs.map((item, at) =>
                      at === index ? { ...item, label: event.target.value } : item,
                    ),
                  )
                }
              />
              <input
                aria-label="Monthly amount"
                inputMode="decimal"
                value={cost.monthly}
                placeholder="0"
                onChange={(event) =>
                  setCosts(
                    costs.map((item, at) =>
                      at === index ? { ...item, monthly: event.target.value } : item,
                    ),
                  )
                }
              />
              <Button
                type="button"
                size="icon-compact"
                variant="ghost"
                icon={X}
                onClick={() => setCosts(costs.filter((_, at) => at !== index))}
              >
                Remove cost
              </Button>
            </div>
          ))}
          <Button
            type="button"
            size="compact"
            variant="ghost"
            icon={Plus}
            data-command="chapter.plan.cost"
            onClick={() => setCosts([...costs, { label: '', monthly: '' }])}
          >
            Add a cost
          </Button>
        </fieldset>
        <div className="chapter-actions">
          <Button
            type="submit"
            size="compact"
            variant="primary"
            icon={Check}
            data-command="chapter.plan.save"
          >
            Save plan
          </Button>
          <Button type="button" size="compact" data-command="chapter.plan.cancel" onClick={onDone}>
            Cancel
          </Button>
        </div>
        {error ? (
          <p role="alert" className="transaction-edit-note">
            {error}
          </p>
        ) : null}
      </form>
    </Card>
  )
}
