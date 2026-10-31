"""Losslessly subroutinize a static four-region CFF2 collection for packaging.

Requires fonttools==4.68.0 and cffsubr==0.4.0 (build-time only).
"""

import argparse
import hashlib
import io
from pathlib import Path
import zlib

from cffsubr import subroutinize
from fontTools.pens.recordingPen import RecordingPen
from fontTools.ttLib import TTCollection, TTFont
from fontTools.ttLib.tables.DefaultTable import DefaultTable


REGIONS = ("SC", "JP", "KR", "TC")
DRAW_OPERATORS = {
    "rmoveto", "hmoveto", "vmoveto", "rlineto", "hlineto", "vlineto",
    "rrcurveto", "hhcurveto", "vvcurveto", "hvcurveto", "vhcurveto",
    "rlinecurve", "rcurveline", "callsubr", "callgsubr", "return",
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def table_bytes(font, tag):
    data = font.getTableData(tag)
    if tag == "head":
        data = data[:8] + bytes(4) + data[12:]
    return data


def verify_cff(original, optimized):
    require(original.getGlyphOrder() == optimized.getGlyphOrder(), "glyph order changed")
    left, right = [f["CFF2"].cff.topDictIndex[0] for f in (original, optimized)]
    require(left.FDSelect.gidArray == right.FDSelect.gidArray, "FDSelect changed")
    offsets = {"CharStrings", "FDArray", "FDSelect"}
    require(
        {k: v for k, v in left.rawDict.items() if k not in offsets}
        == {k: v for k, v in right.rawDict.items() if k not in offsets},
        "CFF top parameters changed",
    )
    require(len(left.FDArray) == len(right.FDArray), "FDArray count changed")
    for a, b in zip(left.FDArray, right.FDArray):
        require(
            {k: v for k, v in a.rawDict.items() if k != "Private"}
            == {k: v for k, v in b.rawDict.items() if k != "Private"},
            "font dictionary parameters changed",
        )
        require(
            {k: v for k, v in a.Private.rawDict.items() if k != "Subrs"}
            == {k: v for k, v in b.Private.rawDict.items() if k != "Subrs"},
            "hint parameters changed",
        )
    # This verifier deliberately refuses hinted/variable programs rather than
    # assuming that outline equality alone proves their equivalence.
    from fontTools.cffLib.transforms import desubroutinize

    for top, font in ((left, original), (right, optimized)):
        desubroutinize(font["CFF2"].cff)
        for cs in top.CharStrings.values():
            cs.decompile()
            require(
                all(not isinstance(v, str) or v in DRAW_OPERATORS for v in cs.program),
                "unsupported hint, variation or arithmetic operator",
            )
    for name in original.getGlyphOrder():
        pens = [RecordingPen(), RecordingPen()]
        for top, pen in zip((left, right), pens):
            top.CharStrings[name].draw(pen)
        require(pens[0].value == pens[1].value, f"outline changed: {name}")
    print(f"Verified all {len(original.getGlyphOrder())} glyph outlines and private parameters", flush=True)


def verify_collection(source, candidate):
    before = TTCollection(source)
    after = TTCollection(candidate)
    require(len(before.fonts) == len(after.fonts) == 4, "expected four faces")
    for region, a, b in zip(REGIONS, before.fonts, after.fonts):
        require(region in a["name"].getDebugName(1).split(), f"wrong {region} face")
        require("fvar" not in a and "CFF2" in a, "expected static CFF2")
        require(set(a.keys()) == set(b.keys()), "table set changed")
        for tag in a.keys():
            if tag not in ("GlyphOrder", "CFF2"):
                require(table_bytes(a, tag) == table_bytes(b, tag), f"{region}: {tag} changed")
        print(f"{region}: all mapping, layout, metrics and metadata tables identical", flush=True)
    require(len({f.getTableData("CFF2") for f in before.fonts}) == 1, "source outlines not shared")
    require(len({f.getTableData("CFF2") for f in after.fonts}) == 1, "output outlines not shared")
    verify_cff(before.fonts[0], after.fonts[0])


def optimize(source, output):
    require(source.resolve() != output.resolve(), "source must not be overwritten")
    require(not output.exists(), "output already exists")
    collection = TTCollection(source)
    require(len(collection.fonts) == 4, "expected four faces")
    require(len({f.getTableData("CFF2") for f in collection.fonts}) == 1, "source outlines not shared")
    buffer = io.BytesIO()
    collection.fonts[0].recalcTimestamp = False
    collection.fonts[0].save(buffer)
    font = TTFont(io.BytesIO(buffer.getvalue()), recalcTimestamp=False)
    subroutinize(font)
    compressed = font.getTableData("CFF2")
    require(len(compressed) < len(collection.fonts[0].getTableData("CFF2")), "no size improvement")
    for face in collection.fonts:
        face.recalcTimestamp = False
        table = DefaultTable("CFF2")
        table.data = compressed
        face["CFF2"] = table
    buffer = io.BytesIO()
    collection.save(buffer, shareTables=True)
    candidate = buffer.getvalue()
    verify_collection(source, io.BytesIO(candidate))
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_bytes(candidate)
    for label, data in (("before", source.read_bytes()), ("after", candidate)):
        print(f"{label}: bytes={len(data)}, deflate9={len(zlib.compress(data, 9))}, sha256={hashlib.sha256(data).hexdigest()}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    optimize(args.source, args.output)
