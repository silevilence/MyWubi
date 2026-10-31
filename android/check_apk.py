"""Verify every packaged native library's ABI, ELF LOAD and APK ZIP alignment."""
import argparse
import hashlib
import json
import struct
import zipfile
from pathlib import Path


def inspect(path: Path) -> dict:
    libraries = []
    with zipfile.ZipFile(path) as archive, path.open("rb") as raw:
        for item in archive.infolist():
            if not item.filename.endswith(".so"):
                continue
            data = archive.read(item)
            if data[:6] != b"\x7fELF\x02\x01":
                raise ValueError(f"{item.filename}: expected little-endian ELF64")
            machine = struct.unpack_from("<H", data, 18)[0]
            abi = item.filename.split("/")[1]
            if machine != {"arm64-v8a": 183, "x86_64": 62}.get(abi):
                raise ValueError(f"{item.filename}: incorrect ELF architecture {machine}")
            phoff = struct.unpack_from("<Q", data, 32)[0]
            phsize, phnum = struct.unpack_from("<HH", data, 54)
            loads = []
            for index in range(phnum):
                offset = phoff + index * phsize
                kind, _, file_offset, address, _, _, _, align = struct.unpack_from("<IIQQQQQQ", data, offset)
                if kind == 1:
                    if align < 16384 or (file_offset - address) % 16384:
                        raise ValueError(f"{item.filename}: LOAD segment incompatible with 16 KiB")
                    loads.append(align)
            if not loads:
                raise ValueError(f"{item.filename}: no LOAD segment")
            raw.seek(item.header_offset + 26)
            name_size, extra_size = struct.unpack("<HH", raw.read(4))
            data_offset = item.header_offset + 30 + name_size + extra_size
            if item.compress_type == zipfile.ZIP_STORED and data_offset % 16384:
                raise ValueError(f"{item.filename}: uncompressed library is not ZIP aligned")
            libraries.append({"name": item.filename, "bytes": len(data), "elf_load_alignments": loads,
                              "zip_offset": data_offset, "compressed": item.compress_type != zipfile.ZIP_STORED,
                              "sha256": hashlib.sha256(data).hexdigest()})
        names = {item["name"] for item in libraries}
        for abi in ("arm64-v8a", "x86_64"):
            if f"lib/{abi}/libmywubi_android.so" not in names:
                raise ValueError(f"Missing required ABI: {abi}")
        if not any(name.startswith("assets/licenses/") for name in archive.namelist()):
            raise ValueError("Missing bundled dictionary licenses")
    return {"apk": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest(), "libraries": libraries}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("apk", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    report = json.dumps(inspect(args.apk), indent=2)
    if args.output:
        args.output.write_text(report + "\n", encoding="utf-8")
    print(report)
