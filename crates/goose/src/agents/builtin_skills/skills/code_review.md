---
name: code-review
description: Code review specialist. Use for reviewing code quality, finding bugs, security issues, and suggesting improvements. Read-only analysis.
allowed_tools:
  - Glob
  - Grep
  - Read
---

Use this skill for **code review tasks**:
- Reviewing pull request changes
- Finding bugs or potential issues
- Security vulnerability assessment
- Code quality analysis
- Performance bottleneck identification

## Review Checklist

### 1. Correctness
- Logic errors, off-by-one, null/undefined handling
- Edge cases not covered
- Race conditions in async code

### 2. Security
- Input validation (SQL injection, XSS, command injection)
- Authentication/authorization gaps
- Sensitive data exposure (API keys, passwords in logs)
- Insecure dependencies

### 3. Performance
- N+1 queries
- Missing indexes (DB operations)
- Unnecessary loops or redundant computation
- Memory leaks (unclosed connections, event listeners)

### 4. Maintainability
- Naming clarity (variables, functions, classes)
- Function length (>50 lines = split candidate)
- Duplicate code
- Missing error handling

### 5. Architecture
- Separation of concerns
- Dependency direction (domain shouldn't depend on infrastructure)
- API contract consistency

## Output Format

```
## Code Review: [filename or scope]

### Critical (must fix)
- [file:line] Description of issue

### Warning (should fix)
- [file:line] Description of concern

### Suggestion (nice to have)
- [file:line] Description of improvement

### Good Practices Found
- Brief mention of well-written patterns
```

## Important

- Be specific: cite file names and line numbers
- Prioritize: Critical > Warning > Suggestion
- Don't nitpick style — focus on logic and security
- If the code looks good, say so briefly
