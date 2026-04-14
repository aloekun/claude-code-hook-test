Focus on reviewing **architecture and design**.

## Obtaining the diff

The diff has been pre-collected by push-runner (Rust exe) and saved to `.takt/review-diff.txt`.
**Read this file first** using the Read tool. This is the authoritative review target.
Do NOT run `git diff` or `jj diff` yourself -- the file already contains the correct diff scope.

## Project-Specific Context (read before judging)

Before evaluating the change, **read the following project documents** using the Read tool:

1. `CLAUDE.md` -- Project overview and ADR index
2. `docs/adr/adr-012-src-naming-convention.md` -- Naming convention for src/ directory (hooks- / cli- / lib- prefixes)
3. `docs/adr/adr-010-hooks-layout-and-build-strategy-v2.md` -- Hooks layout and build strategy

These ADRs define the authoritative architectural decisions. Treat violations as blocking findings.

## Built-in Review Criteria (apply after project-specific rules)

**Review criteria:**
- Structural and design validity
- Modularization (high cohesion, low coupling, no circular dependencies)
- Functionalization (single responsibility per function, operation discoverability, consistent abstraction level)
- Code quality (immutability, error handling, naming)
- Appropriateness of change scope
- Test coverage
- Dead code
- Call chain verification
- File size (200-400 lines typical, 800 max per file)
- Function size (< 50 lines)

**Previous finding tracking (required):**
- First, extract open findings from "Previous Response"
- Assign `finding_id` to each finding and classify current status as `new / persists / resolved`
- If status is `persists`, provide concrete unresolved evidence (file/line)

## Judgment Procedure

1. First, extract previous open findings and preliminarily classify as `new / persists / resolved`
2. Review the change diff and detect issues based on the architecture and design criteria above
3. For each detected issue, classify as blocking/non-blocking
4. If there is even one blocking issue (`new` or `persists`), judge as REJECT
