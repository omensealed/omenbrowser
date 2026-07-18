#!/usr/bin/env python3
"""Isolated Python LXMF peer enforcing direct-message stamps."""

from __future__ import annotations

import argparse
import json
import pathlib
import sys
import threading
import time


NETWORK_NAME = "omen-ifac-vector"
PASSPHRASE = "public-test-fixture"
STAMP_COST = 1
STAMPED_TITLE = "OMEN Rust stamped direct LXMF"


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
  [[Python LXMF Direct Stamp Server]]
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
    parser.add_argument("--lxmf-source", type=pathlib.Path)
    parser.add_argument("--expected-rns", required=True)
    parser.add_argument("--expected-lxmf", required=True)
    parser.add_argument("--root", required=True, type=pathlib.Path)
    parser.add_argument("--port", required=True, type=int)
    parser.add_argument("--rust-source", required=True)
    args = parser.parse_args()

    if args.lxmf_source is not None:
        sys.path.insert(0, str(args.lxmf_source))
    sys.path.insert(0, str(args.rns_source))
    import LXMF  # pylint: disable=import-outside-toplevel
    import RNS  # pylint: disable=import-outside-toplevel

    if RNS.__version__ != args.expected_rns or LXMF.__version__ != args.expected_lxmf:
        raise RuntimeError(
            "Python stack differs from requested interoperability pins: "
            f"rns={RNS.__version__} lxmf={LXMF.__version__}"
        )

    config_dir = write_config(args.root, args.port)
    reticulum = None
    source_announced = threading.Event()
    first_received = threading.Event()
    received: list[dict[str, object]] = []

    class RustSourceAnnounceHandler:
        aspect_filter = "lxmf.delivery"

        def received_announce(
            self,
            destination_hash: bytes,
            announced_identity: object,
            app_data: bytes | None,
        ) -> None:
            del announced_identity, app_data
            if destination_hash.hex() == args.rust_source:
                print(json.dumps({"source_announced": True}), flush=True)
                source_announced.set()

    def delivered(message: object) -> None:
        received.append(
            {
                "title": text(message.title),
                "source_hash": message.source_hash.hex(),
                "signature_validated": bool(message.signature_validated),
                "stamp_valid": bool(message.stamp_valid),
                "stamp_value": int(message.stamp_value),
            }
        )
        first_received.set()

    try:
        reticulum = RNS.Reticulum(configdir=str(config_dir), loglevel=RNS.LOG_ERROR)
        identity = RNS.Identity.from_bytes(bytes(range(64)))
        (args.root / "lxmf").mkdir()
        router = LXMF.LXMRouter(
            storagepath=str(args.root / "lxmf"),
            autopeer=False,
            enforce_stamps=True,
        )
        destination = router.register_delivery_identity(
            identity,
            display_name="OMEN Python direct stamp fixture",
            stamp_cost=STAMP_COST,
        )
        router.register_delivery_callback(delivered)
        RNS.Transport.register_announce_handler(RustSourceAnnounceHandler())
        print(
            json.dumps(
                {
                    "ready": True,
                    "destination": destination.hash.hex(),
                    "identity": identity.hash.hex(),
                    "lxmf": LXMF.__version__,
                    "port": args.port,
                    "rns": RNS.__version__,
                    "stamp_cost": STAMP_COST,
                }
            ),
            flush=True,
        )
        if not source_announced.wait(10):
            raise TimeoutError("Python direct-stamp peer did not learn Rust source announce")
        if not first_received.wait(20):
            raise TimeoutError("Python direct-stamp peer did not receive stamped message")
        time.sleep(2)
        passed = (
            len(received) == 1
            and received[0]["title"] == STAMPED_TITLE
            and received[0]["source_hash"] == args.rust_source
            and received[0]["signature_validated"]
            and received[0]["stamp_valid"]
            and received[0]["stamp_value"] >= STAMP_COST
        )
        print(
            json.dumps(
                {
                    "passed": bool(passed),
                    "received_count": len(received),
                    "stamped_accepted": bool(received),
                    "unstamped_rejected": len(received) == 1,
                },
                sort_keys=True,
            ),
            flush=True,
        )
        return 0 if passed else 1
    finally:
        if reticulum is not None:
            RNS.Reticulum.exit_handler()


if __name__ == "__main__":
    raise SystemExit(main())
