import { displayCase } from './format'
import type { BriefDocument, FinanceSnapshot } from './schema'

export const documentKindNames: Record<BriefDocument['kind'], string> = {
  payslip: 'Pay statement',
  taxForm: 'Tax form',
  taxReturn: 'Tax return',
  statement: 'Statement',
  export: 'Activity export',
  other: 'Document',
}

/** What a document is and who issued it, such as "W-2 · Example Inc". */
export function documentTitle(document: BriefDocument) {
  const what = document.form ?? documentKindNames[document.kind]
  return document.issuer ? `${what} · ${displayCase(document.issuer)}` : what
}

/** The documents that state a transaction, as Rust matched them. */
export function transactionDocuments(
  data: Pick<FinanceSnapshot, 'documents' | 'insights'>,
  transactionId: string,
) {
  return (data.insights?.documents.evidence ?? []).flatMap((evidence) => {
    if (evidence.transactionId !== transactionId) return []
    const document = data.documents?.find((item) => item.id === evidence.documentId)
    return document ? [{ kind: evidence.kind, document }] : []
  })
}
