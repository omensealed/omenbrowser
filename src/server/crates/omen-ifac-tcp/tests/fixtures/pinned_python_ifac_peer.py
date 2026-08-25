#!/usr/bin/env python3
"""Bounded TCP/IFAC peer using the pinned Python Reticulum implementation."""

from __future__ import annotations

import argparse
import importlib.metadata
import json
import os
import socket
import subprocess
import sys
import time
from pathlib import Path


PINNED_REF = "e32d4df754a7b87b1bf1bb0d08675d12ff505ae6"
NETWORK_NAME = "omen-ifac-vector"
PASSPHRASE = "public-test-fixture"
IFAC_SIZE = 16
MAX_FRAME_BYTES = 4096


def verify_source(source: Path) -> None:
    expected_version = os.environ.get("OMEN_PYTHON_RNS_VERSION")
    if expected_version is not None:
        sys.path.insert(0, str(source))
        actual_version = importlib.metadata.version("rns")
        if actual_version != expected_version:
            raise SystemExit(
                f"expected Python RNS {expected_version}, found {actual_version}"
            )
        return

    revision = subprocess.run(
        ["git", "-C", str(source), "rev-parse", "HEAD"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    status = subprocess.run(
        ["git", "-C", str(source), "status", "--porcelain", "--untracked-files=all"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    if revision != PINNED_REF:
        raise SystemExit(f"wrong pinned Python Reticulum revision: {revision}")
    if status:
        raise SystemExit("pinned Python Reticulum source is modified")


class IfacInterface:
    def __init__(self, rns):
        origin = rns.Identity.full_hash(NETWORK_NAME.encode("utf-8"))
        origin += rns.Identity.full_hash(PASSPHRASE.encode("utf-8"))
        origin_hash = rns.Identity.full_hash(origin)
        self.ifac_key = rns.Cryptography.hkdf(
            length=64,
            derive_from=origin_hash,
            salt=rns.Reticulum.IFAC_SALT,
            context=None,
        )
        self.ifac_identity = rns.Identity.from_bytes(self.ifac_key)
        self.ifac_size = IFAC_SIZE
        self.captured = None

    def process_outgoing(self, raw):
        self.captured = raw


def decode_ifac(rns, interface: IfacInterface, raw: bytes) -> bytes | None:
    # Mirrors the authenticated ingress transform in pinned RNS.Transport.inbound.
    if len(raw) <= 2 + interface.ifac_size or raw[0] & 0x80 != 0x80:
        return None
    ifac = raw[2 : 2 + interface.ifac_size]
    mask = rns.Cryptography.hkdf(
        length=len(raw), derive_from=ifac, salt=interface.ifac_key, context=None
    )
    unmasked = bytearray(raw)
    for index, byte in enumerate(unmasked):
        if index <= 1 or index > interface.ifac_size + 1:
            unmasked[index] = byte ^ mask[index]
    new_raw = bytes([unmasked[0] & 0x7F, unmasked[1]]) + bytes(
        unmasked[2 + interface.ifac_size :]
    )
    expected = interface.ifac_identity.sign(new_raw)[-interface.ifac_size :]
    return new_raw if ifac == expected else None


def encode_ifac(rns, interface: IfacInterface, raw: bytes) -> bytes:
    interface.captured = None
    rns.Transport.transmit(interface, raw)
    if interface.captured is None:
        raise RuntimeError("pinned Python Reticulum did not emit IFAC bytes")
    return interface.captured


def hdlc_frame(hdlc, raw: bytes) -> bytes:
    return bytes([hdlc.FLAG]) + hdlc.escape(raw) + bytes([hdlc.FLAG])


def receive_frame(connection: socket.socket, hdlc) -> bytes:
    buffer = bytearray()
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        chunk = connection.recv(1024)
        if not chunk:
            break
        buffer.extend(chunk)
        if len(buffer) > MAX_FRAME_BYTES:
            raise RuntimeError("HDLC frame exceeded test byte limit")
        start = buffer.find(bytes([hdlc.FLAG]))
        if start >= 0:
            end = buffer.find(bytes([hdlc.FLAG]), start + 1)
            if end >= 0:
                frame = bytes(buffer[start + 1 : end])
                frame = frame.replace(
                    bytes([hdlc.ESC, hdlc.FLAG ^ hdlc.ESC_MASK]), bytes([hdlc.FLAG])
                )
                return frame.replace(
                    bytes([hdlc.ESC, hdlc.ESC ^ hdlc.ESC_MASK]), bytes([hdlc.ESC])
                )
    raise RuntimeError("timed out waiting for bounded HDLC frame")


def raw_packet(marker: int) -> bytes:
    return bytes([0x01, 0x02]) + bytes(range(16)) + bytes([0x09, marker, 0x7E, 0x7D])


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--rns-source", required=True, type=Path)
    parser.add_argument("--mode", choices=("roundtrip", "wrong-credential"), required=True)
    args = parser.parse_args()
    source = args.rns_source.resolve()
    verify_source(source)
    sys.path.insert(0, str(source))
    import RNS  # pylint: disable=import-outside-toplevel
    from RNS.Interfaces.TCPInterface import HDLC  # pylint: disable=import-outside-toplevel

    interface = IfacInterface(RNS)
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind(("127.0.0.1", 0))
    listener.listen(1)
    listener.settimeout(12)
    print(json.dumps({"port": listener.getsockname()[1]}), flush=True)

    accepted = 0
    rejected = 0
    connections = 2 if args.mode == "roundtrip" else 1
    for connection_index in range(connections):
        connection, _ = listener.accept()
        connection.settimeout(8)
        with connection:
            encoded = receive_frame(connection, HDLC)
            decoded = decode_ifac(RNS, interface, encoded)
            if decoded is None:
                rejected += 1
            else:
                accepted += 1

            if args.mode == "wrong-credential":
                response = hdlc_frame(
                    HDLC, encode_ifac(RNS, interface, raw_packet(0xE0))
                )
                connection.sendall(response)
                time.sleep(0.1)
                continue

            if decoded is None:
                raise RuntimeError("correct-credential Rust packet failed Python IFAC validation")
            if connection_index == 0:
                split = hdlc_frame(
                    HDLC, encode_ifac(RNS, interface, raw_packet(0xA1))
                )
                midpoint = len(split) // 2
                connection.sendall(split[:midpoint])
                time.sleep(0.02)
                connection.sendall(split[midpoint:])
                coalesced = b"".join(
                    hdlc_frame(HDLC, encode_ifac(RNS, interface, raw_packet(marker)))
                    for marker in (0xA2, 0xA3)
                )
                connection.sendall(coalesced)
            else:
                connection.sendall(
                    hdlc_frame(HDLC, encode_ifac(RNS, interface, raw_packet(0xB1)))
                )

    listener.close()
    print(
        json.dumps(
            {
                "mode": args.mode,
                "connections": connections,
                "accepted": accepted,
                "rejected": rejected,
            },
            sort_keys=True,
        ),
        flush=True,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
