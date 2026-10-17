import { invoke } from '@tauri-apps/api/core'

import { isTauri } from './api'
import type { Chapter } from './bindings/Chapter'
import type { ChapterKind } from './bindings/ChapterKind'
import type { EvidenceGraph } from './bindings/EvidenceGraph'
import type { Explanation } from './bindings/Explanation'
import type { FactTier } from './bindings/FactTier'
import type { Program } from './bindings/Program'
import type { ProgramDraft } from './bindings/ProgramDraft'
import type { ResearchEntry } from './bindings/ResearchEntry'
import type { ResearchState } from './bindings/ResearchState'
import type { FinanceSnapshot } from './schema'

/**
 * The evidence graph as the renderer phrases it: rule conclusions with their tiers and proofs,
 * life chapters, and opt-in research. Rust concludes and detects everything; this names it.
 */
export type {
  Chapter,
  ChapterKind,
  EvidenceGraph,
  Explanation,
  FactTier,
  Program,
  ProgramDraft,
  ResearchEntry,
  ResearchState,
}

/** How each tier reads beside the statement it qualifies; never by color alone. */
export const tierLabels: Record<FactTier, { label: string; detail: string }> = {
  observed: { label: 'Observed', detail: 'A source states it, or you set it' },
  derived: { label: 'Derived', detail: 'Arithmetic over observed facts' },
  explained: { label: 'Explained', detail: 'A cited rule whose every condition is met' },
  suggested: { label: 'Possible', detail: 'A pattern no rule settles; yours to confirm' },
}

export const chapterKinds: Record<ChapterKind, { label: string; lane: string }> = {
  work: { label: 'Work', lane: 'Work' },
  home: { label: 'Home', lane: 'Home' },
  milestone: { label: 'Milestone', lane: 'Accounts and purchases' },
  shift: { label: 'Shift', lane: 'Income and spending' },
  planned: { label: 'Planned', lane: 'Planned' },
}

export const emptyEvidence: EvidenceGraph = {
  explanations: [],
  alerts: [],
  chapters: [],
  now: {},
  residency: [],
  programs: [],
}

export const evidenceOf = (data: FinanceSnapshot | undefined): EvidenceGraph =>
  data?.insights?.evidence ?? emptyEvidence

/** A chapter's span as words: "2026-04-20 to 2026-07-31", or "Since 2026-04-20". */
export const chapterSpan = (chapter: Pick<Chapter, 'start' | 'end'>) =>
  chapter.end ? `${chapter.start} to ${chapter.end}` : `Since ${chapter.start}`

/**
 * Home's one line: "Morgan Stanley · California · month 6". Empty when no open chapter
 * names where you work or live.
 */
export function nowLine(evidence: EvidenceGraph): string {
  const { workTitle, home, month } = evidence.now
  return [workTitle, home, workTitle && month ? `month ${month}` : undefined]
    .filter(Boolean)
    .join(' · ')
}

/** An explanation's dates as words. */
export const explanationSpan = (explanation: Pick<Explanation, 'from' | 'to'>) =>
  explanation.from && explanation.to
    ? `${explanation.from} to ${explanation.to}`
    : explanation.from
      ? `From ${explanation.from}`
      : undefined

const noResearch: ResearchState = {
  enabled: false,
  connected: false,
  gaps: [],
  log: [],
  sources: [],
}

/** Research's gaps, log, and source checks; the preview has none. */
export async function getResearch(): Promise<ResearchState> {
  if (!isTauri()) return noResearch
  return invoke<ResearchState>('get_research')
}

/** Searches for a gap's terms; the query is logged before it runs. */
export async function researchGap(gapId: string): Promise<ResearchEntry> {
  return invoke<ResearchEntry>('research_gap', { gapId })
}

/** Reads rule sources due for their monthly check, or all of them. */
export async function recheckProgramSources(all: boolean): Promise<ResearchEntry[]> {
  return invoke<ResearchEntry[]>('recheck_program_sources', { all })
}

export async function acknowledgeProgramSource(url: string): Promise<void> {
  await invoke('acknowledge_program_source', { url })
}

/** The rule a draft describes, checked by Rust; nothing is saved until you approve it. */
export async function draftProgram(draft: ProgramDraft): Promise<Program> {
  if (!isTauri()) throw new Error('Research is available in the Brief app')
  return invoke<Program>('draft_program', { draft })
}

/** Whether a source's monthly check is due: never read, or read 30 or more days ago. */
export function sourceDue(checkedAt: string | undefined, now: Date): boolean {
  if (!checkedAt) return true
  return now.getTime() - new Date(checkedAt).getTime() >= 30 * 24 * 60 * 60 * 1000
}
