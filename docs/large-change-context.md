# Large staged changes

`git-autocommit` uses staged Git diffs as its primary planning evidence. For large staged sets, the available diff budget is distributed across every changed path rather than being consumed by the first files in Git order.

Source and configuration files receive a larger share than lockfiles, generated output, vendored files, and minified assets. Binary bodies are omitted while path and line-change metadata remain available for grouping.

Oversized textual diffs retain both their beginning and end. When a textual diff is very small and does not provide enough semantic context, the tool may include a bounded excerpt of the staged file content as supplemental evidence. That supplemental excerpt is carved out of the file's existing allocation, so combined diff and file evidence never exceed the active `max_diff_bytes` budget.

## Adaptive default budget

When neither `GIT_AUTOCOMMIT_MAX_DIFF_BYTES` nor repository `max_diff_bytes` is set, `git-autocommit` selects the evidence budget from the total staged textual diff before applying the normal weighted per-file allocator:

- small staged changes use up to 12 KB of evidence;
- medium staged changes use up to 32 KB;
- large staged changes use up to 64 KB.

This keeps routine commit planning substantially below code-review-sized context while retaining more evidence for heterogeneous or large changes. The implicit prompt ceiling is 96 KB. `git autocommit --show-config` reports the 64 KB adaptive ceiling because it intentionally exits before inspecting staged content; the actual 12/32/64 KB tier is selected only for an invocation that reads the staged snapshot.

Setting `max_diff_bytes` explicitly disables the adaptive diff policy and selects that fixed budget. If the diff limit is explicitly increased while `max_prompt_bytes` remains implicit, the prompt ceiling keeps at least 40 KB of headroom above the requested diff limit. An explicit `max_prompt_bytes` remains authoritative.

## Configuration

The planning policy can be customized in `.git/autocommit.toml`:

```toml
# Set only when a fixed evidence budget is preferable to the adaptive default.
# max_diff_bytes = 64000
# max_prompt_bytes = 96000

low_value_file_names = ["Cargo.lock", "flake.lock", "package-lock.json"]
low_value_path_fragments = ["/generated/", "/vendor/"]
low_value_suffixes = [".min.js", ".min.css"]
source_diff_weight = 3
low_value_diff_weight = 1
small_diff_bytes = 320
staged_file_context_bytes = 2000
truncation_marker = "\n...[middle of diff omitted]...\n"
```

All values are optional. Omitting the context limits uses the adaptive defaults; omitting the weighting/excerpt values preserves the existing evidence-allocation policy.