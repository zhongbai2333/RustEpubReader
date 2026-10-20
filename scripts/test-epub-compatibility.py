#!/usr/bin/env python3
import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("compat", Path(__file__).with_name("check-epub-compatibility.py"))
compat = importlib.util.module_from_spec(spec)
spec.loader.exec_module(compat)


class CorpusContract(unittest.TestCase):
    def reports(self):
        eager = {"audit_schema": 2, "path": "/owned/book.epub", "sha256": "a" * 64, "canonical_sha256": "b" * 64,
                 "loading": "eager", "stable_source": True, "chapters": 80,
                 "modes": [{"policy": p, "styles_sha256": "d" * 64, "blocks": 12, "note_references": 1} for p in ["none", "mixed", "publisher"]]}
        lazy = dict(copy.deepcopy(eager), loading="lazy", maximum_cached_chapters=8)
        return [eager], [lazy], [(Path(eager["path"]), eager["sha256"])]

    def test_accepts_matching_canonical_source(self):
        self.assertEqual(compat.compare(*self.reports())[0]["chapters"], 80)

    def test_rejects_content_and_semantic_loss(self):
        for field in ["chapters", "canonical_sha256", "modes", "styles_sha256"]:
            eager, lazy, inputs = self.reports()
            if field == "chapters": lazy[0][field] = 79
            elif field == "canonical_sha256": lazy[0][field] = "c" * 64
            elif field == "styles_sha256": lazy[0]["modes"][1][field] = "e" * 64
            else: lazy[0][field][1]["note_references"] = 0
            with self.subTest(field=field), self.assertRaises(ValueError): compat.compare(eager, lazy, inputs)

    def test_rejects_identity_incomplete_and_failed_input(self):
        for mutation in [lambda l: l.clear(), lambda l: l[0].update(path="/other/book.epub"), lambda l: l[0].update(error="Cannot parse"), lambda l: l[0].update(stable_source=False)]:
            eager, lazy, inputs = self.reports(); mutation(lazy)
            with self.assertRaises(ValueError): compat.compare(eager, lazy, inputs)

    def test_rejects_missing_style_cache_and_vacuous_coverage(self):
        for mutation in [lambda l: l[0].pop("audit_schema"), lambda l: l[0]["modes"][0].pop("styles_sha256"), lambda l: l[0]["modes"].pop(), lambda l: l[0].update(maximum_cached_chapters=9), lambda l: l[0].update(maximum_cached_chapters=True), lambda l: l[0]["modes"][0].update(blocks=0)]:
            eager, lazy, inputs = self.reports(); mutation(lazy)
            with self.assertRaises(ValueError): compat.compare(eager, lazy, inputs)


if __name__ == "__main__": unittest.main()
