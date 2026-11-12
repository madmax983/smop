"""Changelog fragments: one file per change in changelog.d/.

    python3 scripts/changelog.py check            # Validate fragments (CI)
    python3 scripts/changelog.py preview          # Show the next release notes
    python3 scripts/changelog.py release 0.4.0 [YYYY-MM-DD]

A fragment is changelog.d/<name>.<section>.md. Its text is one or more
Markdown list items. See changelog.d/README.md.
"""

import datetime
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# Keep a Changelog sections, in release order. The first is smop's own.
SECTIONS = ["breaking", "added", "changed", "deprecated", "removed", "fixed", "security"]
NAME = re.compile(r"[a-z0-9]+(?:-[a-z0-9]+)*\.(" + "|".join(SECTIONS) + r")\.md")
SKIP = {"README.md", ".gitkeep"}
REPO = "https://github.com/madmax983/smop"
# A released version: `## [0.3.2] - 2026-11-03`. "Unreleased" does not match.
RELEASED = re.compile(r"^## \[([^\]]+)\] - \d{4}-", re.M)
# A version link line: `[0.3.2]: <url>`. Other link lines do not match.
LINK = re.compile(r"^\[\d[^\]]*\]: ", re.M)


def fragments(directory):
    """Returns (section, name, text) for each fragment, sorted by name."""
    out = []
    for name in sorted(os.listdir(directory)):
        if name in SKIP:
            continue
        with open(os.path.join(directory, name), encoding="utf-8") as f:
            text = f.read()
        match = NAME.fullmatch(name)
        out.append((match.group(1) if match else None, name, text))
    return out


def check(root):
    """Returns a list of errors. An empty list means all is correct."""
    errors = []
    for section, name, text in fragments(os.path.join(root, "changelog.d")):
        if section is None:
            errors.append(
                f"changelog.d/{name}: name must be <name>.<section>.md, with a "
                f"lowercase name and a section from: {', '.join(SECTIONS)}"
            )
        elif not text.startswith("- "):
            errors.append(f"changelog.d/{name}: text must start with a list item ('- ')")
    with open(os.path.join(root, "CHANGELOG.md"), encoding="utf-8") as f:
        text = f.read()
    if re.search(r"^## .*Unreleased", text, re.M):
        errors.append(
            "CHANGELOG.md: has an Unreleased section. Put unreleased changes in changelog.d/"
        )
    for version in RELEASED.findall(text):
        if not re.search(rf"^\[{re.escape(version)}\]: \S", text, re.M):
            errors.append(f"CHANGELOG.md: [{version}] has no link line ([{version}]: <url>)")
    return errors


def link(version, previous):
    """Returns the link line for a release: a compare link, or the tag for the first."""
    if previous is None:
        return f"[{version}]: {REPO}/releases/tag/v{version}"
    return f"[{version}]: {REPO}/compare/v{previous}...v{version}"


def render(items, heading):
    """Returns the release notes for the fragments, or '' if there are none."""
    if not items:
        return ""
    parts = [heading + "\n"]
    for section in SECTIONS:
        texts = [text.rstrip("\n") + "\n" for s, _, text in items if s == section]
        if texts:
            parts.append(f"### {section.capitalize()}\n" + "".join(texts))
    return "\n".join(parts)


def release(root, version, date):
    """Writes the fragments into CHANGELOG.md as a release, then removes them."""
    errors = check(root)
    try:
        datetime.date.fromisoformat(date)
    except ValueError:
        errors.append(f"date {date}: must be YYYY-MM-DD")
    if errors:
        sys.exit("\n".join(errors))
    directory = os.path.join(root, "changelog.d")
    items = fragments(directory)
    if not items:
        sys.exit("changelog.d has no fragments to release")
    path = os.path.join(root, "CHANGELOG.md")
    with open(path, encoding="utf-8") as f:
        text = f.read()
    if version in RELEASED.findall(text):
        sys.exit(f"CHANGELOG.md: {version} is released")
    notes = render(items, f"## [{version}] - {date}") + "\n"
    previous = RELEASED.search(text)
    line = link(version, previous and previous.group(1))
    at = text.find("\n## [")
    at = len(text) if at < 0 else at + 1
    text = text[:at] + notes + text[at:]
    first_link = LINK.search(text)
    if first_link:
        text = text[: first_link.start()] + line + "\n" + text[first_link.start() :]
    else:
        text = text.rstrip("\n") + "\n\n" + line + "\n"
    with open(path, "w", encoding="utf-8") as f:
        f.write(text)
    for _, name, _ in items:
        os.remove(os.path.join(directory, name))


def main(args):
    command = args[0] if args else ""
    if command == "check":
        errors = check(ROOT)
        print("\n".join(errors) or "changelog: ok")
        return 1 if errors else 0
    if command == "preview":
        items = fragments(os.path.join(ROOT, "changelog.d"))
        print(render(items, "## [Unreleased]") or "changelog.d has no fragments")
        return 0
    if command == "release" and len(args) in (2, 3):
        date = args[2] if len(args) == 3 else datetime.date.today().isoformat()
        release(ROOT, args[1], date)
        print(f"CHANGELOG.md: added {args[1]}. Commit CHANGELOG.md and changelog.d/.")
        return 0
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
