#!/usr/bin/env python3
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import zipfile


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


performance = module("performance", "check-performance.py")
fixtures = module("fixtures", "performance-fixtures.py")


class PerformanceTests(unittest.TestCase):
    def test_eager_reads_wrong_source_and_unbounded_cache_are_rejected(self):
        book = {"sha256": "sample", "chapters": 3000}
        sample = {"profile": "release", "book_sha256": "sample", "chapter_count": 3000,
                  "cold_stats": [0, 0, 3000], "cold_catalogue_ms": 4, "cached_catalogue_ms": 1,
                  "sequential_200_chapters": {"cache_stats": [8, 200, 3000], "p95_body_style_ms": 2}}
        performance.validate_sample(sample, book)
        for field, bad in [("profile", "debug"), ("book_sha256", "other"), ("cold_stats", [1, 1, 3000]), ("cold_catalogue_ms", float("nan"))]:
            with self.subTest(field=field), self.assertRaises(ValueError):
                performance.validate_sample(dict(sample, **{field: bad}), book)
        sample["sequential_200_chapters"]["cache_stats"][0] = 9
        with self.assertRaises(ValueError):
            performance.validate_sample(sample, book)

    def test_noise_is_allowed_but_regressions_and_unmatched_baselines_are_detected(self):
        baseline = {"environment": {"device": "same"}, "fixture_manifest_sha256": "same",
                    "books": {"book": {"metrics": {"cold_catalogue_ms": 100}}}}
        current = copy.deepcopy(baseline)
        current["books"]["book"]["metrics"]["cold_catalogue_ms"] = 120
        self.assertEqual(performance.compare(current, baseline), [])
        current["books"]["book"]["metrics"]["cold_catalogue_ms"] = 160
        self.assertEqual(len(performance.compare(current, baseline)), 1)
        for key in ["environment", "fixture_manifest_sha256", "books"]:
            changed = copy.deepcopy(current)
            changed[key] = {} if key != "fixture_manifest_sha256" else "different"
            with self.subTest(key=key), self.assertRaises(ValueError):
                performance.compare(changed, baseline)
        current["books"]["book"]["metrics"]["retained_rss_bytes"] = 200_000_000
        with self.assertRaisesRegex(ValueError, "metric coverage"):
            performance.compare(current, baseline)

    def test_corrupted_baseline_and_candidate_numbers_cannot_hide_a_regression(self):
        base={"environment":{"device":"same"},"fixture_manifest_sha256":"same","books":{"book":{"metrics":{"cold_catalogue_ms":100}}}}
        for side in ["baseline","candidate"]:
            for number in [float("nan"),float("inf"),-1,True]:
                baseline=copy.deepcopy(base);current=copy.deepcopy(base)
                target=baseline if side=="baseline" else current
                target["books"]["book"]["metrics"]["cold_catalogue_ms"]=number
                with self.subTest(side=side,number=number),self.assertRaisesRegex(ValueError,"comparison metric"):
                    performance.compare(current,baseline)

    def test_fixture_bytes_are_reproducible_and_have_epub_container(self):
        with tempfile.TemporaryDirectory() as directory:
            a, b = Path(directory) / "a.epub", Path(directory) / "b.epub"
            fixtures.write_epub(a, "fixture", 2, "tables")
            fixtures.write_epub(b, "fixture", 2, "tables")
            self.assertEqual(a.read_bytes(), b.read_bytes())
            with zipfile.ZipFile(a) as archive:
                self.assertEqual(archive.namelist()[0], "mimetype")
                self.assertEqual(archive.read("mimetype"), b"application/epub+zip")
                self.assertIn(b"<table>", archive.read("OEBPS/ch0.xhtml"))

    def test_media_corpus_contains_distinct_interleaved_images_and_long_tables(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "images.epub"
            fixtures.write_epub(path, "images", 1, "images")
            with zipfile.ZipFile(path) as archive:
                images = [archive.read(f"OEBPS/image{i}.png") for i in range(12)]
                self.assertEqual(len(set(images)), 12)
                self.assertTrue(all(len(data) > 50_000 for data in images))
                chapter = archive.read("OEBPS/ch0.xhtml")
                self.assertEqual(chapter.count(b"<figure>"), 12)
                self.assertLess(chapter.index(b"image0.png"), chapter.index(b"image1.png"))
            fixtures.write_epub(path, "table", 1, "tables")
            with zipfile.ZipFile(path) as archive:
                self.assertEqual(archive.read("OEBPS/ch0.xhtml").count(b"<tr>"), 161)


if __name__ == "__main__":
    unittest.main()
