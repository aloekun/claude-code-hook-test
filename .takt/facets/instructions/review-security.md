Focus on **security anomaly detection** in the changed diff. Categorical vulnerability classes (injection / auth flaws / data exposure / crypto weakness / unsafe code / path traversal) remain in scope, but the bar for raising a finding is the same as the simplicity facet: an articulable concern with a concrete exploit path, not a checklist tick.

## Obtaining the diff

The diff has been pre-collected by push-runner (Rust exe) and saved to `.takt/review-diff.txt`.
**Read this file first** using the Read tool. This is the authoritative review target.
Do NOT run `git diff` or `jj diff` yourself -- the file already contains the correct diff scope.

## Project-Specific Context (read before judging)

Before evaluating the change, **read the following project documents** using the Read tool:

1. `CLAUDE.md` -- Project overview and ADR index
2. `docs/adr/adr-012-src-naming-convention.md` -- Naming convention (understand what each crate does)

**Important:**
- Do not treat documented precedence rules, extension points, or configuration override behavior as vulnerabilities by themselves.
- To raise a blocking finding, make the exploit path concrete: who controls what input, and what newly becomes possible.

## Vulnerability dimensions (use as memory aid, not a checklist)

The following classes remain reviewable, but flag them only when you can construct a concrete exploit path:

- **Injection attacks**: SQL, command, XSS — actor-controlled input flowing into an interpreter without escaping
- **Authentication and authorization flaws**: Missing checks, scope confusion, privilege escalation paths
- **Data exposure risks**: Hardcoded secrets, API keys, tokens, sensitive logs
- **Cryptographic weaknesses**: Broken algorithms, missing randomness, weak key handling
- **Unsafe code without safety comments** (Rust): `unsafe` blocks lacking `// SAFETY:` justification
- **Path traversal**: Unsanitized file paths reaching filesystem APIs

## Anomaly mode (preferred entry point)

Read the diff once, end-to-end, before consulting the dimensions list. If a pattern reads as **unusual / unexplained / hard to justify** from a security standpoint, that is your primary signal. The dimensions above are a memory aid for what to do with that signal, not a substitute for it.

For each finding, articulate:

- **What is unusual or risky**
- **Who controls the relevant input or configuration**
- **What newly becomes possible** (data access, privilege, code execution, prompt modification)

If you cannot articulate the third bullet, the finding is speculative — downgrade or drop it.

## Judgment procedure

1. Read the diff from `.takt/review-diff.txt`
2. Read straight through. Note any pattern that triggers a security concern
3. For each candidate, verify the concrete exploit path (input control, what becomes possible)
4. Classify each verified concern as blocking or non-blocking
5. If there is even one blocking concern with a concrete exploit path, judge as REJECT

## Docs-only changes: trust boundary criterion

For changes that touch **only documentation** (`docs/**`, ADRs, README, comments) and no executable code or configuration:

- **Pass criterion**: If the change does NOT alter a trust boundary, judge as APPROVE without further security analysis
- **Trust boundary unchanged** (APPROVE immediately):
  - Policy explanations, terminology definitions, design rationale
  - Workflow descriptions, ADR records of past decisions
  - Reformatting, hierarchy reorganization, cross-reference updates
- **Trust boundary changed** (apply full security review):
  - Documentation of new authentication / authorization policies
  - Redefinition of permission scopes or privilege boundaries
  - Changes to documented secret handling, credential storage, or trust assumptions
  - Specifications that other systems will rely on (API contracts, security guarantees)

Rationale: documentation that does not redefine who-can-do-what cannot introduce security vulnerabilities by itself. Treating descriptive docs as security-relevant produces false-positive iterations and erodes review signal.
