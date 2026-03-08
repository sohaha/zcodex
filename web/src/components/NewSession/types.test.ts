import { describe, expect, it } from 'vitest'
import { MODEL_OPTIONS } from './types'

describe('MODEL_OPTIONS', () => {
    it('includes gpt-5.2-codex for codex', () => {
        expect(MODEL_OPTIONS.codex.some((opt) => opt.value === 'gpt-5.2-codex')).toBe(true)
    })
})

