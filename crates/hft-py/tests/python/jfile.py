"""Byte-level edits of journal segment copies for the corruption and schema-version cases
(layout: docs/subsystems/journal-format.md; record header: hft-event)."""

from __future__ import annotations

import struct
from pathlib import Path
from typing import Iterator

HEADER_LEN = 4096
HEADER_CRC_LEN = 3068
SCHEMA_OFF = 256  # SegmentHeader.schema[]: 128 × {kind u16, version u16, size u32}
N_SCHEMA_OFF = 248
FLAG_CRC = 1 << 5
FLAG_PART = 1 << 4

_POLY = 0x82F63B78
_TABLE: list[int] = []
for _i in range(256):
    _c = _i
    for _ in range(8):
        _c = (_c >> 1) ^ _POLY if _c & 1 else _c >> 1
    _TABLE.append(_c)


def crc32c(b: bytes) -> int:
    c = 0xFFFFFFFF
    for x in b:
        c = _TABLE[(c ^ x) & 0xFF] ^ (c >> 8)
    return c ^ 0xFFFFFFFF


def fix_header_crc(raw: bytearray) -> None:
    raw[HEADER_CRC_LEN : HEADER_CRC_LEN + 4] = struct.pack("<I", crc32c(bytes(raw[:HEADER_CRC_LEN])))


class Rec:
    """One record of a segment file: offset, header fields and body."""

    def __init__(self, off: int, raw: bytes) -> None:
        self.off = off
        self.len, self.kind, self.flags, self.seq, self.tsc = struct.unpack_from("<IHHQQ", raw, off)

    @property
    def crc(self) -> bool:
        return bool(self.flags & FLAG_CRC)

    def body(self, raw: bytes) -> bytes:
        end = self.off + self.len - (4 if self.crc else 0)
        return raw[self.off + 24 : end]


def records(raw: bytes) -> Iterator[Rec]:
    off = HEADER_LEN
    while off + 24 <= len(raw):
        r = Rec(off, raw)
        if r.len == 0 or off + r.len > len(raw):
            return
        yield r
        off += r.len


def schema_entry(raw: bytes, kind: int) -> int:
    """Offset of `kind`'s entry in the header schema table."""
    n = struct.unpack_from("<H", raw, N_SCHEMA_OFF)[0]
    for i in range(n):
        off = SCHEMA_OFF + 8 * i
        if struct.unpack_from("<H", raw, off)[0] == kind:
            return off
    raise KeyError(kind)


def set_schema(raw: bytearray, kind: int, version: int, size: int) -> None:
    off = schema_entry(raw, kind)
    struct.pack_into("<HHI", raw, off, kind, version, size)
    fix_header_crc(raw)


def get_schema(raw: bytes, kind: int) -> tuple[int, int]:
    off = schema_entry(raw, kind)
    _, v, s = struct.unpack_from("<HHI", raw, off)
    return v, s


def resize_kind(raw: bytes, kind: int, new_body: int, fill: bytes = b"\0") -> bytearray:
    """Rewrites every record of `kind` with its body cut or padded to `new_body` bytes."""
    out = bytearray(raw[:HEADER_LEN])
    for r in records(raw):
        rec = raw[r.off : r.off + r.len]
        if r.kind == kind and not r.crc:
            body = r.body(raw)[:new_body]
            body = body + fill * (new_body - len(body))
            n = 24 + (len(body) + 7) // 8 * 8
            hdr = struct.pack("<IHHQQ", n, r.kind, r.flags, r.seq, r.tsc)
            rec = hdr + body + b"\0" * (n - 24 - len(body))
        out += rec
    return out


def set_record_crc(raw: bytearray, r: Rec) -> None:
    end = r.off + r.len - 4
    raw[end : end + 4] = struct.pack("<I", crc32c(bytes(raw[r.off : end])))


def segment_file(cold: Path) -> list[Path]:
    return sorted(cold.glob("*/*.jrn"))
