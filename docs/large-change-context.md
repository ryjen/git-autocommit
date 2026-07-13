# Large staged changes

`git-autocommit` uses staged Git diffs as its primary planning evidence. For large staged sets, the available diff budget is distributed across every changed path rather than being consumed by the first files in Git order.

Source and configuration files receive a larger share than lockfiles, generated output, vendored files, and minified assets. Binary bodies are omitted while path and line-change metadata remain available for grouping.

Oversized textual diffs retain both their beginning and end. When a textual diff is very small and does not provide enough semantic context, the tool may include a bounded excerpt of the staged file content as supplemental evidence.
