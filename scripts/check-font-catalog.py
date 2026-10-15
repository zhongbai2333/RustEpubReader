#!/usr/bin/env python3
"""Audit the single native font catalog; optionally verify downloaded artifacts."""
import argparse, hashlib, json, re, struct
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument("--files", type=Path)
args = parser.parse_args()
catalog = json.loads((ROOT / "resources/fonts/catalog.json").read_text())
assert len({f["id"] for f in catalog}) == len(catalog)
assert len({f["postscript"] for f in catalog}) == len(catalog)
for font in catalog:
    assert re.fullmatch(r"[a-z0-9-]+", font["id"])
    assert Path(font["filename"]).name == font["filename"]
    assert Path(font["filename"]).suffix in {".ttf", ".otf"}
    assert re.fullmatch(r"https://raw\.githubusercontent\.com/[^/]+/[^/]+/[0-9a-f]{40}/.+", font["url"]) or re.fullmatch(r"https://github\.com/[^/]+/[^/]+/releases/download/v[0-9][^/]+/[^/]+", font["url"])
    assert font["language"] in {"zh", "en"}
    assert all(font[k] for k in ["name_zh", "name_en", "description_zh", "description_en"])
    assert font["license_url"].startswith("https://")
    assert re.fullmatch(r"[0-9]{4}-[0-9]{2}-[0-9]{2}", font["checked_at"])
    assert re.fullmatch(r"[0-9a-f]{64}", font["sha256"])
    assert 0 < font["bytes"] <= 64 * 1024 * 1024
    assert font["license_file"] == font["id"] + "-LICENSE.txt"
    license = (ROOT / "resources/fonts" / font["license_file"]).read_text()
    assert font["license_text"] == license
    assert font["license"] == "OFL-1.1" and "SIL OPEN FONT LICENSE" in license
    if args.files:
        file = args.files / font["filename"]
        assert file.stat().st_size == font["bytes"]
        assert hashlib.sha256(file.read_bytes()).hexdigest() == font["sha256"]
preview_root = ROOT / "resources/fonts/previews"
manifest = json.loads((preview_root / "manifest.json").read_text())
assert manifest["font_size"] == 68
samples = {p["id"]: p for p in manifest["samples"]}
assert len(samples) == len(catalog) == len(manifest["samples"])
assert set(samples) == {f["id"] for f in catalog}
assert {language: sum(f["language"] == language for f in catalog) for language in ["zh", "en"]} == {"zh": 10, "en": 10}
preview_bytes = 0
for font in catalog:
    sample = samples[font["id"]]
    assert sample["font_sha256"] == font["sha256"] and sample["postscript"] == font["postscript"]
    assert sample["file"] == font["id"] + ".png" and font["preview"] == "previews/" + sample["file"]
    assert sample["text"] == font["preview_text"] and len(sample["text"].splitlines()) == 2
    data = (preview_root / sample["file"]).read_bytes()
    assert hashlib.sha256(data).hexdigest() == sample["sha256"]
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    assert struct.unpack(">II", data[16:24]) == (sample["width"], sample["height"]) == (1200, 216)
    assert data[24:26] == bytes([8, 6]), "Expected 8-bit RGBA masks"
    preview_bytes += len(data)
assert preview_bytes < 1024 * 1024, "Preview payload must stay below 1 MiB"
print(f"Font previews: {len(samples)} exact-face masks, {preview_bytes} bytes")
external = json.loads((ROOT / "resources/fonts/external.json").read_text())
ids = {f["id"] for f in catalog}
for font in external:
    assert re.fullmatch(r"[a-z0-9-]+", font["id"]) and font["id"] not in ids
    ids.add(font["id"])
    assert font["license"] == "personal-noncommercial"
    assert not any(key in font for key in ["url", "filename", "sha256", "bytes", "license_text"]), "Official-only fonts must not have downloadable artifacts"
    assert font["project_url"].startswith("https://") and font["license_url"].startswith("https://")
    assert font["language"] in {"zh", "en"}
    assert all(font[k] for k in ["name_zh", "name_en", "description_zh", "description_en", "checked_at"])
print(f"Official-only font links: {len(external)} verified")
print(f"Font catalog: {len(catalog)} pinned entries, licenses and metadata verified" + (", all binaries verified" if args.files else ""))
