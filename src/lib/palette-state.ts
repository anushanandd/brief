import type { FilterChip, FilterStep } from './palette-filters'
import { paletteTabPrefix, paletteTabs, type PaletteMode, type PaletteTab } from './palette-ranking'

/** What the palette shows: its mode, the query, the result type, filters, and the selected row. */
export type PaletteState = {
  mode: PaletteMode
  query: string
  tabId: PaletteTab['id']
  chips: FilterChip[]
  /** The `/` filter picker's step while it is open: a field, then one of its values. */
  filterStep?: FilterStep
  /** A verb asking which object it acts on, by verb id; the query searches that kind. */
  verbStep?: string
  /** The highlighted row; empty highlights the first one available. */
  selected: string
}

export type PaletteEvent =
  | { type: 'select'; id: string }
  /** Typed text: a leading `/` opens the filter picker and a tab prefix jumps to its tab. */
  | { type: 'type'; query: string }
  /** Text completed for the highlighted row. */
  | { type: 'complete'; query: string }
  | { type: 'switchMode'; mode: PaletteMode; query?: string }
  | { type: 'showTab'; tabId: PaletteTab['id'] }
  | { type: 'stepTab'; offset: -1 | 1 }
  | { type: 'chooseField'; field: FilterStep }
  /** A verb that needs an object: the palette asks which one. */
  | { type: 'chooseVerb'; verb: string }
  | { type: 'addChip'; chip: FilterChip }
  | { type: 'removeChip'; field: FilterChip['field'] }
  /** Esc: leaves the filter picker and clears the query. */
  | { type: 'clear' }
  /** Backspace in an empty query: one picker step back, else the last filter. */
  | { type: 'back' }

export const initialPalette = (mode: PaletteMode): PaletteState => ({
  mode,
  query: '',
  tabId: 'all',
  chips: [],
  selected: '',
})

export function paletteReducer(state: PaletteState, event: PaletteEvent): PaletteState {
  switch (event.type) {
    case 'select':
      return { ...state, selected: event.id }
    case 'type': {
      if (state.mode === 'ask') return { ...state, query: event.query }
      // A verb's question searches its kind with whatever is typed.
      if (state.verbStep) return { ...state, query: event.query, selected: '' }
      if (!state.filterStep && !state.query && event.query === '/')
        return { ...state, filterStep: 'field', selected: '' }
      const jump = state.filterStep || state.query ? undefined : paletteTabPrefix(event.query)
      return jump
        ? { ...state, tabId: jump.tab.id, query: jump.term, selected: '' }
        : { ...state, query: event.query, selected: '' }
    }
    case 'complete':
      return { ...state, query: event.query, selected: '' }
    case 'switchMode':
      return {
        ...state,
        mode: event.mode,
        query: event.query ?? '',
        filterStep: undefined,
        verbStep: undefined,
        selected: '',
      }
    case 'showTab':
      return { ...state, tabId: event.tabId, selected: '' }
    case 'stepTab': {
      const index = paletteTabs.findIndex(({ id }) => id === state.tabId)
      const next = paletteTabs[(index + event.offset + paletteTabs.length) % paletteTabs.length]
      return { ...state, tabId: next.id, selected: '' }
    }
    case 'chooseField':
      return { ...state, filterStep: event.field, query: '', selected: '' }
    case 'chooseVerb':
      return { ...state, verbStep: event.verb, filterStep: undefined, query: '', selected: '' }
    case 'addChip':
      return {
        ...state,
        chips: [...state.chips.filter(({ field }) => field !== event.chip.field), event.chip],
        filterStep: undefined,
        query: '',
        selected: '',
        tabId: 'activity',
      }
    case 'removeChip':
      return { ...state, chips: state.chips.filter(({ field }) => field !== event.field) }
    case 'clear':
      // Esc leaves a verb's question with its query, then the question itself.
      if (state.verbStep && !state.query) return { ...state, verbStep: undefined, selected: '' }
      return { ...state, filterStep: undefined, query: '', selected: '' }
    case 'back':
      if (state.verbStep) return { ...state, verbStep: undefined, selected: '' }
      if (state.filterStep)
        return {
          ...state,
          filterStep: state.filterStep === 'field' ? undefined : 'field',
          selected: '',
        }
      return { ...state, chips: state.chips.slice(0, -1) }
    default: {
      const unhandled: never = event
      return unhandled
    }
  }
}
