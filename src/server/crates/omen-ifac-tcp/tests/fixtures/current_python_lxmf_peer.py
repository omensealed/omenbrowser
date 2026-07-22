#!/usr/bin/env python3
"""Isolated current-Python LXMF direct-delivery peer for OMEN interoperability."""

from __future__ import annotations

import argparse
import importlib.metadata
import json
import pathlib
import sys
import threading


RNS_VERSION = "1.4.0"
LXMF_VERSION = "1.1.0"
NETWORK_NAME = "omen-ifac-vector"
PASSPHRASE = "public-test-fixture"


def verify_packages(source: pathlib.Path) -> None:
    sys.path.insert(0, str(source))
    actual_rns = importlib.metadata.version("rns")
    actual_lxmf = importlib.metadata.version("lxmf")
    if actual_rns != RNS_VERSION or actual_lxmf != LXMF_VERSION:
        raise RuntimeError(
            "current Python stack differs from drift pins: "
            f"rns={actual_rns} lxmf={actual_lxmf}"
        )


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
  [[Current LXMF IFAC Server]]
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


def text(value: object) -> str:
    if isinstance(value, bytes):
        return value.decode("utf-8")
    return str(value)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--rns-source", required=True, type=pathlib.Path)
    parser.add_argument("--root", required=True, type=pathlib.Path)
    parser.add_argument("--port", required=True, type=int)
    parser.add_argument("--source", required=True)
    args = parser.parse_args()

    verify_packages(args.rns_source)
    config_dir = write_config(args.root, args.port)
    import LXMF  # pylint: disable=import-outside-toplevel
    import RNS  # pylint: disable=import-outside-toplevel

    reticulum = None
    completed = threading.Event()
    source_announced = threading.Event()
    result: dict[str, object] = {"received": False}

    class SourceAnnounceHandler:
        aspect_filter = "lxmf.delivery"

        def received_announce(
            self,
            destination_hash: bytes,
            announced_identity: object,
            app_data: bytes | None,
        ) -> None:
            del announced_identity, app_data
            if destination_hash.hex() == args.source:
                print(json.dumps({"source_announced": True}), flush=True)
                source_announced.set()

    def delivered(message: object) -> None:
        result.update(
            {
                "received": True,
                "title": text(message.title),
                "content": text(message.content),
                "source_hash": message.source_hash.hex(),
                "destination_hash": message.destination_hash.hex(),
                "signature_validated": bool(message.signature_validated),
                "method": int(message.method),
                "direct_method": int(LXMF.LXMessage.DIRECT),
            }
        )
        completed.set()

    try:
        reticulum = RNS.Reticulum(configdir=str(config_dir), loglevel=RNS.LOG_ERROR)
        identity = RNS.Identity.from_bytes(bytes(range(64)))
        (args.root / "lxmf").mkdir()
        router = LXMF.LXMRouter(
            storagepath=str(args.root / "lxmf"),
            autopeer=False,
            enforce_stamps=False,
        )
        destination = router.register_delivery_identity(
            identity,
            display_name="OMEN current Python LXMF fixture",
            stamp_cost=None,
        )
        router.register_delivery_callback(delivered)
        RNS.Transport.register_announce_handler(SourceAnnounceHandler())
        print(
            json.dumps(
                {
                    "ready": True,
                    "destination": destination.hash.hex(),
                    "identity": identity.hash.hex(),
                    "port": args.port,
                    "rns": RNS_VERSION,
                    "lxmf": LXMF_VERSION,
                }
            ),
            flush=True,
        )
        if not source_announced.wait(8):
            raise TimeoutError("current Python LXMF peer did not learn Rust source announce")
        if not completed.wait(20):
            raise TimeoutError("Rust peer did not complete current LXMF delivery")
        print(json.dumps(result), flush=True)
        return 0
    finally:
        if reticulum is not None:
            RNS.Reticulum.exit_handler()


if __name__ == "__main__":
    raise SystemExit(main())
