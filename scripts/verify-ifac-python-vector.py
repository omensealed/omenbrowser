#!/usr/bin/env python3
"""Generate or verify OMEN's IFAC vector with an identified Python RNS tree."""

from __future__ import annotations

import argparse
import importlib.metadata
import json
import subprocess
import sys
from pathlib import Path


PINNED_RETICULUM_REF = "e32d4df754a7b87b1bf1bb0d08675d12ff505ae6"
NETWORK_NAME = "omen-ifac-vector"
PASSPHRASE = "public-test-fixture"
IFAC_SIZE = 16
FIXED_PRIVATE_IDENTITY = bytes(range(64))
EXPECTED_IDENTITY_HASH = "aca31af0441d81dbec71e82da0b4b5f5"
EXPECTED_DESTINATIONS = {
    "nomadnetwork.node": {
        "name_hash": "213e6311bcec54ab4fde",
        "destination_hash": "8e484af42dd1c865a87fb2d16a5d8e63",
    },
    "lxmf.delivery": {
        "name_hash": "6ec60bc318e2c0f0d908",
        "destination_hash": "fae321c442e3c9bdcd7a3e79d850e03c",
    },
    "lxmf.propagation": {
        "name_hash": "e03a09b77ac21b22258e",
        "destination_hash": "809879e19dd239c50bf8cbf6a6bd4bae",
    },
    "omenchat.node": {
        "name_hash": "6962d95d0bb3bd5596ff",
        "destination_hash": "f24dd05da9d491e038fdfb3ee26a4959",
    },
}
RAW_PACKET = bytes.fromhex(
    "010200112233445566778899aabbccddeeff096f6d656e2d696661632d766563746f72"
)
EXPECTED_ENCODED = bytes.fromhex(
    "e22a38b18897fb59171f8f7ed906f0160b06ead3800df59254662fdcea8c9dcf"
    "8c27873d839b608fd30b202551e7c7781922a7"
)


def verify_source(rns_source: Path, expected_version: str | None) -> str:
    if expected_version is not None:
        sys.path.insert(0, str(rns_source))
        actual_version = importlib.metadata.version("rns")
        if actual_version != expected_version:
            raise SystemExit(
                "Python RNS package version differs from the current-drift pin: "
                f"expected={expected_version} actual={actual_version}"
            )
        return f"pypi:rns=={actual_version}"

    try:
        revision = subprocess.run(
            ["git", "-C", str(rns_source), "rev-parse", "HEAD"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        status = subprocess.run(
            ["git", "-C", str(rns_source), "status", "--porcelain", "--untracked-files=all"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    except (OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"cannot verify Python Reticulum source revision: {error}") from error

    if revision != PINNED_RETICULUM_REF:
        raise SystemExit(
            "Python Reticulum source is not the release-blocking pinned revision: "
            f"expected={PINNED_RETICULUM_REF} actual={revision}"
        )
    if status:
        raise SystemExit("Python Reticulum source tree has local or untracked changes")
    return PINNED_RETICULUM_REF


class CaptureInterface:
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


def python_vector(rns_source: Path) -> bytes:
    if not (rns_source / "RNS" / "Transport.py").is_file():
        raise SystemExit(f"not a Python Reticulum source root: {rns_source}")
    sys.path.insert(0, str(rns_source))
    import RNS  # pylint: disable=import-outside-toplevel

    interface = CaptureInterface(RNS)
    RNS.Transport.transmit(interface, RAW_PACKET)
    if interface.captured is None:
        raise SystemExit("pinned Python RNS did not emit an IFAC packet")
    return interface.captured


def compatibility_vectors(rns) -> dict[str, object]:
    identity = rns.Identity.from_bytes(FIXED_PRIVATE_IDENTITY)
    destinations = {}
    for full_name, expected in EXPECTED_DESTINATIONS.items():
        app_name, aspect = full_name.split(".", 1)
        name_hash = rns.Identity.full_hash(full_name.encode("utf-8"))[
            : rns.Identity.NAME_HASH_LENGTH // 8
        ].hex()
        destination_hash = rns.Destination.hash(identity, app_name, aspect).hex()
        actual = {"name_hash": name_hash, "destination_hash": destination_hash}
        if actual != expected:
            raise SystemExit(
                "pinned Python RNS destination vector differs from the reviewed fixture: "
                f"name={full_name} expected={expected} actual={actual}"
            )
        destinations[full_name] = actual

    identity_hash = identity.hash.hex()
    if identity_hash != EXPECTED_IDENTITY_HASH:
        raise SystemExit(
            "pinned Python RNS identity hash differs from the reviewed fixture: "
            f"expected={EXPECTED_IDENTITY_HASH} actual={identity_hash}"
        )
    return {"identity_hash": identity_hash, "destinations": destinations}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--rns-source", required=True, type=Path)
    parser.add_argument("--rns-version")
    args = parser.parse_args()
    source = args.rns_source.resolve()
    source_ref = verify_source(source, args.rns_version)
    encoded = python_vector(source)
    if encoded != EXPECTED_ENCODED:
        raise SystemExit(
            "pinned Python RNS IFAC output differs from the reviewed fixture: "
            f"expected={EXPECTED_ENCODED.hex()} actual={encoded.hex()}"
        )
    import RNS  # pylint: disable=import-outside-toplevel

    result = compatibility_vectors(RNS)
    result.update(
        {
            "source_ref": source_ref,
            "network_name": NETWORK_NAME,
            "ifac_size": IFAC_SIZE,
            "raw_hex": RAW_PACKET.hex(),
            "encoded_hex": encoded.hex(),
            "matched": True,
        }
    )
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
