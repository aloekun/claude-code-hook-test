Review the changes from a security perspective. Check for the following vulnerabilities:
- Injection attacks (SQL, command, XSS)
- Authentication and authorization flaws
- Data exposure risks (hardcoded secrets, API keys, tokens)
- Cryptographic weaknesses
- Unsafe code without safety comments (Rust)
- Path traversal risks

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
- To issue a blocking finding, make the exploit path concrete: who controls what input, and what newly becomes possible.

## Judgment Procedure

1. Review the change diff and extract issue candidates
2. For each candidate, verify the concrete exploit path
   - Which actor controls the input or configuration
   - Whether the change enables new privilege, data access, code execution, or prompt modification
3. For each detected issue, classify it as blocking or non-blocking
4. If there is even one blocking issue, judge as REJECT
