#!/usr/bin/env python3
"""Extract a monster/character Spine rig (.skel + .atlas + .png) from the game PCK.

The game ships Spine rigs as Godot *imported* resources under `.godot/imported/`:
  <name>.skel-<hash>.spskel   — the raw Spine binary skeleton, verbatim
  <name>.atlas-<hash>.spatlas — JSON `{"atlas_data": "<libgdx .atlas text>"}`
  <name>.png-<hash>.ctex      — Godot CompressedTexture2D (BPTC/S3TC) atlas page

This reconstructs the three plain files the workbench/wiki expect under
  site/public/images/{monster_rigs,character_rigs}/<slug>/<slug>.{skel,atlas,png}

Usage:
  uv run python -m scripts.extract_rig <pck> <source_name> <out_dir> [--slug SLUG]
e.g.
  uv run python -m scripts.extract_rig SlayTheSpire2.pck skulking_colony \
      site/public/images/monster_rigs
"""

import argparse
import json
import os
import struct

from scripts.extract_images import decode_ctex


def read_pck_index(pck_path):
    """Return {res-path: (offset, size)} for a Godot 4 PCK (mirrors extract_pck)."""
    with open(pck_path, "rb") as f:
        assert f.read(4) == b"GDPC", "bad PCK magic"
        fmt = struct.unpack("<I", f.read(4))[0]
        f.read(12)  # engine major/minor/patch
        f.read(4)  # pack flags
        file_base = struct.unpack("<Q", f.read(8))[0]
        dir_off = struct.unpack("<Q", f.read(8))[0] if fmt >= 3 else None
        f.read(64)  # reserved
        if dir_off is not None:
            f.seek(dir_off)
        n = struct.unpack("<I", f.read(4))[0]
        index = {}
        for _ in range(n):
            pl = struct.unpack("<I", f.read(4))[0]
            path = f.read(pl).rstrip(b"\x00").decode("utf-8")
            offset, size = struct.unpack("<q", f.read(8))[0], struct.unpack("<q", f.read(8))[0]
            f.read(16 + 4)  # md5 + flags
            index[path] = (file_base + offset, size)
    return index


def _read(pck_path, offset, size):
    with open(pck_path, "rb") as f:
        f.seek(offset)
        return f.read(size)


def _find(index, name, ext):
    """Find `.godot/imported/<name>.<src_ext>-<hash>.<ext>` entries, largest first."""
    pre = f".godot/imported/{name}."
    hits = [p for p in index if p.startswith(pre) and p.endswith(ext) and f".{ext_src(ext)}-" in p]
    return sorted(hits, key=lambda p: index[p][1], reverse=True)


def ext_src(ext):
    return {"spskel": "skel", "spatlas": "atlas", "ctex": "png"}[ext]


def extract_rig(pck_path, name, out_root, slug=None, atlas_name=None):
    """Reconstruct a Spine rig (``.skel`` + ``.atlas`` + page ``.png``\\ s).

    Most rigs name the skeleton, atlas and texture page identically (``name``).
    Some don't: the Decimillipede segments pair e.g. ``decimillipede3.skel`` with
    ``decimillipede_back.atlas`` (and all three segments share one
    ``decimillipede.png`` page), so the skeleton and atlas are passed separately.
    Texture-page filenames are always read from the atlas itself.

    Args:
        pck_path: Path to the game PCK.
        name: Source skeleton asset name in the PCK (the ``.spskel``).
        out_root: Output root; the rig is written under ``<out_root>/<slug>/``.
        slug: Output dir/file slug (default: ``name``).
        atlas_name: Source atlas asset name, when it differs from ``name``
            (default: ``name``).
    """
    slug = slug or name
    atlas_name = atlas_name or name
    index = read_pck_index(pck_path)
    out_dir = os.path.join(out_root, slug)
    os.makedirs(out_dir, exist_ok=True)

    # .skel — the .spskel is the raw Spine binary, verbatim
    skel = _find(index, name, "spskel")
    if not skel:
        raise SystemExit(f"no .spskel for '{name}' — it likely has no Spine rig (static sprite)")
    with open(os.path.join(out_dir, f"{slug}.skel"), "wb") as o:
        o.write(_read(pck_path, *index[skel[0]]))

    # .atlas — libgdx atlas text inside the .spatlas JSON wrapper. A rig may pack
    # into MULTIPLE atlas pages (kaiser_crab → kaiser_crab.png + kaiser_crab_2.png
    # …); the atlas lists each page by its own image filename. Parse those page
    # names so every page gets extracted, not just the first.
    atl = _find(index, atlas_name, "spatlas")
    if not atl:
        raise SystemExit(f"no .spatlas for '{atlas_name}'")
    atlas_text = json.loads(_read(pck_path, *index[atl[0]]))["atlas_data"]
    # Page headers are the only lines that end in ".png" (region names never do).
    page_files = [ln.strip() for ln in atlas_text.splitlines() if ln.strip().endswith(".png")]
    if not page_files:
        raise SystemExit(f"no atlas pages found for '{atlas_name}'")
    # Output page filenames, by slug: <slug>.png, <slug>_2.png, … (the page-index
    # convention the libgdx atlas / spine-webgl loader expects).
    out_pngs = [f"{slug}.png" if i == 0 else f"{slug}_{i + 1}.png" for i in range(len(page_files))]
    # Rewrite the atlas's page references (whatever they were named) to ours.
    for src_pf, out_pf in zip(page_files, out_pngs):
        if src_pf != out_pf:
            atlas_text = atlas_text.replace(src_pf, out_pf, 1)
    with open(os.path.join(out_dir, f"{slug}.atlas"), "w") as o:
        o.write(atlas_text)

    # .png — decode each page's Godot compressed-texture (one .ctex per page; a
    # page may ship >1 compression variant — BPTC/S3TC — so try largest first).
    sizes = []
    for src_pf, out_pf in zip(page_files, out_pngs):
        page_name = src_pf[:-4]  # source ctex asset name, e.g. "decimillipede"
        ctex = _find(index, page_name, "ctex")
        if not ctex:
            raise SystemExit(f"no .ctex page for atlas page '{page_name}' (rig '{name}')")
        img = None
        for c in ctex:
            img = decode_ctex(_read(pck_path, *index[c]))
            if img is not None:
                break
        if img is None:
            raise SystemExit(f"failed to decode any .ctex variant for page '{page_name}'")
        img.save(os.path.join(out_dir, out_pf))
        sizes.append(img.size)
    return out_dir, sizes[0], len(page_files)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("pck")
    ap.add_argument("name", help="source skeleton asset name in the PCK, e.g. skulking_colony")
    ap.add_argument("out_root", help="e.g. site/public/images/monster_rigs")
    ap.add_argument("--slug", help="output dir/file slug (default: name)")
    ap.add_argument("--atlas", help="source atlas asset name when it differs from <name>")
    a = ap.parse_args()
    out_dir, size, npages = extract_rig(a.pck, a.name, a.out_root, a.slug, a.atlas)
    # ASCII only: this is the LAST line of a successful run, and on a cp1252
    # console a non-ASCII glyph here raises UnicodeEncodeError *after* every
    # file is already written - so a clean extraction exits 1 with a traceback.
    print(f"wrote rig -> {out_dir}  (page 1 {size[0]}x{size[1]}, {npages} atlas page(s))")


if __name__ == "__main__":
    main()
