Plan signed Conventional Commits for the staged Git changes below.

{{grouping_instruction}}

Return only a JSON array with this exact shape:
[
  {"message": "type(scope): imperative summary", "files": ["path/to/file"]}
]

Rules:
- Every staged path must appear exactly once across the plan.
- Do not invent, omit, duplicate, or rename paths.
- Group by intent and dependency, not merely directory.
- Keep related implementation, tests, and documentation together.
- Separate unrelated fixes, refactors, infrastructure, and generated changes.
- Never split one file across commits; file-level grouping is the safety boundary.
- Order foundational commits before dependent commits.
- Allowed types: feat, fix, docs, style, refactor, perf, test, build, ci, chore, revert.
- Use a scope only when obvious and useful.
- When a scope is used, it must contain only ASCII letters, digits, `-`, `_`, `.`, or `/`; spaces and other punctuation are invalid.
- Keep the complete subject, including type and optional scope, concise and imperative, with at most 72 characters.
- Add a short prose body only when it explains important rationale.
- Separate an optional body from the subject with exactly one blank line.
- Body lines must be prose, not metadata; do not emit trailers, attribution, sign-offs, review claims, issue-closing metadata, or other lines shaped like `Token: value` or `Token=value`.
- Keep the complete message within 4096 bytes and avoid control, bidirectional, or zero-width characters.
- Never claim tests passed unless the diff proves it.
- Produce at most {{max_commits}} commits.

Authoritative staged paths:
{{files_json}}

Staged changes:
{{context}}
