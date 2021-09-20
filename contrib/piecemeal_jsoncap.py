#!/usr/bin/env python3
# Takes a JSON export of a Wireshark capture and attempts to peel apart the commands within it.
import json
import sys
from typing import Optional


def index_of_byte_with_top_bit(bs: bytes) -> Optional[int]:
    for i, b in enumerate(bs):
        if (b & 0x80) != 0:
            return i
    return None


def main():
    if len(sys.argv) != 2:
        print("Usage: piecemeal_jsoncap.py WIRESHARK_JSON", file=sys.stderr)
        sys.exit(1)

    with open(sys.argv[1], "r", encoding="utf-8") as f:
        cap = json.load(f)

    bytes_list = []

    for entry in cap:
        byte_string: str = entry["_source"]["layers"]["usbhid.data"]
        entry_bytes = bytes(int(b, 16) for b in byte_string.split(":")).rstrip(b"\x00")
        bytes_list.append(entry_bytes)

    my_bytes = b"".join(bytes_list)
    i = 0

    while i < len(my_bytes):
        next_cmd_start = index_of_byte_with_top_bit(my_bytes[i+1:])
        if next_cmd_start is None:
            byte_slice = my_bytes[i:]
            i = len(my_bytes)
        else:
            next_cmd_start += i + 1
            byte_slice = my_bytes[i:next_cmd_start]
            i = next_cmd_start
        print(" ".join(f"{b:02x}" for b in byte_slice))


if __name__ == "__main__":
    main()
