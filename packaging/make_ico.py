"""Builds a Windows .ico from PNG files (each embedded as-is, supported since Windows Vista).

Usage: python3 make_ico.py out.ico 16.png 32.png 256.png ...
Uses only the standard library.
"""

import struct
import sys


def png_size(data: bytes) -> tuple[int, int]:
    # Width and height are big-endian u32s in the IHDR chunk, right after the signature.
    return struct.unpack(">II", data[16:24])


def main() -> None:
    out_path, png_paths = sys.argv[1], sys.argv[2:]
    images = [open(path, "rb").read() for path in png_paths]

    header = struct.pack("<HHH", 0, 1, len(images))  # reserved, type 1 = icon, count
    directory = b""
    offset = len(header) + 16 * len(images)
    for data in images:
        width, height = png_size(data)
        # 256 px is written as 0 in the one-byte size fields.
        directory += struct.pack(
            "<BBBBHHII", width % 256, height % 256, 0, 0, 1, 32, len(data), offset
        )
        offset += len(data)

    with open(out_path, "wb") as out:
        out.write(header + directory + b"".join(images))


if __name__ == "__main__":
    main()
