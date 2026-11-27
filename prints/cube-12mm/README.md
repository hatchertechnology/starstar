# 12 mm test cube

A 12 mm cube with a 0.5 mm bottom chamfer (against elephant's foot) and "SS"
debossed 0.6 mm on top. Sliced for MK4, MK4S and MINI with Prusament PLA at
0.20 mm (print profile `0.20mm SPEED`). The MK4 file was uploaded to `mk4-2`
through the starstar CLI (not printed).

## Files

| File | What |
|---|---|
| `cube.py` | CadQuery model (PEP 723 header, run with `uv`) |
| `cube-12mm.step`, `cube-12mm.stl` | Model exports |
| `cube-12mm.MK4.bgcode`, `.ini` | Binary G-code and flat slicer config for MK4 (standard 0.4 mm nozzle; printer model MK4IS). Includes preview thumbnails, see below |
| `cube-12mm.MK4S.bgcode`, `.ini` | Binary G-code and the flat slicer config for MK4S (0.4 mm high-flow nozzle) |
| `cube-12mm.MINI.bgcode`, `.ini` | Same for MINI & MINI+ (0.4 mm nozzle) |

## Model check

`uv run cube.py` prints: valid = True, 1 solid, volume 1714.71 mm³,
bounding box 12.000 x 12.000 x 12.000 mm (measured on the tessellation; the
raw BREP bounding box is loose on curved text faces).

## Slicer estimates (PrusaSlicer 2.9.7)

| Printer | Filament | Nozzle / bed | Time | Filament |
|---|---|---|---|---|
| MK4 (0.4) | Prusament PLA @PGIS | 230 / 60 C (225 after layer 1) | 8m 57s | 1.11 g, 371.6 mm |
| MK4S (HF 0.4) | Prusament PLA @MK4S HF0.4 | 230 / 60 C | 7m 1s | 1.11 g, 371.6 mm |
| MINI & MINI+ | Prusament PLA | 215 / 60 C | 12m 21s | 1.24 g, 416.8 mm |

Use the MK4 file for a plain MK4 (starstar printer kind `mk4`). The MK4S file
will not match it: its G-code checks for model MK4S and a high-flow nozzle.

## Regenerate

`S` is the prusaslicer skill script
(`<cad-skills>/skills/prusaslicer/scripts/slice.py`).

```bash
cd prints/cube-12mm
uv run cube.py
uv run $S slice cube-12mm.stl --printer MK4 --filament "Prusament PLA" \
    --print "0.20mm SPEED" -o cube-12mm.MK4.bgcode
uv run $S slice cube-12mm.stl --printer MK4S --filament "Prusament PLA" \
    --print "0.20mm SPEED" -o cube-12mm.MK4S.bgcode
uv run $S slice cube-12mm.stl --printer "Original Prusa MINI & MINI+" \
    --filament "Prusament PLA" --print "0.20mm SPEED" -o cube-12mm.MINI.bgcode
```

The `.ini` next to each G-code reproduces the exact slice.

## Thumbnails

The PrusaSlicer CLI does not render G-code thumbnails, so the MK4 screen
showed a blank preview. The MK4 file has four thumbnail blocks added after
slicing (16x16, 313x173 and 480x240 QOI, 380x285 PNG). They were rendered from
`cube.py` and inserted after the printer metadata block, with CRC32 checksums.
The G-code and metadata blocks are unchanged. The MK4S and MINI files still
have no thumbnails.
