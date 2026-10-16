// React's act() warns unless the environment says it supports it.
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true })
// jsdom lays nothing out, so scrolling the window does nothing instead of logging an error.
if (typeof window !== 'undefined') window.scrollTo = () => undefined
