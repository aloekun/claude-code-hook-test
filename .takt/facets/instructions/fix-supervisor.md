Use reports in the Report Directory and fix the issues raised by the supervisor.

## Read-only zones (ABSOLUTE -- same constraints as the fix step)

The following paths are **immutable inputs** and MUST NEVER be edited:

- **`.takt/runs/**`** -- Run-local report directories.
- **`.takt/facets/**`**, **`.takt/workflows/**`**, **`.takt/config.yaml`** -- takt configuration assets.
- **`docs/adr/**`** -- Decision records are not review targets.
- **`templates/**`** -- Hook configuration templates.
- **`.claude/hooks-config.toml`** -- Hook configuration is not a review target.
- **`push-runner-config.toml`** -- Pipeline configuration is not a review target.

Fixes MUST target the **source tree under review**: files under `src/`.

## Fix principles

- Follow the supervisor's specific guidance for each finding.
- Fix the target code directly. Do not deflect findings by adding tests or documentation instead.
- After fixing, run the build and tests for the affected crate(s).

## Required output

## Work results
- {Summary of actions taken}

### Read-only zone compliance
- {Confirm no writes attempted under read-only zones}

## Changes made
- {File paths modified}

## Build results
- {Build execution results}

## Test results
- {Test results}
