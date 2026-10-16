# /// script
# requires-python = ">=3.11"
# dependencies = ["pillow>=10"]
# ///
"""Build the self-hosted timer buddy sprites from a PMD SpriteCollab checkout.

Each Pokemon becomes one lossless WebP atlas plus a small JSON manifest in
`app/public/buddy/`. Only the animations and facing directions the buddy uses are
kept, and left-facing rows are dropped because the app mirrors the right-facing ones.
The up-right rows are the back view the battle screen shows for your own buddies.

Get the source (sparse, only the files this script reads):

    git clone --depth 1 --filter=blob:none --no-checkout \
        https://github.com/PMDCollab/SpriteCollab.git
    cd SpriteCollab
    git sparse-checkout init --no-cone
    git sparse-checkout set /tracker.json /credit_names.txt \
        '/sprite/*/AnimData.xml' '/sprite/*/credits.txt' \
        '/sprite/*/Walk-Anim.png' '/sprite/*/Idle-Anim.png' '/sprite/*/Sleep-Anim.png' \
        '/sprite/*/Hop-Anim.png' '/sprite/*/Hurt-Anim.png' '/sprite/*/Charge-Anim.png' \
        '/sprite/*/Attack-Anim.png' '/sprite/*/Swing-Anim.png' '/sprite/*/Rotate-Anim.png' \
        '/sprite/*/Double-Anim.png'
    git checkout

Then, from `app/`:

    uv run scripts/build_buddy_sprites.py --source /path/to/SpriteCollab
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
import xml.etree.ElementTree as ET
from dataclasses import dataclass
from pathlib import Path

from PIL import Image

# Sheet rows are ordered by facing direction: D, DR, R, UR, U, UL, L, DL.
DIRECTION_ROW = {"D": 0, "DR": 1, "R": 2, "UR": 3}

# Facing directions that are packed only when the sheet has them. A Pokemon
# without them keeps its atlas, and the battle screen falls back to a side view.
OPTIONAL_DIRECTIONS = {"UR"}

# Animation -> facing directions to keep. Sleep sheets have a single row.
ANIMATIONS: dict[str, list[str]] = {
    "Idle": ["D", "DR", "R", "UR"],
    "Walk": ["D", "R"],
    "Sleep": ["D"],
    "Hop": ["D"],
    "Hurt": ["D", "UR"],
    "Rotate": ["D"],
    "Charge": ["D", "R", "UR"],
    "Attack": ["R", "UR"],
    "Swing": ["R"],
    "Double": ["R"],
}

# The tracker names Pokemon with identifiers. These are the ones whose display
# name is spelled differently.
DISPLAY_NAMES: dict[str, str] = {
    "Nidoran_F": "Nidoran♀",
    "Nidoran_M": "Nidoran♂",
    "Farfetch_d": "Farfetch'd",
    "Mr_Mime": "Mr. Mime",
    "Ho_Oh": "Ho-Oh",
    "Mime_Jr_": "Mime Jr.",
    "Porygon_Z": "Porygon-Z",
}

OPAQUE_ALPHA = 40
APP_DIR = Path(__file__).resolve().parent.parent


@dataclass
class SourceAnim:
    sheet: Path
    frame_width: int
    frame_height: int
    durations: list[int]


def read_anims(sprite_dir: Path) -> dict[str, SourceAnim]:
    """Parses AnimData.xml, following `CopyOf` references to the sheet they reuse."""
    root = ET.parse(sprite_dir / "AnimData.xml").getroot()
    nodes = {
        node.findtext("Name", ""): node for node in root.iterfind("./Anims/Anim")
    }

    anims: dict[str, SourceAnim] = {}
    for name in ANIMATIONS:
        node, sheet_name = nodes.get(name), name
        for _ in range(4):
            if node is None or node.find("CopyOf") is None:
                break
            sheet_name = node.findtext("CopyOf", "")
            node = nodes.get(sheet_name)
        if node is None or node.find("FrameWidth") is None:
            continue

        sheet = sprite_dir / f"{sheet_name}-Anim.png"
        if not sheet.is_file():
            continue

        anims[name] = SourceAnim(
            sheet=sheet,
            frame_width=int(node.findtext("FrameWidth", "0")),
            frame_height=int(node.findtext("FrameHeight", "0")),
            durations=[int(d.text or "0") for d in node.iterfind("./Durations/Duration")],
        )
    return anims


def opaque_bounds(frame: Image.Image) -> tuple[int, int]:
    """Returns (top, foot): the first opaque row and one past the last opaque row."""
    alpha = frame.getchannel("A").point(lambda a: 255 if a > OPAQUE_ALPHA else 0)
    box = alpha.getbbox()
    if box is None:
        return 0, frame.height
    return box[1], box[3]


def build_species(dex: int, name: str, sprite_dir: Path, out_dir: Path) -> bool:
    anims = read_anims(sprite_dir)
    if set(anims) != set(ANIMATIONS):
        return False

    rows: list[Image.Image] = []
    manifest_anims: dict[str, dict] = {}
    y = 0
    for anim_name, directions in ANIMATIONS.items():
        anim = anims[anim_name]
        sheet = Image.open(anim.sheet).convert("RGBA")
        row_width = anim.frame_width * len(anim.durations)
        single_row = sheet.height < anim.frame_height * 2
        if sheet.width < row_width:
            return False

        row_offsets: dict[str, int] = {}
        first_row = len(rows)
        for direction in directions:
            optional = direction in OPTIONAL_DIRECTIONS
            # A single-row sheet only faces down, so it has no back view.
            if single_row and optional:
                continue
            source_row = 0 if single_row else DIRECTION_ROW[direction]
            top = source_row * anim.frame_height
            if top + anim.frame_height > sheet.height:
                if optional:
                    continue
                return False
            rows.append(sheet.crop((0, top, row_width, top + anim.frame_height)))
            row_offsets[direction] = y
            y += anim.frame_height

        first_frame = rows[first_row].crop(
            (0, 0, anim.frame_width, anim.frame_height)
        )
        head, foot = opaque_bounds(first_frame)
        manifest_anims[anim_name] = {
            "w": anim.frame_width,
            "h": anim.frame_height,
            "durations": anim.durations,
            "top": head,
            "foot": foot,
            "rows": row_offsets,
        }

    atlas = Image.new("RGBA", (max(row.width for row in rows), y), (0, 0, 0, 0))
    y = 0
    for row in rows:
        atlas.paste(row, (0, y))
        y += row.height

    atlas.save(out_dir / f"{dex}.webp", "WEBP", lossless=True, quality=100, method=6)
    manifest = {"id": dex, "name": name, "anims": manifest_anims}
    (out_dir / f"{dex}.json").write_text(
        json.dumps(manifest, separators=(",", ":"), ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    return True


def display_names(tracker: dict, max_id: int) -> dict[int, str]:
    """Display name per dex number, failing on an identifier with no known spelling."""
    names: dict[int, str] = {}
    unknown: list[str] = []
    for dex in range(1, max_id + 1):
        tracked = tracker.get(f"{dex:04d}", {}).get("name") or f"#{dex}"
        names[dex] = DISPLAY_NAMES.get(tracked, tracked)
        if "_" in names[dex]:
            unknown.append(tracked)
    if unknown:
        raise SystemExit(f"add these tracker names to DISPLAY_NAMES: {unknown}")
    return names


def read_credit_names(source: Path) -> dict[str, str]:
    names: dict[str, str] = {}
    lines = (source / "credit_names.txt").read_text(encoding="utf-8").splitlines()
    for line in lines[1:]:
        parts = line.split("\t")
        if len(parts) >= 2:
            names[parts[1]] = parts[0] or parts[1]
    return names


def read_credits(sprite_dir: Path, credit_names: dict[str, str]) -> list[str]:
    """Authors who contributed at least one of the animations the buddy uses."""
    credits_file = sprite_dir / "credits.txt"
    if not credits_file.is_file():
        return []

    authors: list[str] = []
    for line in credits_file.read_text(encoding="utf-8").splitlines():
        parts = line.split("\t")
        if len(parts) < 2:
            continue
        contributed = set(parts[4].split(",")) if len(parts) >= 5 else set(ANIMATIONS)
        if not contributed & set(ANIMATIONS):
            continue
        author = credit_names.get(parts[1], parts[1])
        if author not in authors:
            authors.append(author)
    return authors


def sprites_version(out_dir: Path) -> str:
    """Short hash of every atlas and manifest. The app adds it to the asset URLs,
    so a browser cache never pairs an old manifest with a new atlas."""
    digest = hashlib.sha256()
    for path in sorted(out_dir.iterdir()):
        if path.suffix in {".json", ".webp"}:
            digest.update(path.name.encode())
            digest.update(path.read_bytes())
    return digest.hexdigest()[:10]


def source_revision(source: Path) -> str:
    try:
        return subprocess.run(
            ["git", "-C", str(source), "rev-parse", "HEAD"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--out", type=Path, default=APP_DIR / "public" / "buddy")
    parser.add_argument(
        "--species-module",
        type=Path,
        default=APP_DIR / "src" / "lib" / "buddy" / "species.generated.ts",
    )
    parser.add_argument("--max-id", type=int, default=649)
    args = parser.parse_args()

    tracker = json.loads((args.source / "tracker.json").read_text(encoding="utf-8"))
    names = display_names(tracker, args.max_id)
    credit_names = read_credit_names(args.source)

    if args.out.exists():
        shutil.rmtree(args.out)
    args.out.mkdir(parents=True)

    species: list[tuple[int, str]] = []
    credits: list[str] = []
    skipped: list[int] = []
    for dex in range(1, args.max_id + 1):
        key = f"{dex:04d}"
        sprite_dir = args.source / "sprite" / key
        name = names[dex]
        if not (sprite_dir / "AnimData.xml").is_file() or not build_species(
            dex, name, sprite_dir, args.out
        ):
            skipped.append(dex)
            continue
        species.append((dex, name))
        authors = ", ".join(read_credits(sprite_dir, credit_names)) or "Unknown"
        credits.append(f"- #{key} {name}: {authors}")

    revision = source_revision(args.source)
    (args.out / "CREDITS.md").write_text(
        "\n".join(
            [
                "# Timer buddy sprite credits",
                "",
                "Sprites come from [PMD SpriteCollab](https://github.com/PMDCollab/SpriteCollab)",
                f"(revision `{revision}`), licensed under",
                "[CC BY-NC 4.0](https://creativecommons.org/licenses/by-nc/4.0/). They were",
                "cropped and repacked into one atlas per Pokemon by",
                "`app/scripts/build_buddy_sprites.py`.",
                "",
                "Pokemon and all related characters are trademarks of Nintendo, Game Freak and",
                "The Pokemon Company. This is a non-commercial fan use and is not affiliated",
                "with or endorsed by them. Sprites credited to CHUNSOFT are original game",
                "assets.",
                "",
                *credits,
                "",
            ]
        ),
        encoding="utf-8",
    )

    entries = "\n".join(
        f"  [{dex}, {json.dumps(name, ensure_ascii=False)}]," for dex, name in species
    )
    args.species_module.parent.mkdir(parents=True, exist_ok=True)
    args.species_module.write_text(
        "// Generated by app/scripts/build_buddy_sprites.py. Do not edit by hand.\n"
        "\n"
        "/** Pokemon with a complete buddy sprite atlas, as [dex number, name]. */\n"
        "export const BUDDY_SPECIES: ReadonlyArray<readonly [number, string]> = [\n"
        f"{entries}\n"
        "];\n"
        "\n"
        "/** Changes whenever an atlas or manifest changes. Added to the asset URLs. */\n"
        f'export const BUDDY_SPRITES_VERSION = "{sprites_version(args.out)}";\n',
        encoding="utf-8",
    )

    total_bytes = sum(f.stat().st_size for f in args.out.iterdir())
    print(f"built {len(species)} species, {total_bytes / 1_000_000:.1f} MB in {args.out}")
    print(f"skipped {len(skipped)}: {skipped}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
