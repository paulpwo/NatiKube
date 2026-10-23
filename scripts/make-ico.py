# Packs 16/24/32/48/64/128/256 px PNGs (named <size>.png in DIR) into an .ico.
# Render them first, e.g.: sips -z 48 48 assets/icon/natikube-1024.png --out DIR/48.png
# Usage: python3 -I scripts/make-ico.py DIR assets/icon/natikube.ico
import struct, sys, os
d, out = sys.argv[1], sys.argv[2]
sizes = [16, 24, 32, 48, 64, 128, 256]
imgs = [open(os.path.join(d, f"{s}.png"), "rb").read() for s in sizes]
hdr = struct.pack("<HHH", 0, 1, len(sizes))
off = 6 + 16 * len(sizes)
ents = b""
for s, b in zip(sizes, imgs):
    w = 0 if s == 256 else s
    ents += struct.pack("<BBBBHHII", w, w, 0, 0, 1, 32, len(b), off)
    off += len(b)
open(out, "wb").write(hdr + ents + b"".join(imgs))
