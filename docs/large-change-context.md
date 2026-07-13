# Large staged changes

`git-autocommit` uses staged Git diffs as its primary planning evidence. For large staged sets, the available diff budget is distributed across every changed path rather than being consumed by the first files in Git order.

Source and configuration files receive a larger share than lockfiles, generated output, vendored files, and minified assets. Binary bodies are omitted while path and line-change metadata remain available for grouping.

Oversized textual diffs retain both their beginning and end. When a textual diff is very small and does not provide enough semantic context, the tool may include a bounded excerpt of the staged file content as supplemental evidence. That supplemental excerpt is carved out of the file's existing allocation, so combined diff and file evidence never exceed the configured `max_diff_bytes` budget.

## Configuration

The planning policy can be customized in `.git/autocommit.toml`:

```toml
low_value_file_names = ["Cargo.lock", "flake.lock", "package-lock.json"]
low_value_path_fragments = ["/generated/", "/vendor/"]
low_value_suffixes = [".min.js", ".min.css"]
source_diff_weight = 3
low_value_diff_weight = 1
small_diff_bytes = 320
staged_file_context_bytes = 2000
truncation_marker = "\n...[middle of diff omitted]...\n"
```

All values are optional. Omitting them preserves the default policy.