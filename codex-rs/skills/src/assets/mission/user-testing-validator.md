---
name: user-testing-validator
description: Perform user testing validation for Mission work. Use when verifying implementation works correctly from a user perspective, testing actual workflows, edge cases, and integration scenarios.
metadata:
  short-description: User testing validation
---

# User Testing Validator

Perform user testing validation to ensure Mission work meets user needs and works correctly in practice.

## Validation Scope

User Testing Validator focuses on functional correctness and user experience:

### Functional Testing
- Core workflows work as intended
- Features behave according to requirements
- Edge cases are handled properly
- Error conditions are manageable

### Integration Testing
- Components integrate correctly
- Data flows between systems properly
- APIs work as expected
- Side effects are correct

### User Experience
- Interface is intuitive and clear
- Error messages are helpful
- Performance is acceptable
- Workflow is smooth

### Documentation
- User-facing documentation is accurate
- Examples work correctly
- Setup instructions are complete
- Troubleshooting covers common issues

## Validation Process

1. **Understand Requirements**: Review what the worker was supposed to implement
2. **Plan Tests**: Identify test cases covering normal, edge, and error cases
3. **Execute Tests**: Run through test scenarios manually or with automated tests
4. **Document Results**: Record what worked, what didn't, and any issues
5. **Generate Report**: Produce validation report with findings

## Test Planning

### Test Categories

**Smoke Tests**
- Basic functionality works
- Can complete the primary workflow
- No crashes or fatal errors

**Normal Cases**
- Typical user workflows
- Common use cases
- Standard inputs and configurations

**Edge Cases**
- Boundary conditions (empty, single item, max values)
- Unusual but valid inputs
- Less common workflows

**Error Cases**
- Invalid inputs
- Network failures
- Resource constraints
- Permission issues

**Integration Cases**
- Interactions with other components
- Data persistence and retrieval
- Side effects and callbacks

## Test Execution

For each test case:

1. **Setup**: Prepare the test environment and data
2. **Execute**: Perform the test steps
3. **Observe**: Record what happens
4. **Verify**: Check if results match expectations
5. **Document**: Note any issues or unexpected behavior

## Validation Report Format

Generate a validation report with this structure:

```markdown
# User Testing Validation Report

**Worker:** <worker-name>
**Timestamp:** <ISO-8601 timestamp>
**Tester:** User Testing Validator

## Summary

**Overall Status:** <PASSED|FAILED|PARTIAL>
**Tests Executed:** <total>
**Tests Passed:** <count>
**Tests Failed:** <count>
**Tests Skipped:** <count>

## Test Results

### Smoke Tests

| Test | Status | Notes |
|------|--------|-------|
| <test name> | <PASS|FAIL|SKIP> | <notes> |

### Normal Cases

| Test | Status | Notes |
|------|--------|-------|
| <test name> | <PASS|FAIL|SKIP> | <notes> |

### Edge Cases

| Test | Status | Notes |
|------|--------|-------|
| <test name> | <PASS|FAIL|SKIP> | <notes> |

### Error Cases

| Test | Status | Notes |
|------|--------|-------|
| <test name> | <PASS|FAIL|SKIP> | <notes> |

### Integration Cases

| Test | Status | Notes |
|------|--------|-------|
| <test name> | <PASS|FAIL|SKIP> | <notes> |

## Failed Tests

<For each failed test:>
### <Test Name>

- **Category:** <Smoke|Normal|Edge|Error|Integration>
- **Expected:** <what should have happened>
- **Actual:** <what actually happened>
- **Impact:** <why this matters>
- **Recommendation:** <how to fix>

## Issues Found

### Critical Issues

<Issues that block the workflow or cause data loss/corruption>

### High Issues

<Issues that significantly impact usability or functionality>

### Medium Issues

<Issues that affect less common scenarios or have workarounds>

### Low Issues

<Minor issues, nice-to-have improvements>

## Positive Findings

<What worked well, good UX, smooth workflows>

## Recommendations

<Recommendations for improvement>

## Conclusion

<Overall assessment and whether to proceed>
```

## Decision Criteria

### PASSED
- All smoke tests pass
- At least 90% of normal cases pass
- No critical test failures
- User experience is acceptable

### FAILED
- Any smoke test fails
- More than 30% of normal cases fail
- Critical test failures
- User experience is poor

### PARTIAL
- All smoke tests pass
- 70-90% of normal cases pass
- Some high-severity issues but workarounds exist
- User experience is acceptable with some friction

## Test Case Examples

### Smoke Test Example
**Test:** Can start the application and see the main screen
**Steps:**
1. Run `codex mission start "test goal"`
2. Verify prompt appears
3. Enter intent phase input
4. Verify next phase prompt appears
**Expected:** Smooth progression through first phase
**Actual:** <result>

### Normal Case Example
**Test:** Complete full planning workflow
**Steps:**
1. Start mission with clear goal
2. Complete all 7 planning phases
3. Verify state is saved
4. Check status command shows correct phase
**Expected:** All phases complete, state persists
**Actual:** <result>

### Edge Case Example
**Test:** Empty input handling
**Steps:**
1. Start mission
2. Enter empty string for phase input
3. Verify system handles gracefully
**Expected:** Clear error message, prompt to re-enter
**Actual:** <result>

### Error Case Example
**Test:** Invalid workspace
**Steps:**
1. Try to start mission in directory without write permissions
2. Verify error handling
**Expected:** Clear error message explaining the issue
**Actual:** <result>

## Best Practices

1. **Think Like a User**: Test workflows as a user would, not just as designed
2. **Be Thorough**: Don't skip edge cases and error cases
3. **Document Everything**: Record test steps, expected results, and actual results
4. **Reproduce Issues**: Try to reproduce failures consistently
5. **Provide Context**: When reporting issues, include steps to reproduce
6. **Be Fair**: Don't fail for trivial issues that don't impact usability
