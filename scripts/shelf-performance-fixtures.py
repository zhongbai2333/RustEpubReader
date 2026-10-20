#!/usr/bin/env python3
"""Portable, deterministic shelf corpus. No private books, fonts or artwork."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import uuid

spec = importlib.util.spec_from_file_location("performance_fixtures", Path(__file__).with_name("performance-fixtures.py"))
fixtures = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixtures)


def generate(output, count=2000):
    if not 1 <= count <= 10000:
        raise ValueError("Shelf size must be 1..10000")
    output.mkdir(parents=True, exist_ok=False)
    books = output / "books"
    books.mkdir()
    entries = []
    for index in range(count):
        identity = str(uuid.uuid5(uuid.NAMESPACE_URL, f"urn:reader:performance:shelf:v1:{index}"))
        title = f"Shelf {index:05d} — Reading fixture"
        path = books / (identity + ".epub")
        fixtures.write_epub(path, title, 1, "many", cover_variant=index)
        entries.append({"id": identity, "title": title, "file": path.relative_to(output).as_posix(),
                        "bytes": path.stat().st_size, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
    manifest = {"schema": 1, "scenario": "dense-shelf", "count": count,
                "source": "Generated text and unique procedural cover artwork; no external content", "books": entries}
    (output / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
    return manifest


def catalog(manifest, root):
    """Native installers use these same absolute-path and clock conventions."""
    books = []
    configs = {}
    for index, entry in enumerate(manifest["books"]):
        identifier = str(uuid.UUID(entry["id"]))
        if identifier != entry["id"] or entry["file"] != f"books/{identifier}.epub":
            raise ValueError("Invalid synthetic book identity")
        path = str(root.resolve() / entry["file"])
        book = {"id": identifier, "title": entry["title"], "path": path,
                "last_chapter": 0, "last_block": 0, "last_char_offset": 0,
                "last_chapter_title": "第1章 " + entry["title"], "last_opened": 1700000000000 - index * 1000}
        books.append(book)
        configs[identifier] = dict(book, epub_path=path, created_at=1700000000, updated_at=1700000000, settings={})
        del configs[identifier]["path"]
    if len(books) != manifest["count"] or len(configs) != len(books):
        raise ValueError("Missing or duplicate synthetic books")
    return {"books": books}, configs


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--count", type=int, default=2000)
    args = parser.parse_args()
    generate(args.output, args.count)
