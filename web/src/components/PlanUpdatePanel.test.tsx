import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { PlanUpdatePanel } from './PlanUpdatePanel'

describe('PlanUpdatePanel', () => {
    it('renders explanation and steps', () => {
        render(
            <PlanUpdatePanel
                planUpdate={{
                    explanation: 'Doing the thing',
                    plan: [
                        { step: 'Step one', status: 'pending' },
                        { step: 'Step two', status: 'in_progress' },
                        { step: 'Step three', status: 'completed' },
                    ],
                }}
            />
        )

        expect(screen.getByText('Plan')).toBeInTheDocument()
        expect(screen.getByText('Doing the thing')).toBeInTheDocument()
        expect(screen.getByText('Step one')).toBeInTheDocument()
        expect(screen.getByText('Step two')).toBeInTheDocument()
        expect(screen.getByText('Step three')).toBeInTheDocument()
    })

    it('renders nothing when missing', () => {
        render(<PlanUpdatePanel planUpdate={null} />)
        expect(screen.queryByText('Plan')).toBeNull()
    })
})

