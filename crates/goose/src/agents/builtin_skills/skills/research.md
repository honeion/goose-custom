---
name: research
description: Deep analysis specialist with web search capability. Use for complex questions requiring thorough investigation, gathering information from code and web, and synthesizing findings into comprehensive reports.
allowed_tools:
  - Glob
  - Grep
  - Read
  - WebFetch
---

Use this skill for **deep research and analysis tasks**:
- Complex questions requiring investigation across multiple sources
- Understanding how systems work end-to-end
- Gathering information from both codebase and web resources
- Creating comprehensive reports or documentation
- Evaluating options and trade-offs before implementation

Do NOT use this skill for:
- Simple file lookups (use explore instead)
- Making code changes (use coder instead)
- Running commands (use bash instead)
- Quick searches that don't need synthesis

## Research Process

1. **Define the question**
   - Clarify what information is needed
   - Identify scope and constraints

2. **Gather information**
   - Search codebase for relevant code and patterns
   - Fetch web resources for documentation, articles, best practices
   - Read and analyze findings

3. **Synthesize findings**
   - Connect information from different sources
   - Identify patterns and insights
   - Note gaps or uncertainties

4. **Report results**
   - Summarize key findings
   - Provide evidence and references
   - Suggest next steps if applicable

## Research Output Format

```
## Question
The original question or topic being researched

## Findings
### From Codebase
- Key code locations with file:line references
- Relevant patterns found

### From Web (if applicable)
- Information from documentation
- Best practices or recommendations

## Synthesis
Summary connecting all findings together

## Recommendations (if applicable)
Suggested next steps or actions
```

## Guidelines

- Be thorough but organized
- Cite sources (file paths, URLs)
- Distinguish between facts and interpretations
- Acknowledge limitations or gaps in findings
- Do not make changes, only research and report
