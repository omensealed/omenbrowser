#!/usr/bin/env python3
"""Isolated Python propagation node observing stamped Rust network admission."""

from __future__ import annotations

import argparse
import json
import pathlib
import sys
import threading
import time


NETWORK_NAME = "omen-ifac-vector"
PASSPHRASE = "public-test-fixture"
ADVERTISED_COST = 13
REJECTION_COST = 255


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

    if RNS.__version__ != expected_rns or LXMF.__version__ != expected_lxmf:
        raise RuntimeError(
            "Python stack differs from requested interoperability pins: "
            f"rns={RNS.__version__} lxmf={LXMF.__version__}"
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
  [[Stamped LXMF Propagation IFAC Server]]
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
    parser.add_argument("--source", required=True)
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
    from LXMF import LXStamper  # pylint: disable=import-outside-toplevel

    reticulum = None
    accepted = threading.Event()
    rejected = threading.Event()
    source_announced = threading.Event()
    validation_lock = threading.Lock()
    validation_results: list[dict[str, int]] = []
    delivered_messages: list[dict[str, object]] = []

    original_validate = LXStamper.validate_pn_stamps

    def observed_validate(messages: list[bytes], target_cost: int) -> list[object]:
        result = original_validate(messages, target_cost)
        with validation_lock:
            validation_results.append(
                {
                    "messages": len(messages),
                    "accepted": len(result),
                    "target_cost": target_cost,
                    "stamp_value": int(result[0][2]) if result else -1,
                    "active_propagation_links": sum(
                        link.status == RNS.Link.ACTIVE
                        for link in router.active_propagation_links
                    ),
                }
            )
        if result:
            accepted.set()
        else:
            rejected.set()
        return result

    LXStamper.validate_pn_stamps = observed_validate

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
                source_announced.set()

    def delivered(message: object) -> None:
        delivered_messages.append(
            {
                "title": text(message.title),
                "content": text(message.content),
                "source_hash": message.source_hash.hex(),
                "destination_hash": message.destination_hash.hex(),
                "signature_validated": bool(message.signature_validated),
                "method": int(message.method),
            }
        )

    try:
        reticulum = RNS.Reticulum(configdir=str(config_dir), loglevel=RNS.LOG_ERROR)
        router_identity = RNS.Identity.from_bytes(bytes(range(64)))
        receiver_identity = RNS.Identity.from_bytes(bytes(reversed(range(64))))
        (args.root / "lxmf").mkdir()
        router = LXMF.LXMRouter(
            identity=router_identity,
            storagepath=str(args.root / "lxmf"),
            autopeer=False,
            enforce_stamps=False,
            propagation_cost=ADVERTISED_COST,
            propagation_cost_flexibility=0,
            name="OMEN stamped propagation fixture",
        )
        destination = router.register_delivery_identity(
            receiver_identity,
            display_name="OMEN stamped propagation receiver",
            stamp_cost=None,
        )
        router.register_delivery_callback(delivered)
        router.enable_propagation()
        RNS.Transport.register_announce_handler(SourceAnnounceHandler())
        destination.announce()
        router.propagation_destination.announce(
            app_data=router.get_propagation_node_app_data()
        )
        print(
            json.dumps(
                {
                    "ready": True,
                    "destination": destination.hash.hex(),
                    "propagation": router.propagation_destination.hash.hex(),
                    "port": args.port,
                    "rns": args.expected_rns,
                    "lxmf": args.expected_lxmf,
                    "advertised_cost": ADVERTISED_COST,
                }
            ),
            flush=True,
        )
        if not source_announced.wait(10):
            raise TimeoutError("Python propagation node did not learn Rust source announce")
        if not accepted.wait(45):
            raise TimeoutError("Python propagation handler did not accept the stamped message")
        deadline = time.monotonic() + 5
        while not delivered_messages and time.monotonic() < deadline:
            time.sleep(0.02)
        if len(delivered_messages) != 1:
            raise RuntimeError("accepted propagation transient was not delivered exactly once")

        with validation_lock:
            accepted_result = validation_results[-1].copy()
        accepted.clear()
        router.propagation_stamp_cost = REJECTION_COST
        print(
            json.dumps(
                {
                    "accepted": True,
                    "validation": accepted_result,
                    "delivery": delivered_messages[0],
                    "client_messages": router.client_propagation_messages_received,
                }
            ),
            flush=True,
        )

        if not rejected.wait(20):
            raise TimeoutError("Python propagation handler did not reject the under-cost message")
        time.sleep(0.2)
        with validation_lock:
            rejected_result = validation_results[-1].copy()
        result = {
            "rejected": True,
            "validation": rejected_result,
            "delivery_count": len(delivered_messages),
            "client_messages": router.client_propagation_messages_received,
            "rejection_cost": REJECTION_COST,
        }
        print(json.dumps(result), flush=True)
        return 0 if len(delivered_messages) == 1 else 1
    finally:
        LXStamper.validate_pn_stamps = original_validate
        if reticulum is not None:
            RNS.Reticulum.exit_handler()


if __name__ == "__main__":
    raise SystemExit(main())
