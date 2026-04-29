---
name: mission-worker-base
description: Base instructions for all Mission workers. Provides common workflow, handoff format, and integration patterns. Use when executing as a Mission worker to ensure consistent behavior and proper handoff generation.
metadata:
  short-description: Base Mission worker instructions
---

# Mission Worker Base

Common instructions and workflow for all Mission workers.

## Worker Workflow

When assigned a task as a Mission worker:

1. **Understand Context**: Read the handoff from previous workers (if any)
2. **Execute Responsibility**: Complete your assigned tasks
3. **Generate Handoff**: Create standardized handoff JSON
4. **Report Status**: Clearly communicate completion status

## Handoff Format

All Mission workers must generate a handoff JSON file with this structure:

```json
{
  "worker": "<worker-name>",
  "timestamp": "<ISO-8601 timestamp>",
  "salientSummary": "<Concise 1-2 sentence summary of what was done>",
  "whatWasImplemented": [
    "<Key change 1>",
    "<Key change 2>",
    "..."
  ],
  "filesModified": [
    {
      "path": "<relative/path/to/file>",
      "changeSummary": "<brief description of changes>"
    }
  ],
  "filesCreated": [
    {
      "path": "<relative/path/to/file>",
      "purpose": "<what this file is for>"
    }
  ],
  "verification": {
    "codeReview": {
      "status": "<passed|failed|partial|skipped>",
      "findings": "<summary of code review findings>",
      "issuesFound": <number>,
      "issuesFixed": <number>
    },
    "userTesting": {
      "status": "<passed|failed|partial|skipped>",
      "results": "<summary of user testing results>",
      "testCasesExecuted": <number>,
      "testCasesPassed": <number>
    },
    "remainingWork": "<description of any remaining work or concerns>"
  },
  "nextSteps": "<guidance for the next worker or for final review>",
  "blockers": [
    "<any blockers preventing progress, empty array if none>"
  ]
}
```

## Handoff Best Practices

### salientSummary
- Keep it to 1-2 sentences maximum
- Focus on the most important outcome
- Avoid implementation details
- Make it useful for the next worker

### whatWasImplemented
- List 3-7 key changes
- Focus on high-level outcomes, not implementation details
- Use clear, action-oriented language
- Order by importance

### filesModified / filesCreated
- Use relative paths from workspace root
- Keep change summaries concise (1 line each)
- Include all files that were touched
- Group related changes when appropriate

### verification
- **Be honest about status**: Don't claim `passed` if verification was incomplete
- **Document issues found**: Even if you fixed them, note what you found
- **Highlight remaining work**: What would you do next with more time?
- **Provide evidence**: Reference tests, reviews, or validation performed

### nextSteps
- Provide clear guidance for the next worker
- Highlight any risks or areas needing attention
- Suggest what to focus on next
- Flag any decisions that need revisiting

## Worker Behavior Guidelines

### Clarity
- Make your work easy to review and understand
- Document non-obvious decisions
- Explain why you did something, not just what

### Thoroughness
- Complete your assigned responsibility fully
- Don't leave half-finished work
- If blocked, clearly document the blocker and suggest workarounds

### Quality
- Write clean, maintainable code
- Follow project conventions and style guidelines
- Add tests for new functionality
- Consider edge cases and error handling

### Communication
- Report status clearly and frequently
- Highlight blockers or risks immediately
- Ask for clarification when requirements are ambiguous
- Confirm understanding before starting work

## Error Handling

When encountering errors or blockers:

1. **Document the Issue**: Clearly describe what went wrong
2. **Attempt Recovery**: Try to resolve or work around the issue
3. **Report Status**: Update handoff with current status
4. **Suggest Next Steps**: Provide recommendations for how to proceed

## Common Scenarios

### Task Completed Successfully
- Set verification status to `passed` if you verified your work
- Include evidence of verification (tests run, code reviewed)
- Provide clear next steps for follow-up work

### Task Partially Completed
- Set verification status to `partial`
- Document what was completed and what remains
- Explain why the task wasn't fully completed
- Estimate what's needed to finish

### Task Blocked
- Document blockers in the `blockers` array
- Set verification status to `failed` or `partial` as appropriate
- Provide suggestions for unblocking
- Preserve any work that was completed

### Task Not Started
- Explain why the task wasn't started (e.g., waiting on dependency)
- Document what information or work is needed
- Suggest how to proceed

## Handoff File Location

Save handoff JSON to:
```
.mission/handoffs/<worker-name>-<timestamp>.json
```

Use ISO-8601 format for timestamp (e.g., `2024-04-28T12-34-56Z`).
