from pathlib import Path

path = Path(".github/scripts/apply-bearer-token-file.py")
text = path.read_text()
helper_anchor = '''def replace_once(path: str, old: str, new: str, label: str) -> None:
    target = Path(path)
    text = target.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one match in {path}, found {count}")
    target.write_text(text.replace(old, new, 1))
'''
helper_replacement = helper_anchor + '''

def replace_first(path: str, old: str, new: str, label: str) -> None:
    target = Path(path)
    text = target.read_text()
    if old not in text:
        raise SystemExit(f"{label}: expected a match in {path}")
    target.write_text(text.replace(old, new, 1))
'''
if text.count(helper_anchor) != 1:
    raise SystemExit("replace helper anchor not found exactly once")
text = text.replace(helper_anchor, helper_replacement, 1)
call_anchor = '''replace_once(
    "tests/bearer_token.rs",
    ''' + "'''" + '''        .arg("--base-url")
        .arg(&base_url)
        .env(TOKEN_ENV, TEST_TOKEN);''' + "'''" + ''',
    ''' + "'''" + '''        .arg("--base-url")
        .arg(&base_url)
        .env(TOKEN_ENV, TEST_TOKEN)
        .env_remove(TOKEN_FILE_ENV);''' + "'''" + ''',
    "direct token request test isolation",
)'''
call_replacement = call_anchor.replace("replace_once(", "replace_first(", 1)
if text.count(call_anchor) != 1:
    raise SystemExit("direct token request patch call not found exactly once")
path.write_text(text.replace(call_anchor, call_replacement, 1))
