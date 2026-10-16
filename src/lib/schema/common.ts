/** Pieces every contract shares. */
import { z } from 'zod'

export const isoDate = z.iso.date()
