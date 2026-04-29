---
name: define-mission-skills
description: Define specialized skills for Mission workers. Use when creating skill files for Mission workers that will execute specific tasks with clear responsibilities, inputs, outputs, and handoff requirements.
metadata:
  short-description: Define Mission worker skills
---

# Define Mission Skills

Create specialized skill files for Mission workers based on the planning phase outputs.

## When to Use

Use this skill during the Worker Definition phase or when creating new Mission worker skills.

## Skill Definition Template

Each Mission worker skill should follow this structure:

### Frontmatter (Required)

```yaml
---
name: <worker-skill-name>
description: <clear description of when this skill should be used>
metadata:
  short-description: <brief summary>
---
```

**Guidelines:**
- `name`: Use lowercase, hyphenated format (e.g., `frontend-implementation`)
- `description`: Be specific about triggers and use cases. Include what the worker does and when to use it.
- `short-description`: Under 50 characters for UI display

### Skill Body (Required)

Structure the skill body with these sections:

#### 1. Responsibility

Clearly define what this worker is responsible for:
- What tasks does it perform?
- What decisions does it make?
- What does it NOT do (boundaries)?

#### 2. Input

Specify what information this worker receives:
- What context is passed in?
- What artifacts does it work with?
- What handoff data does it consume?

#### 3. Output

Specify what this worker produces:
- What artifacts does it create?
- What handoff data does it generate?
- What format does it use for reporting?

#### 4. Handoff Format

Define the handoff JSON structure this worker generates:

```json
{
  "worker": "<worker-name>",
  "salientSummary": "<1-2 sentence summary>",
  "whatWasImplemented": "<list of key changes>",
  "verification": {
    "codeReview": "<review findings>",
    "userTesting": "<test results>",
    "remainingWork": "<what's left>"
  },
  "nextSteps": "<recommendations for next worker>"
}
```

#### 5. Integration Points

Specify how this worker integrates with:
- Which workers come before this one (dependencies)?
- Which workers come after this one (consumers)?
- What shared state or files does it use?

## Skill Creation Workflow

1. **Identify Need**: Based on Worker Definition phase, determine required workers
2. **Define Responsibility**: Clearly scope what the worker does
3. **Specify I/O**: Define inputs, outputs, and handoff format
4. **Create Skill File**: Write the skill following the template
5. **Validate**: Test skill on realistic tasks
6. **Iterate**: Refine based on usage

## Common Worker Types

### Feature Worker
Implements a specific feature or capability:
- **Input**: Design spec, requirements
- **Output**: Implementation code, tests
- **Handoff**: Feature completion report

### Validation Worker
Verifies implementation quality:
- **Input**: Code to review, test results
- **Output**: Validation report, issues found
- **Handoff**: Verification findings

### Integration Worker
Connects components or systems:
- **Input**: Component implementations
- **Output**: Integration code, integration tests
- **Handoff**: Integration status report

### Documentation Worker
Creates or updates documentation:
- **Input**: Implementation details
- **Output**: Documentation files, updates
- **Handoff**: Documentation completion report

## Best Practices

- **Clear Boundaries**: Each worker should have a single, well-defined responsibility
- **Composable**: Workers should be able to work independently
- **Standardized Handoff**: All workers use the same handoff format
- **Idempotent**: Workers should be able to rerun safely if needed
- **Observable**: Workers should produce clear output and status
