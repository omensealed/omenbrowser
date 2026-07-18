#!/usr/bin/env python3
"""Isolated pinned-Python Reticulum peer for the full IFAC transport test."""

import argparse
import importlib.metadata
import json
import os
import pathlib
import subprocess
import sys
import threading
import time

PINNED_REF = "15320e4d2cfabb143c1db20ca887e275fd521585"
NETWORK_NAME = "omen-ifac-vector"
PASSPHRASE = "public-test-fixture"
OLD_REQUEST = b"rust-link-data-old-attempt"
RETRY_REQUEST = b"rust-link-data-retry"
RESPONSE = b"python-link-data"


def verify_source(source: pathlib.Path) -> None:
    expected_version = os.environ.get("OMEN_PYTHON_RNS_VERSION")
    if expected_version is not None:
        sys.path.insert(0, str(source))
        actual_version = importlib.metadata.version("rns")
        if actual_version != expected_version:
            raise RuntimeError(
                f"expected Python RNS {expected_version}, found {actual_version}"
            )
        return

    head = subprocess.check_output(
        ["git", "-C", str(source), "rev-parse", "HEAD"], text=True
    ).strip()
    if head != PINNED_REF:
        raise RuntimeError(f"expected pinned Reticulum {PINNED_REF}, found {head}")
    dirty = subprocess.check_output(
        ["git", "-C", str(source), "status", "--porcelain"], text=True
    )
    if dirty:
        raise RuntimeError("pinned Reticulum source checkout is dirty")


def write_config(root: pathlib.Path, port: int) -> pathlib.Path:
    config_dir = root / "config"
    storage_dir = root / "storage"
    config_dir.mkdir(parents=True)
    storage_dir.mkdir(parents=True)
    (config_dir / "config").write_text(
        f"""[reticulum]
  enable_transport = No
  share_instance = No
  instance_control_port = 0
  panic_on_interface_error = Yes

[logging]
  loglevel = 1

[interfaces]
  [[Pinned IFAC Server]]
    type = TCPServerInterface
    enabled = Yes
    listen_ip = 127.0.0.1
    listen_port = {port}
    network_name = {NETWORK_NAME}
    passphrase = {PASSPHRASE}
""",
        encoding="utf-8",
    )
    return config_dir


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--rns-source", required=True, type=pathlib.Path)
    parser.add_argument("--root", required=True, type=pathlib.Path)
    parser.add_argument("--port", required=True, type=int)
    args = parser.parse_args()

    verify_source(args.rns_source)
    config_dir = write_config(args.root, args.port)
    sys.path.insert(0, str(args.rns_source))
    import RNS  # pylint: disable=import-outside-toplevel

    reticulum = None
    completed = threading.Event()
    proof_sequence_completed = threading.Event()
    result = {
        "links": 0,
        "received": False,
        "replied": False,
        "old_attempt_deferred": False,
        "forged_proof_sent": False,
        "stale_proof_sent": False,
        "valid_proof_sent": False,
    }

    def packet_received(data, packet):
        with proof_lock:
            if bytes(data) == OLD_REQUEST:
                deferred_packet[:] = [packet]
                result["old_attempt_deferred"] = True
                return
            if bytes(data) != RETRY_REQUEST or not deferred_packet:
                return
            result["received"] = True
            forged_hash = bytes([packet.packet_hash[0] ^ 0x01]) + packet.packet_hash[1:]
            forged = RNS.Packet(
                active_link[0], forged_hash + bytes(64), RNS.Packet.PROOF
            )
            forged.send()
            result["forged_proof_sent"] = True
            deferred_packet[0].prove()
            result["stale_proof_sent"] = True
            packet.prove()
            result["valid_proof_sent"] = True
            proof_sequence_completed.set()
            RNS.Packet(active_link[0], RESPONSE).send()
            result["replied"] = True
            completed.set()

    def link_established(link):
        result["links"] += 1
        active_link[:] = [link]
        link.set_packet_callback(packet_received)

    active_link = []
    deferred_packet = []
    proof_lock = threading.Lock()
    try:
        reticulum = RNS.Reticulum(configdir=str(config_dir), loglevel=RNS.LOG_ERROR)
        identity = RNS.Identity.from_bytes(bytes(range(64)))
        destination = RNS.Destination(
            identity, RNS.Destination.IN, RNS.Destination.SINGLE, "omeninterop", "link"
        )
        destination.set_proof_strategy(RNS.Destination.PROVE_NONE)
        destination.set_link_established_callback(link_established)
        print(
            json.dumps(
                {
                    "ready": True,
                    "destination": destination.hash.hex(),
                    "identity": identity.hash.hex(),
                    "port": args.port,
                }
            ),
            flush=True,
        )
        if not completed.wait(20):
            raise TimeoutError("Rust peer did not complete link-data exchange")
        if not proof_sequence_completed.wait(2):
            raise TimeoutError("Python peer did not complete forged/stale/current proof sequence")
        time.sleep(0.15)
        print(json.dumps(result), flush=True)
        return 0
    finally:
        if reticulum is not None:
            RNS.Reticulum.exit_handler()


if __name__ == "__main__":
    raise SystemExit(main())
