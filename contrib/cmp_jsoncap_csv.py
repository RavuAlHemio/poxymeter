#!/usr/bin/env python3
#
# compares the data from a Wireshark .json capture export
# with a CSV file exported by SpO2 Assistant
#
import json
import sys


def main():
    if len(sys.argv) < 2:
        print("Usage: cmp_jsoncap_csv.py WIRESHARK_JSON SPO2_CSV", file=sys.stderr)
        sys.exit(1)

    with open(sys.argv[1], "r", encoding="utf-8") as f:
        cap = json.load(f)

    with open(sys.argv[2], "r", encoding="utf-8") as f:
        csv_rows = []
        for (i, row) in enumerate(f.readlines()):
            if i == 0:
                continue
            csv_rows.append(row.split(", "))

    byte_lists = []

    for entry in cap:
        byte_string: str = entry["_source"]["layers"]["usbhid.data"]
        entry_bytes = bytes(int(b, 16) for b in byte_string.split(":"))
        byte_lists.append(entry_bytes)

    all_bytes = b"".join(byte_lists)

    # assume it's all d2/d3 packets
    csv_offset = 0
    for i in range(0, len(all_bytes), 20):
        byte_slice = all_bytes[i:i+20]
        if len(byte_slice) < 20:
            continue

        command_byte = byte_slice[0]
        sequence_bytes = byte_slice[1:3]
        data_bytes = byte_slice[3:19]
        checksum_byte = byte_slice[19]

        sequence_bytes_str = " ".join(f"{b:02x}" for b in sequence_bytes)
        data_bytes_str = " ".join(f"{b:02x}" for b in data_bytes)

        print(f"| {command_byte:02x} | {sequence_bytes_str} | {data_bytes_str} | {checksum_byte:02x}")

        done_bytes = []
        if all(db == 0x7F for db in data_bytes):
            # invalid data
            done_bytes.extend(27 * [0x7F])
        else:
            # a chunk of data is two flag bytes one base value byte, and 16 bytes with deltas
            flag_bytes = data_bytes[0:2]
            value_byte = data_bytes[2]
            delta_bytes = data_bytes[3:]

            # first off, the flags are actually a little-endian integer
            # protocol forbids the top bit being set, leaving 14 bits
            # (top bit is only for the first byte in a command/response)
            # since there are only 13 nibbles after the MSB, ignore the remaining bottom one
            # this gives us the following structure: 0sssssss 0ssssss?
            top_nibble_signs = ((flag_bytes[1] << 6) | (flag_bytes[0] >> 1))
            print(f"signs: {top_nibble_signs:013b}")

            # the initial value byte is part of the output
            done_bytes.append(value_byte)

            # deltas are stored as nibbles
            for bi, b in enumerate(delta_bytes):
                top_nibble = (b >> 4) & 0x0F
                bottom_nibble = (b >> 0) & 0x0F

                # TODO: 0xF is an invalid value
                # handle it... somehow

                # see comment above top_nibble_signs definition for what is going on here
                if (top_nibble_signs & (1 << bi)) != 0:
                    top_nibble |= 0b1000

                #print(f"TN {top_nibble:04b}, BN {bottom_nibble:04b}")

                if (top_nibble & 0b1000) != 0:
                    value_byte -= (top_nibble & 0b0111)
                else:
                    value_byte += (top_nibble & 0b0111)
                done_bytes.append(value_byte)

                # bottom nibble is more straightforward
                if (bottom_nibble & 0b1000) != 0:
                    value_byte -= (bottom_nibble & 0b0111)
                else:
                    value_byte += (bottom_nibble & 0b0111)
                done_bytes.append(value_byte)

        for done_byte in done_bytes:
            csv_byte = int(csv_rows[csv_offset][3].strip())
            if csv_byte != done_byte:
                print(f"{csv_offset} | CSV: {csv_byte} | USB: {done_byte}")
            csv_offset += 1


if __name__ == "__main__":
    main()
