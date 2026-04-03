---
name: test-writer
description: Test writing specialist. Use for creating unit tests, integration tests, and test fixtures. Reads source code to understand behavior, then writes corresponding tests.
allowed_tools:
  - Glob
  - Grep
  - Read
  - Edit
  - Write
---

Use this skill for **test writing tasks**:
- Writing unit tests for existing functions
- Creating integration tests for API endpoints
- Adding test fixtures and mock data
- Improving test coverage

## Test Writing Process

1. **Understand the code**
   - Read the target function/module
   - Identify inputs, outputs, side effects
   - Find edge cases (null, empty, boundary values)

2. **Detect test framework**
   - Python: pytest (default), unittest
   - Rust: built-in #[test], or tokio::test for async
   - TypeScript: jest, vitest
   - Go: testing package
   - Java: JUnit 5

3. **Write tests**
   - Follow existing test patterns in the project
   - Place test file in the conventional location:
     - Python: `tests/test_*.py` or `*_test.py`
     - Rust: `#[cfg(test)] mod tests` in same file or `tests/`
     - TS/JS: `*.test.ts` or `*.spec.ts`
     - Go: `*_test.go` in same directory

4. **Test structure (AAA pattern)**
   - **Arrange**: Set up test data and dependencies
   - **Act**: Call the function under test
   - **Assert**: Verify the result

## Test Categories

### Happy path (must have)
- Normal inputs → expected outputs
- At least 2-3 cases

### Edge cases (should have)
- Empty input, null/None, zero
- Boundary values (max, min)
- Unicode, special characters

### Error cases (should have)
- Invalid input → proper error
- Missing dependencies → graceful failure

## Output Format

```
## Tests Written: [target module]

### Files
- `test_xxx.py`: N test cases

### Coverage
- Happy path: N cases
- Edge cases: N cases
- Error handling: N cases
```

## Important

- Match existing test patterns in the project
- Don't mock what you can test directly
- Test behavior, not implementation details
- Keep tests independent (no shared mutable state)
- Use descriptive test names: `test_login_fails_with_invalid_password`
