#!/usr/bin/env python3
"""Isolated current-Python LXMF direct sender for OMEN interoperability."""

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
TITLE = "Current Python direct LXMF"
CONTENT = "Rust 0.9.6 received this signed Python LXMF message"


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


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--rns-source", required=True, type=pathlib.Path)
    parser.add_argument("--root", required=True, type=pathlib.Path)
    parser.add_argument("--port", required=True, type=int)
    parser.add_argument("--destination", required=True)
    args = parser.parse_args()

    verify_packages(args.rns_source)
    config_dir = write_config(args.root, args.port)
    import LXMF  # pylint: disable=import-outside-toplevel
    import RNS  # pylint: disable=import-outside-toplevel

    reticulum = None
    completed = threading.Event()
    started = threading.Event()
    result: dict[str, object] = {"delivered": False, "failed": False}

    try:
        reticulum = RNS.Reticulum(configdir=str(config_dir), loglevel=RNS.LOG_ERROR)
        identity = RNS.Identity.from_bytes(bytes(reversed(range(64))))
        (args.root / "lxmf").mkdir()
        router = LXMF.LXMRouter(
            storagepath=str(args.root / "lxmf"),
            autopeer=False,
            enforce_stamps=False,
        )
        source = router.register_delivery_identity(
            identity,
            display_name="OMEN current Python LXMF sender",
            stamp_cost=None,
        )

        def delivered(message: object) -> None:
            result.update(
                {
                    "delivered": True,
                    "message_id": message.message_id.hex(),
                    "source_hash": message.source_hash.hex(),
                    "destination_hash": message.destination_hash.hex(),
                    "method": int(message.method),
                    "direct_method": int(LXMF.LXMessage.DIRECT),
                }
            )
            completed.set()

        def failed(message: object) -> None:
            result.update(
                {
                    "failed": True,
                    "message_id": (
                        message.message_id.hex()
                        if message.message_id is not None
                        else None
                    ),
                }
            )
            completed.set()

        class DestinationAnnounceHandler:
            aspect_filter = "lxmf.delivery"

            def received_announce(
                self,
                destination_hash: bytes,
                announced_identity: object,
                app_data: bytes | None,
            ) -> None:
                del app_data
                if destination_hash.hex() != args.destination or started.is_set():
                    return
                started.set()

                def send() -> None:
                    destination = RNS.Destination(
                        announced_identity,
                        RNS.Destination.OUT,
                        RNS.Destination.SINGLE,
                        "lxmf",
                        "delivery",
                    )
                    message = LXMF.LXMessage(
                        destination,
                        source,
                        content=CONTENT,
                        title=TITLE,
                        desired_method=LXMF.LXMessage.DIRECT,
                    )
                    message.register_delivery_callback(delivered)
                    message.register_failed_callback(failed)
                    router.handle_outbound(message)

                threading.Thread(target=send, daemon=True).start()

        RNS.Transport.register_announce_handler(DestinationAnnounceHandler())
        print(
            json.dumps(
                {
                    "ready": True,
                    "source": source.hash.hex(),
                    "identity": identity.hash.hex(),
                    "title": TITLE,
                    "content": CONTENT,
                    "port": args.port,
                    "rns": RNS_VERSION,
                    "lxmf": LXMF_VERSION,
                }
            ),
            flush=True,
        )
        if not completed.wait(20):
            raise TimeoutError("current Python LXMF direct send did not complete")
        print(json.dumps(result), flush=True)
        return 0 if result["delivered"] and not result["failed"] else 1
    finally:
        if reticulum is not None:
            RNS.Reticulum.exit_handler()


if __name__ == "__main__":
    raise SystemExit(main())
