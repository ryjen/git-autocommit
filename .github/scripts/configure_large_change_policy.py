from pathlib import Path
import re

path = Path("src/main.rs")
text = path.read_text()

text = text.replace(
    "const DEFAULT_MAX_COMMITS: usize = 8;\n",
    '''const DEFAULT_MAX_COMMITS: usize = 8;
const DEFAULT_SOURCE_DIFF_WEIGHT: usize = 3;
const DEFAULT_LOW_VALUE_DIFF_WEIGHT: usize = 1;
const DEFAULT_SMALL_DIFF_BYTES: usize = 320;
const DEFAULT_STAGED_FILE_CONTEXT_BYTES: usize = 2_000;
const DEFAULT_TRUNCATION_MARKER: &str = "\\n...[middle of diff omitted]...\\n";
''',
    1,
)
text = text.replace(
    "    sign_commits: Option<bool>,\n}",
    '''    sign_commits: Option<bool>,
    low_value_file_names: Option<Vec<String>>,
    low_value_path_fragments: Option<Vec<String>>,
    low_value_suffixes: Option<Vec<String>>,
    source_diff_weight: Option<usize>,
    low_value_diff_weight: Option<usize>,
    small_diff_bytes: Option<usize>,
    staged_file_context_bytes: Option<usize>,
    truncation_marker: Option<String>,
}''',
    1,
)
text = text.replace(
    "    sign_commits: bool,\n    config_path: PathBuf,\n}",
    '''    sign_commits: bool,
    low_value_file_names: Vec<String>,
    low_value_path_fragments: Vec<String>,
    low_value_suffixes: Vec<String>,
    source_diff_weight: usize,
    low_value_diff_weight: usize,
    small_diff_bytes: usize,
    staged_file_context_bytes: usize,
    truncation_marker: String,
    config_path: PathBuf,
}''',
    1,
)

defaults = '''fn default_low_value_file_names() -> Vec<String> {
    ["Cargo.lock", "flake.lock", "package-lock.json", "pnpm-lock.yaml", "yarn.lock", "poetry.lock", "uv.lock", "go.sum"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

fn default_low_value_path_fragments() -> Vec<String> {
    ["/generated/", "/vendor/"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

fn default_low_value_suffixes() -> Vec<String> {
    [".min.js", ".min.css"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

'''
if defaults not in text:
    text = text.replace("fn resolve_toggle(", defaults + "fn resolve_toggle(", 1)

text = text.replace(
    "        sign_commits,\n        config_path,\n",
    '''        sign_commits,
        low_value_file_names: config.low_value_file_names.unwrap_or_else(default_low_value_file_names),
        low_value_path_fragments: config.low_value_path_fragments.unwrap_or_else(default_low_value_path_fragments),
        low_value_suffixes: config.low_value_suffixes.unwrap_or_else(default_low_value_suffixes),
        source_diff_weight: positive_usize(config.source_diff_weight.unwrap_or(DEFAULT_SOURCE_DIFF_WEIGHT), "source_diff_weight")?,
        low_value_diff_weight: positive_usize(config.low_value_diff_weight.unwrap_or(DEFAULT_LOW_VALUE_DIFF_WEIGHT), "low_value_diff_weight")?,
        small_diff_bytes: positive_usize(config.small_diff_bytes.unwrap_or(DEFAULT_SMALL_DIFF_BYTES), "small_diff_bytes")?,
        staged_file_context_bytes: positive_usize(config.staged_file_context_bytes.unwrap_or(DEFAULT_STAGED_FILE_CONTEXT_BYTES), "staged_file_context_bytes")?,
        truncation_marker: config.truncation_marker.unwrap_or_else(|| DEFAULT_TRUNCATION_MARKER.to_owned()),
        config_path,
''',
    1,
)

start = text.index("fn is_low_value_diff(")
end = text.index("\nfn staged_context(", start)
replacement = r'''fn is_low_value_diff(path: &str, settings: &Settings) -> bool {
    let name = Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(path);
    settings.low_value_file_names.iter().any(|value| value == name)
        || settings.low_value_path_fragments.iter().any(|value| path.contains(value))
        || settings.low_value_suffixes.iter().any(|value| path.ends_with(value))
}

fn diff_weight(path: &str, settings: &Settings) -> usize {
    if is_low_value_diff(path, settings) {
        settings.low_value_diff_weight
    } else {
        settings.source_diff_weight
    }
}

fn allocate_diff_budgets(files: &[String], binary: &[bool], max_bytes: usize, settings: &Settings) -> Vec<usize> {
    let weights: Vec<usize> = files
        .iter()
        .zip(binary)
        .map(|(path, binary)| if *binary { 0 } else { diff_weight(path, settings) })
        .collect();
    let total_weight: usize = weights.iter().sum();
    if total_weight == 0 {
        return vec![0; files.len()];
    }
    let mut budgets: Vec<usize> = weights
        .iter()
        .map(|weight| max_bytes.saturating_mul(*weight) / total_weight)
        .collect();
    let assigned: usize = budgets.iter().sum();
    let mut remainder = max_bytes.saturating_sub(assigned);
    for (budget, weight) in budgets.iter_mut().zip(&weights) {
        if remainder == 0 { break; }
        if *weight > 0 { *budget += 1; remainder -= 1; }
    }
    budgets
}

fn utf8_prefix_len(value: &str, max_bytes: usize) -> usize {
    let mut end = max_bytes.min(value.len());
    while end > 0 && !value.is_char_boundary(end) { end -= 1; }
    end
}

fn excerpt(value: &str, max_bytes: usize, marker: &str) -> (String, bool) {
    if value.len() <= max_bytes { return (value.to_owned(), false); }
    if max_bytes == 0 { return (String::new(), true); }
    if max_bytes <= marker.len() + 8 {
        let end = utf8_prefix_len(value, max_bytes);
        return (value[..end].to_owned(), true);
    }
    let content_budget = max_bytes - marker.len();
    let head_limit = content_budget * 2 / 3;
    let tail_limit = content_budget - head_limit;
    let head_end = utf8_prefix_len(value, head_limit);
    let mut tail_start = value.len().saturating_sub(tail_limit);
    while tail_start < value.len() && !value.is_char_boundary(tail_start) { tail_start += 1; }
    (format!("{}{}{}", &value[..head_end], marker, &value[tail_start..]), true)
}

fn split_evidence_budget(path: &str, diff_len: usize, budget: usize, settings: &Settings) -> (usize, usize) {
    if diff_len < settings.small_diff_bytes && !is_low_value_diff(path, settings) {
        let file_budget = budget.min(settings.staged_file_context_bytes) / 2;
        (budget.saturating_sub(file_budget), file_budget)
    } else {
        (budget, 0)
    }
}

fn staged_file_excerpt(repo: &Repo, path: &str, max_bytes: usize, settings: &Settings) -> Option<String> {
    if max_bytes == 0 || is_low_value_diff(path, settings) { return None; }
    let spec = format!(":{path}");
    let content = repo.git(&["show", &spec]).ok()?;
    if content.as_bytes().contains(&0) { return None; }
    let (content, truncated) = excerpt(&content, max_bytes, &settings.truncation_marker);
    Some(if truncated { format!("{content}\n[staged file excerpt truncated]") } else { content })
}
'''
text = text[:start] + replacement + text[end:]
text = text.replace(
    "fn staged_context(repo: &Repo, files: &[String], max_bytes: usize) -> Result<String> {",
    "fn staged_context(repo: &Repo, files: &[String], settings: &Settings) -> Result<String> {",
    1,
)
text = text.replace(
    "let budgets = allocate_diff_budgets(files, &binary, max_bytes);",
    "let budgets = allocate_diff_budgets(files, &binary, settings.max_diff_bytes, settings);",
    1,
)
text = text.replace("is_low_value_diff(path)", "is_low_value_diff(path, settings)")
text = text.replace("split_evidence_budget(path, diff.len(), budget)", "split_evidence_budget(path, diff.len(), budget, settings)")
text = text.replace("excerpt(&diff, diff_budget)", "excerpt(&diff, diff_budget, &settings.truncation_marker)")
text = text.replace("staged_file_excerpt(repo, path, file_budget)", "staged_file_excerpt(repo, path, file_budget, settings)")
text = text.replace("staged_context(&repo, &files, settings.max_diff_bytes)?", "staged_context(&repo, &files, &settings)?")
text = re.sub(r"let budgets = allocate_diff_budgets\(&files, &\[false, false, false\], 700\);", "let settings = settings_for(false);\n        let budgets = allocate_diff_budgets(&files, &[false, false, false], 700, &settings);", text)
text = re.sub(r"let budgets = allocate_diff_budgets\(&files, &\[true, false\], 1_000\);", "let settings = settings_for(false);\n        let budgets = allocate_diff_budgets(&files, &[true, false], 1_000, &settings);", text)
text = text.replace('let (diff_budget, file_budget) = split_evidence_budget("src/main.rs", 100, 1_000);', 'let settings = settings_for(false);\n        let (diff_budget, file_budget) = split_evidence_budget("src/main.rs", 100, 1_000, &settings);')
text = text.replace('let (diff_budget, file_budget) = split_evidence_budget("Cargo.lock", 100, 1_000);', 'let (diff_budget, file_budget) = split_evidence_budget("Cargo.lock", 100, 1_000, &settings);')
text = text.replace("excerpt(&value, 80)", "excerpt(&value, 80, DEFAULT_TRUNCATION_MARKER)")
text = text.replace('excerpt("αβγδε", 5)', 'excerpt("αβγδε", 5, DEFAULT_TRUNCATION_MARKER)')
path.write_text(text)

doc = Path("docs/large-change-context.md")
if "## Configuration" not in doc.read_text():
    doc.write_text(doc.read_text() + '''

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
truncation_marker = "\\n...[middle of diff omitted]...\\n"
```

All values are optional. Omitting them preserves the default policy.
''')
