import type { EvidenceGraph } from '../../lib/bindings/EvidenceGraph'

const proof = {
  program: 'example-bonus',
  name: 'Example engagement bonus',
  source: { url: 'https://bank.example/terms', title: 'Example terms' },
  reviewed: '2026-10-01',
  steps: [
    {
      says: 'An Example Checking account',
      evidence: ['account:plaid:checking'],
      detail: 'Example Checking',
      met: true,
    },
  ],
  unchecked: [],
  notes: [],
}

/** A synthetic evidence graph: one Explained and one Suggested conclusion, a job and a home. */
export const evidenceFixture: EvidenceGraph = {
  explanations: [
    {
      id: 'example-bonus:abc',
      program: 'example-bonus',
      title: 'Card fee offset',
      says: 'The $100 bonus offsets the card’s $100 fee',
      tier: 'explained',
      subject: 'transaction:bonus',
      predicate: 'offsets',
      object: 'transaction:fee',
      from: '2026-07-15',
      proof,
    },
    {
      id: 'example-waiver:account:plaid:checking:2026-05-01',
      program: 'example-waiver',
      title: 'Monthly fee waived',
      says: 'The $10 monthly fee is waived',
      tier: 'suggested',
      subject: 'account:plaid:checking',
      predicate: 'feeWaived',
      from: '2026-05-01',
      to: '2026-08-31',
      proof: { ...proof, program: 'example-waiver', unchecked: ['Paperless statements'] },
    },
  ],
  alerts: [
    {
      id: 'example-waiver:watch:2026-09',
      program: 'example-waiver',
      kind: 'watch',
      says: 'Deposits were short in September 2026.',
      date: '2026-09-30',
      subject: 'account:plaid:checking',
    },
  ],
  chapters: [
    {
      id: 'home:ca:2026-04',
      kind: 'home',
      title: 'Lived in California',
      start: '2026-04-01',
      precision: 'month',
      tier: 'suggested',
      confirmed: false,
      state: 'CA',
      anchors: [{ says: '30 of 30 in-person purchases were in CA', evidence: ['transaction:a'] }],
      members: [],
      programs: [],
      effects: [],
      checklist: [],
    },
    {
      id: 'work:example',
      kind: 'work',
      title: 'Example Employer',
      start: '2026-04-20',
      precision: 'day',
      tier: 'suggested',
      confirmed: false,
      anchors: [{ says: 'Pay statements from Example Employer', evidence: ['document:a'] }],
      members: [{ id: 'account:plaid:checking', tier: 'suggested', says: 'Opened near the start' }],
      programs: ['example-waiver'],
      effects: [{ measure: 'spending', before: -2_000, after: -3_000 }],
      checklist: ['Check the final paycheck.'],
    },
  ],
  now: { work: 'work:example', workTitle: 'Example Employer', home: 'California', month: 6 },
  residency: [
    {
      year: 2026,
      state: 'CA',
      from: '2026-04-01',
      to: '2026-10-17',
      days: 200,
      tier: 'suggested',
      chapter: 'home:ca:2026-04',
    },
  ],
  programs: [
    {
      id: 'example-waiver',
      name: 'Example fee waiver',
      organization: 'Example Bank',
      summary: 'A waiver.',
      source: { url: 'https://bank.example/waiver', title: 'Waiver terms' },
      reviewed: '2026-10-01',
      stale: false,
      approved: false,
    },
  ],
}
