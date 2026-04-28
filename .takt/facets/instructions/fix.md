Use reports in the Report Directory and fix the issues raised by the reviewer.

## Read-only zones (ABSOLUTE -- violating these silently corrupts the review contract)

The `fix` step has broad `Edit` / `Write` / `Bash` permissions, but the following paths are **immutable inputs** to the workflow and MUST NEVER be edited, created under, deleted from, or moved by this step:

- **`.takt/runs/**`** -- Run-local report directories. The harness owns these.
- **`.takt/facets/**`**, **`.takt/workflows/**`**, **`.takt/config.yaml`** -- takt configuration assets.
- **`docs/adr/**`** -- Decision records are not review targets.
- **`templates/**`** -- Hook configuration templates.
- **`.claude/hooks-config.toml`** -- Hook configuration is not a review target.
- **`push-runner-config.toml`** -- Pipeline configuration is not a review target.

Your fixes MUST target the **source tree under review**: files under `src/` (Rust crates and TypeScript/Python sources). If a finding's suggested fix appears to require editing any file under the read-only zones listed above, treat it as a misdirected suggestion: report the mismatch in your `## Work results` section under a `### Misdirected finding` sub-heading and leave the read-only zone untouched.

If you catch yourself about to run a Bash command that writes into a read-only zone (including redirection like `>`, `>>`, `tee`, `sed -i` etc.), **stop**.

## Fix principles

- When a finding includes a "suggested fix", follow it rather than inventing your own workaround -- **except** when the suggestion targets a read-only zone; in that case, report the conflict and skip the fix.
- Fix the target code directly. Do not deflect findings by adding tests or documentation instead.

## Report reference policy

- Use the latest review reports in the Report Directory as primary evidence.
- Past iteration reports are saved as `{filename}.{timestamp}` in the same directory. For each report, run Glob with a `{report-name}.*` pattern, read up to 2 files in descending timestamp order, and understand persists / reopened trends before starting fixes.

## Completion criteria (all must be satisfied)

- All findings in this iteration (new / reopened) have been fixed in the correct source tree (not in any read-only zone).
- Potential occurrences of the same `family_tag` have been fixed simultaneously (no partial fixes that cause recurrence).

**Important**: After fixing, run the build and tests for the affected crate(s).

## Required output (include headings)

## Work results
- {Summary of actions taken}

### Read-only zone compliance
- {Confirm no writes attempted under read-only zones}

## Changes made
- {Summary of changes, listing the exact file paths modified}

## Build results
- {Build execution results}

## Test results
- {Test command executed and results}

## Convergence gate

| Metric | Count |
|--------|-------|
| new (fixed in this iteration) | {N} |
| reopened (recurrence fixed) | {N} |
| persists (carried over, not addressed this iteration) | {N} |
| misdirected (suggestion pointed at a read-only zone, skipped) | {N} |
