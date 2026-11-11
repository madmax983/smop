"""Tests for scripts/changelog.py. Run: python3 -I scripts/test_changelog.py"""

import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import changelog  # noqa: E402

HEAD = "# Changelog\n\nIntro.\n\n"
OLD = "## [0.3.2] - 2026-11-03\n\n### Added\n- Old entry.\n"


class Repo:
    """A temporary root with CHANGELOG.md and changelog.d/."""

    def __init__(self, changelog_text=HEAD + OLD):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = self.tmp.name
        os.mkdir(self.dir)
        self.write("CHANGELOG.md", changelog_text)
        self.write("changelog.d/README.md", "# How to\n")

    @property
    def dir(self):
        return os.path.join(self.root, "changelog.d")

    def write(self, rel, text):
        with open(os.path.join(self.root, rel), "w", encoding="utf-8") as f:
            f.write(text)

    def read(self, rel):
        with open(os.path.join(self.root, rel), encoding="utf-8") as f:
            return f.read()

    def fragment(self, name, text):
        self.write(f"changelog.d/{name}", text)


class Case(unittest.TestCase):
    def repo(self, changelog_text=HEAD + OLD):
        repo = Repo(changelog_text)
        self.addCleanup(repo.tmp.cleanup)
        return repo


class Render(Case):
    def test_sections_in_fixed_order_and_files_by_name(self):
        repo = self.repo()
        repo.fragment("zeta.fixed.md", "- Fix Z.\n")
        repo.fragment("b.added.md", "- Add B.\n  More text.\n")
        repo.fragment("a.added.md", "- Add A.\n")
        repo.fragment("x.breaking.md", "- Break X.\n")
        text = changelog.render(changelog.fragments(repo.dir), "## [Unreleased]")
        self.assertEqual(
            text,
            "## [Unreleased]\n\n"
            "### Breaking\n- Break X.\n\n"
            "### Added\n- Add A.\n- Add B.\n  More text.\n\n"
            "### Fixed\n- Fix Z.\n",
        )

    def test_no_fragments_renders_nothing(self):
        self.assertEqual(changelog.render([], "## [Unreleased]"), "")


class Check(Case):
    def test_good_fragments_pass(self):
        repo = self.repo()
        repo.fragment("env-var-or-2.breaking.md", "- Text.\n")
        self.assertEqual(changelog.check(repo.root), [])

    def test_bad_names_fail(self):
        for name in ["Upper.added.md", "x.feature.md", "x.md", "x.added.txt", "x..added.md"]:
            repo = self.repo()
            repo.fragment(name, "- Text.\n")
            errors = changelog.check(repo.root)
            self.assertEqual(len(errors), 1, name)
            self.assertIn(name, errors[0])

    def test_content_must_be_a_list_item(self):
        for text in ["", "\n", "Text without a bullet.\n", "## Heading\n"]:
            repo = self.repo()
            repo.fragment("x.added.md", text)
            errors = changelog.check(repo.root)
            self.assertEqual(len(errors), 1, repr(text))
            self.assertIn("x.added.md", errors[0])

    def test_unreleased_section_in_changelog_fails(self):
        repo = self.repo(HEAD + "## [0.4.0] - Unreleased\n\n" + OLD)
        errors = changelog.check(repo.root)
        self.assertEqual(len(errors), 1)
        self.assertIn("Unreleased", errors[0])


class Release(Case):
    def test_release_writes_section_and_removes_fragments(self):
        repo = self.repo()
        repo.fragment("a.added.md", "- Add A.\n")
        repo.fragment("b.changed.md", "- Change B.\n")
        changelog.release(repo.root, "0.4.0", "2026-12-01")
        self.assertEqual(
            repo.read("CHANGELOG.md"),
            HEAD
            + "## [0.4.0] - 2026-12-01\n\n"
            "### Added\n- Add A.\n\n"
            "### Changed\n- Change B.\n\n"
            + OLD,
        )
        self.assertEqual(sorted(os.listdir(repo.dir)), ["README.md"])

    def test_release_without_fragments_fails(self):
        repo = self.repo()
        with self.assertRaisesRegex(SystemExit, "no fragments"):
            changelog.release(repo.root, "0.4.0", "2026-12-01")
        self.assertEqual(repo.read("CHANGELOG.md"), HEAD + OLD)

    def test_release_with_a_bad_fragment_changes_nothing(self):
        repo = self.repo()
        repo.fragment("a.added.md", "- Add A.\n")
        repo.fragment("b.nope.md", "- Bad.\n")
        with self.assertRaisesRegex(SystemExit, "b.nope.md"):
            changelog.release(repo.root, "0.4.0", "2026-12-01")
        self.assertEqual(repo.read("CHANGELOG.md"), HEAD + OLD)
        self.assertIn("a.added.md", os.listdir(repo.dir))


if __name__ == "__main__":
    unittest.main()
