---
name: scrutiny-validator
description: Perform code review validation for Mission work. Use when verifying implementation quality through code inspection, checking for correctness, safety, maintainability, and adherence to project conventions.
metadata:
  short-description: Code review validation
---

# Scrutiny Validator

Perform code review validation to ensure Mission work meets quality standards.

## Validation Scope

Scrutiny Validator focuses on code quality through static analysis and review:

### Correctness
- Logic correctness and edge cases
- Error handling and error propagation
- Race conditions and concurrency issues
- Resource management (memory, file handles, connections)

### Safety & Security
- Input validation and sanitization
- SQL injection, XSS, command injection risks
- Secret/credential handling
- Access control and authorization

### Maintainability
- Code clarity and readability
- Naming conventions
- Function/module cohesion
- Appropriate abstraction levels

### Project Conventions
- Adherence to project style guidelines
- Consistent patterns with existing code
- Proper use of project utilities and libraries
- Integration with existing systems

## Validation Process

1. **Read Handoff**: Review the worker's handoff JSON to understand what was implemented
2. **Examine Changes**: Review all modified and created files
3. **Check Quality**: Verify correctness, safety, and maintainability
4. **Document Findings**: Record issues found and severity
5. **Generate Report**: Produce validation report

## Severity Levels

### Critical
- Security vulnerabilities
- Data corruption or loss risks
- Crashes or panics in normal operation
- Must fix before proceeding

### High
- Significant bugs or logic errors
- Poor error handling that could cause failures
- Major maintainability concerns
- Should fix before proceeding

### Medium
- Minor bugs or edge cases
- Style inconsistencies
- Moderate maintainability issues
- Consider fixing

### Low
- Nitpicks or suggestions
- Minor style issues
- Nice-to-have improvements
- Optional improvements

## Review Checklist

For each implementation:

### Logic & Correctness
- [ ] Algorithms are correct for the problem
- [ ] Edge cases are handled
- [ ] Error conditions are properly handled
- [ ] No obvious bugs or logic errors

### Safety & Security
- [ ] User input is validated
- [ ] No injection vulnerabilities
- [ ] Secrets are not hardcoded
- [ ] Resource cleanup is handled (RAII, defer, etc.)

### Maintainability
- [ ] Code is readable and clear
- [ ] Names are descriptive and appropriate
- [ ] Functions are focused and not too long
- [ ] Complexity is reasonable

### Project Integration
- [ ] Follows project conventions
- [ ] Uses existing utilities where appropriate
- [ ] Integrates well with existing code
- [ ] Doesn't duplicate existing functionality

### Testing
- [ ] Tests cover key functionality
- [ ] Tests are clear and maintainable
- [ ] Edge cases are tested
- [ ] Failure modes are tested

## Validation Report Format

Generate a validation report with this structure:

```markdown
# Scrutiny Validation Report

**Worker:** <worker-name>
**Timestamp:** <ISO-8601 timestamp>
**Reviewer:** Scrutiny Validator

## Summary

**Overall Status:** <PASSED|FAILED|PARTIAL>
**Issues Found:** <total>
**Critical:** <count>, **High:** <count>, **Medium:** <count>, **Low:** <count>

## Critical Issues

<If none: No critical issues found.>

<For each critical issue:>
### <Issue Title>

- **Location:** `<file:path>`
- **Problem:** <description of the issue>
- **Impact:** <why this is critical>
- **Recommendation:** <how to fix>

## High Issues

<Similar format as Critical Issues>

## Medium Issues

<Similar format as Critical Issues>

## Low Issues

<Similar format as Critical Issues>

## Positive Findings

<Highlight good practices, well-written code, etc.>

## Recommendations

<General recommendations for improvement>

## Conclusion

<Overall assessment and whether to proceed>
```

## Decision Criteria

### PASSED
- No critical or high issues
- Medium issues are documented but acceptable
- Code quality meets project standards

### FAILED
- One or more critical issues
- Multiple high issues that suggest systemic problems
- Code quality is below acceptable standards

### PARTIAL
- No critical issues
- Some high issues that should be addressed
- Code is acceptable but has clear improvement opportunities

## Best Practices

1. **Be Constructive**: Focus on helping improve the code, not just criticizing
2. **Explain Why**: Don't just say what's wrong, explain the impact
3. **Provide Examples**: Show how to fix issues when helpful
4. **Prioritize**: Focus on issues that matter most
5. **Be Thorough**: Review all changes, not just the main files
6. **Stay Objective**: Base feedback on best practices and project standards
