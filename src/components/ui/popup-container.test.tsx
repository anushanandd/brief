// @vitest-environment jsdom

import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import { PopupContainerContext, usePopupPortal } from './popup-container'

function Probe() {
  return <>{usePopupPortal().positionMethod}</>
}

it('positions popups inside a container against the viewport', () => {
  expect(renderToStaticMarkup(<Probe />)).toBe('absolute')
  expect(
    renderToStaticMarkup(
      <PopupContainerContext.Provider value={document.createElement('div')}>
        <Probe />
      </PopupContainerContext.Provider>,
    ),
  ).toBe('fixed')
})
