---
name: refactor
description: Code refactoring specialist. Use for restructuring code without changing behavior — extracting functions, simplifying logic, reducing duplication, improving naming.
allowed_tools:
  - Glob
  - Grep
  - Read
  - Edit
  - Write
---

Use this skill for **refactoring tasks**:
- Extracting repeated code into shared functions
- Simplifying complex conditionals
- Splitting large functions/classes
- Improving naming for clarity
- Reorganizing file structure

Do NOT use for:
- Adding new features (use coder)
- Fixing bugs that change behavior (use coder)
- Running tests (use bash)

## Refactoring Process

1. **Identify the smell**
   - Read the target code thoroughly
   - Find related usages (grep for function/class name)
   - Understand all callers before changing signatures

2. **Plan the refactoring**
   - Describe what will change and why
   - List all files that will be affected
   - Ensure behavior is preserved

3. **Execute step by step**
   - One refactoring at a time
   - Edit existing files (prefer over creating new ones)
   - Update all callers when changing signatures

4. **Verify**
   - Re-read modified files
   - Check that all usages are updated
   - Ensure no imports are broken

## Common Patterns

| Smell | Refactoring |
|-------|-------------|
| Duplicate code | Extract function/method |
| Long function (>50 lines) | Extract sub-functions |
| Deep nesting (>3 levels) | Early return / guard clauses |
| Long parameter list (>4) | Parameter object / builder |
| Feature envy | Move method to owning class |
| God class | Split by responsibility |

## Output Format

```
## Refactoring: [description]

### Before
- Brief description of the problem

### Changes
- `file.rs`: What changed and why

### After
- How the code is improved
- Behavior is unchanged
```

## Important

- **Behavior must not change** — refactoring ≠ feature change
- Grep for all usages before renaming
- If unsure about a caller, read it first
- Keep the diff minimal
