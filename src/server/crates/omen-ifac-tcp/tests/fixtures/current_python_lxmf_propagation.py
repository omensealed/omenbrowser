#!/usr/bin/env python3
"""Isolated current-Python LXMF propagation node for OMEN interoperability."""

from __future__ import annotations

import argparse
import json
import pathlib
import sys
import threading
import time


RNS_VERSION = "1.4.2"
LXMF_VERSION = "1.1.1"
NETWORK_NAME = "omen-ifac-vector"
PASSPHRASE = "public-test-fixture"
TITLE = "Current Python propagated LXMF"
CONTENT = "Rust 0.9.7 synced and acknowledged this signed Python LXMF message"


def verify_packages(
    rns_source: pathlib.Path,
    lxmf_source: pathlib.Path | None,
    expected_rns: str,
    expected_lxmf: str,
) -> None:
    if lxmf_source is not None:
        sys.path.insert(0, str(lxmf_source))
    sys.path.insert(0, str(rns_source))
    import LXMF  # pylint: disable=import-outside-toplevel
    import RNS  # pylint: disable=import-outside-toplevel

    actual_rns = RNS.__version__
    actual_lxmf = LXMF.__version__
    if actual_rns != expected_rns or actual_lxmf != expected_lxmf:
        raise RuntimeError(
            "Python stack differs from requested interoperability pins: "
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
  [[Current LXMF Propagation IFAC Server]]
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
    parser.add_argument("--lxmf-source", type=pathlib.Path)
    parser.add_argument("--expected-rns", default=RNS_VERSION)
    parser.add_argument("--expected-lxmf", default=LXMF_VERSION)
    parser.add_argument("--root", required=True, type=pathlib.Path)
    parser.add_argument("--port", required=True, type=int)
    parser.add_argument("--destination", required=True)
    args = parser.parse_args()

    verify_packages(
        args.rns_source,
        args.lxmf_source,
        args.expected_rns,
        args.expected_lxmf,
    )
    config_dir = write_config(args.root, args.port)
    import LXMF  # pylint: disable=import-outside-toplevel
    import RNS  # pylint: disable=import-outside-toplevel
    import msgpack  # pylint: disable=import-outside-toplevel

    reticulum = None
    queued = threading.Event()
    result: dict[str, object] = {"acknowledged": False}

    try:
        reticulum = RNS.Reticulum(configdir=str(config_dir), loglevel=RNS.LOG_ERROR)
        router_identity = RNS.Identity.from_bytes(bytes(range(64)))
        source_identity = RNS.Identity.from_bytes(bytes(reversed(range(64))))
        (args.root / "lxmf").mkdir()
        router = LXMF.LXMRouter(
            identity=router_identity,
            storagepath=str(args.root / "lxmf"),
            autopeer=False,
            enforce_stamps=False,
            propagation_cost=0,
            name="OMEN current Python propagation fixture",
        )
        source = router.register_delivery_identity(
            source_identity,
            display_name="OMEN current Python propagated sender",
            stamp_cost=None,
        )
        router.enable_propagation()

        class DestinationAnnounceHandler:
            aspect_filter = "lxmf.delivery"

            def received_announce(
                self,
                destination_hash: bytes,
                announced_identity: object,
                app_data: bytes | None,
            ) -> None:
                del app_data
                if destination_hash.hex() != args.destination or queued.is_set():
                    return

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
                    desired_method=LXMF.LXMessage.PROPAGATED,
                )
                message.pack()
                envelope = msgpack.unpackb(message.propagation_packed)
                lxmf_data = envelope[1][0]
                accepted = router.lxmf_propagation(
                    lxmf_data,
                    stamp_value=0,
                    stamp_data=bytes(32),
                )
                if not accepted:
                    raise RuntimeError("Python propagation router rejected fixture transient")

                source.announce()
                router.propagation_destination.announce(
                    app_data=router.get_propagation_node_app_data()
                )
                result.update(
                    {
                        "queued": True,
                        "title": TITLE,
                        "content": CONTENT,
                        "source_hash": source.hash.hex(),
                        "destination_hash": destination.hash.hex(),
                        "transient_id": message.transient_id.hex(),
                    }
                )
                print(json.dumps(result), flush=True)
                queued.set()

        RNS.Transport.register_announce_handler(DestinationAnnounceHandler())
        print(
            json.dumps(
                {
                    "ready": True,
                    "propagation": router.propagation_destination.hash.hex(),
                    "source": source.hash.hex(),
                    "port": args.port,
                    "rns": args.expected_rns,
                    "lxmf": args.expected_lxmf,
                }
            ),
            flush=True,
        )
        if not queued.wait(12):
            raise TimeoutError("Rust receiver announce was not observed")

        deadline = time.monotonic() + 20
        while router.propagation_entries and time.monotonic() < deadline:
            time.sleep(0.05)
        result["acknowledged"] = not router.propagation_entries
        result["remaining"] = len(router.propagation_entries)
        print(json.dumps(result), flush=True)
        return 0 if result["acknowledged"] else 1
    finally:
        if reticulum is not None:
            RNS.Reticulum.exit_handler()


if __name__ == "__main__":
    raise SystemExit(main())
