#!/usr/bin/env python3
"""Deterministic, freely redistributable EPUB stress fixtures; no external books."""
import argparse
import hashlib
import json
from pathlib import Path
import random
import struct
import zlib
import zipfile

TEXT = "阅读测试：章节按需加载，翻页保留原始位置。插图与表格前后的正文必须连续。English words and Unicode 😀 should stay intact. "


def png(width=512, height=768, variant=-1):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    if variant >= 0:
        # Deterministic coloured 4x4 blocks: meaningful compressed image payloads,
        # without third-party photos or enormous uncompressible ZIP fixtures.
        rng = random.Random(8107 + variant)
        rows = []
        for _ in range(height // 4):
            colours = rng.randbytes((width // 4) * 3)
            row = b"\0" + b"".join(colours[i:i + 3] * 4 for i in range(0, len(colours), 3))
            rows.extend([row] * 4)
        pixels = b"".join(rows)
    else:
        pixels = b"".join(b"\0" + bytes([40 + y % 80, 90, 130]) * width for y in range(height))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(pixels)) + chunk(b"IEND", b""))


def write_epub(path, name, count, kind, cover_variant=-1):
    with zipfile.ZipFile(path, "w") as archive:
        def put(filename, data, compression=zipfile.ZIP_DEFLATED):
            info = zipfile.ZipInfo(filename, (1980, 1, 1, 0, 0, 0))
            info.compress_type = compression
            archive.writestr(info, data)
        put("mimetype", "application/epub+zip", zipfile.ZIP_STORED)
        put("META-INF/container.xml", '<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/package.opf" media-type="application/oebps-package+xml"/></rootfiles></container>')
        manifest = '<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="cover" href="cover.png" media-type="image/png" properties="cover-image"/>'
        if kind == "images":
            for image in range(12):
                put(f"OEBPS/image{image}.png", png(variant=image))
                manifest += f'<item id="image{image}" href="image{image}.png" media-type="image/png"/>'
        spine, links = "", ""
        for chapter in range(count):
            title = f"第{chapter + 1}章 {name}"
            paragraphs = 1600 if kind == "long" else 5
            if kind == "many" and chapter in (3, count // 2, count - 1):
                paragraphs = 160  # Enough leaves for repeated turns at wide tablet sizes.
            body = f"<h1>{title}</h1>" + "".join(f"<p>{index + 1}. {TEXT * 3}</p>" for index in range(paragraphs))
            if kind == "images":
                body = f"<h1>{title}</h1>" + ''.join(f'<p>{TEXT}</p><figure><img src="image{image}.png" alt="Generated illustration {image}"/><figcaption>Figure {image + 1}</figcaption></figure>' for image in range(12))
            if kind == "tables":
                body += '<table><tr><th>章节</th><th>正文</th></tr>' + ''.join(f'<tr><td>{row}</td><td>{TEXT * 2}</td></tr>' for row in range(160)) + '</table>'
            put(f"OEBPS/ch{chapter}.xhtml", f'<html xmlns="http://www.w3.org/1999/xhtml"><head><title>{title}</title></head><body>{body}</body></html>')
            manifest += f'<item id="ch{chapter}" href="ch{chapter}.xhtml" media-type="application/xhtml+xml"/>'
            spine += f'<itemref idref="ch{chapter}"/>'
            links += f'<li><a href="ch{chapter}.xhtml">{title}</a></li>'
        put("OEBPS/cover.png", png(variant=cover_variant))
        put("OEBPS/nav.xhtml", f'<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>目录</title></head><body><nav epub:type="toc"><ol>{links}</ol></nav></body></html>')
        put("OEBPS/package.opf", f'<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="book"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="book">urn:reader:performance:{name}:v1</dc:identifier><dc:title>{name}</dc:title><dc:language>zh-CN</dc:language><meta property="dcterms:modified">2026-01-01T00:00:00Z</meta></metadata><manifest>{manifest}</manifest><spine>{spine}</spine></package>')


def generate(output):
    output.mkdir(parents=True, exist_ok=False)
    entries = []
    for name, count, kind in [("many-chapters", 3000, "many"), ("long-chapter", 1, "long"),
                              ("illustrated", 24, "images"), ("tables", 24, "tables")]:
        path = output / (name + ".epub")
        write_epub(path, name, count, kind)
        entries.append({"file": path.name, "chapters": count, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
    (output / "manifest.json").write_text(json.dumps({"schema": 1, "corpus_version": 2, "source": "Generated test content; no third-party book text or artwork", "books": entries}, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    generate(parser.parse_args().output)
