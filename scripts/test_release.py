"""Release gates must reject partial, corrupted, or incorrectly versioned builds."""

from contextlib import redirect_stdout
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import release


class ReleaseTests(unittest.TestCase):
    def test_tag_matches_package_including_prerelease(self):
        release.check_tag("v1.2.3", "1.2.3")
        release.check_tag("v1.2.3-rc.1", "1.2.3-rc.1")
        for tag in ("1.2.3", "v1.2.4", "v1.2.3-rc.1"):
            with self.assertRaises(ValueError):
                release.check_tag(tag, "1.2.3")

    def test_release_requires_all_platforms_and_intact_archives(self):
        with tempfile.TemporaryDirectory() as name, redirect_stdout(io.StringIO()):
            directory = Path(name)
            binary = directory / "binary"
            binary.write_bytes(b"fixture executable")
            output = directory / "dist"
            with patch("release.subprocess.check_output", return_value="git-tui 1.2.3\n"):
                for target in release.TARGETS[:2]:
                    release.package(binary, target, output, "1.2.3")
                with self.assertRaisesRegex(ValueError, "missing="):
                    release.collect(output, "1.2.3")
                release.package(binary, release.TARGETS[2], output, "1.2.3")
            release.collect(output, "1.2.3")
            self.assertEqual(len((output / "SHA256SUMS").read_text().splitlines()), 3)
            archive = next(output.glob("*.tar.gz"))
            archive.write_bytes(archive.read_bytes() + b"corruption")
            with self.assertRaisesRegex(ValueError, "Checksum mismatch"):
                release.collect(output, "1.2.3")

    def test_binary_version_mismatch_prevents_packaging(self):
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            with patch("release.subprocess.check_output", return_value="git-tui 0.0.1\n"):
                with self.assertRaisesRegex(ValueError, "Binary version mismatch"):
                    release.package(directory / "binary", release.TARGETS[0], directory, "1.2.3")
            self.assertEqual(list(directory.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
