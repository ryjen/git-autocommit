from pathlib import Path

path = Path("README.md")
text = path.read_text()
old = "`base_url` contains embedded credentials, a query string, or a fragment; move authentication to the supported request mechanism and configure only the endpoint origin/path."
new = "`base_url` contains embedded credentials, a query string, or a fragment; URL-based authentication is not supported, so configure only the endpoint origin/path."

if old in text:
    path.write_text(text.replace(old, new, 1))
elif new not in text:
    raise SystemExit("base URL authentication guidance was not found")
