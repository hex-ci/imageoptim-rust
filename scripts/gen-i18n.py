#!/usr/bin/env python3
"""Generate crates/app/assets/i18n.json from upstream .lproj files."""
import json
import os
import re
import sys
from pathlib import Path

_env_dir = os.environ.get("UPSTREAM_LPROJ_DIR")
if len(sys.argv) > 1:
    UPSTREAM = Path(sys.argv[1])
elif _env_dir:
    UPSTREAM = Path(_env_dir)
else:
    sys.stderr.write("Usage: gen-i18n.py <path-to-ImageOptim.app/Contents/Resources-or-upstream-dir>\n")
    sys.exit(2)
OUT = Path(__file__).resolve().parent.parent / "crates/app/assets/i18n.json"

# XIB object id -> the English string used by this port.
IDS = {
    "238.headerCell.title": "File",
    "239.headerCell.title": "Size",
    "243.headerCell.title": "Savings",
    "771.headerCell.title": "Best tool",
    "795.headerCell.title": "Original Size",
    "239.headerToolTip": "Size of file after optimization",
    "632.title": "Drag and drop image files onto the area above",
    "635.title": "Again",
    "635.alternateTitle": "Run optimizations again",
    "423.ibShadowedToolTip": "Add",
    "423.ibExternalAccessibilityDescription": "Add",
    "363.title": "Preferences…",
    "437.title": "Add Files…",
    "593.title": "Optimize Again",
    "725.title": "Optimize Optimized",
    "402.title": "Delete",
    "721.title": "Delete Completed",
    "377.title": "Select All",
    "378.title": "Undo",
    "384.title": "Redo",
    "399.title": "Copy",
    "400.title": "Paste",
    "361.title": "About ImageOptim",
    "134.title": "Hide ImageOptim",
    "136.title": "Quit ImageOptim",
    "145.title": "Hide Others",
    "150.title": "Show All",
    "130.title": "Services",
    "526.title": "About ImageOptim",
    "506.title": "ImageOptim Help",
    "708.title": "Quick Look",
    "ddH-3k-EuT.title": "Show in Finder",
    "p0m-mn-Gep.title": "Show in Finder",
    "HUo-GO-MIU.title": "Stop",
    "FLl-C9-GLI.title": "Revert to Original",
    "NlY-Bc-7tu.title": "Copy as Data URL",
    "5.title": "ImageOptim Preferences",
    "167.label": "General",
    "168.label": "Optimization speed",
    "177.label": "Quality",
    "280.title": "Writing files to disk",
    "804.title": "Preserve file permissions, attributes and hardlinks",
    "818.title": "Optimization level",
    "819.title": "Fast",
    "820.title": "Insane",
    "821.title": "Normal",
    "822.title": "Extra",
    "828.title": "Make images interlaced (progressive)",
    "831.title": "Interlaced files are usually larger",
    "854.title": "Strip PNG metadata (gamma, color profiles, optional chunks)",
    "865.title": "Web browsers require gamma chunks to be removed",
    "868.title": "JPEG quality",
    "911.title": "Strip JPEG metadata (EXIF, color profiles, GPS, rotation, etc.)",
    "946.title": "Interrupt if takes too long to execute",
    "1043.title": "Metadata and color profiles",
    "1107.title": "Enable",
    "1122.title": "Saving to network drives is faster when permissions are not preserved",
    "1124.title": "Not recommended if you rely on embedded copyright information",
    "OIX-Pu-xXQ.title": "GIF quality",
    "LossTipTxt.title": "Makes files much smaller, but may change how images look",
    "kE2-GK-JIe.title": "PNG quality",
    "wBgxz7uj.title": "Enable lossy minification",
}

PAIR = re.compile(r'"((?:\\.|[^"\\])*)"\s*=\s*"((?:\\.|[^"\\])*)"\s*;', re.S)


def unescape(value: str) -> str:
    return (
        value.replace("\\n", "\n")
        .replace("\\r", "\r")
        .replace("\\t", "\t")
        .replace('\\"', '"')
        .replace("\\\\", "\\")
    )


def parse(path: Path) -> dict[str, str]:
    if not path.exists():
        return {}
    text = path.read_text(encoding="utf-8", errors="replace")
    return {unescape(k): unescape(v) for k, v in PAIR.findall(text)}


def main() -> None:
    data: dict[str, dict[str, str]] = {}
    for lproj in sorted(UPSTREAM.glob("*.lproj")):
        lang = lproj.stem
        if lang == "Base":
            continue
        table: dict[str, str] = {}
        for source, mapping in [
            ("ImageOptim.strings", IDS),
            ("PrefsController.strings", IDS),
            ("Localizable.strings", None),
        ]:
            parsed = parse(lproj / source)
            if mapping is None:
                for key, value in parsed.items():
                    norm_key = re.sub(r"%\d+\$@|%@", "{}", key)
                    norm_value = re.sub(r"%\d+\$@|%@", "{}", value)
                    table[norm_key] = norm_value
            else:
                for key, english in mapping.items():
                    if key in parsed:
                        table[english] = parsed[key]
        if lang.startswith("en"):
            table = {}
        if table:
            data[lang] = table
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(data, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")
    print(f"wrote {OUT} ({OUT.stat().st_size} bytes, {len(data)} languages)")


if __name__ == "__main__":
    main()
