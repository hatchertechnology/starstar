# /// script
# requires-python = ">=3.10,<3.13"
# dependencies = ["cadquery"]
# ///
"""12 mm test cube: 0.5 mm bottom chamfer, "SS" debossed on top.

Run: uv run cube.py   (writes cube-12mm.step and cube-12mm.stl next to it)
"""
from pathlib import Path

import cadquery as cq

SIZE = 12.0  # mm, cube edge
CHAMFER = 0.5  # mm, bottom edge chamfer (elephant's foot)
TEXT = "SS"
TEXT_SIZE = 6.0  # mm, font height
TEXT_DEPTH = 0.6  # mm, deboss depth

assert CHAMFER < SIZE / 2 and TEXT_DEPTH < SIZE

result = (
    cq.Workplane("XY")
    .box(SIZE, SIZE, SIZE, centered=(True, True, False))
    .faces("<Z")
    .chamfer(CHAMFER)
    .faces(">Z")
    .workplane(centerOption="CenterOfBoundBox")
    .text(TEXT, TEXT_SIZE, -TEXT_DEPTH, combine="cut", halign="center", valign="center")
)

if __name__ == "__main__":
    out = Path(__file__).parent
    cq.exporters.export(result, str(out / "cube-12mm.step"))
    cq.exporters.export(result, str(out / "cube-12mm.stl"))
    s = result.val()
    # BREP bbox is loose on curved text faces, so measure the tessellation.
    verts, _ = s.tessellate(0.01)
    xs, ys, zs = zip(*((v.x, v.y, v.z) for v in verts))
    print(f"valid={s.isValid()} solids={len(result.solids().vals())} volume={s.Volume():.2f}")
    print(f"bbox={max(xs)-min(xs):.3f} x {max(ys)-min(ys):.3f} x {max(zs)-min(zs):.3f}")
