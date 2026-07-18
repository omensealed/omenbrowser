#!/usr/bin/env python3
"""Isolated Python LXMF peer for a live Rust-issued ticket round trip."""

from __future__ import annotations

import argparse
import json
import pathlib
import sys
import threading


NETWORK_NAME = "omen-ifac-vector"
PASSPHRASE = "public-test-fixture"
REPLY_TITLE = "Python ticket-stamped reply"
REPLY_CONTENT = "Python LXMF used the Rust-issued reply ticket"


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
  [[Python LXMF Ticket Server]]
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
    completed = threading.Event()
    source_identity: dict[str, object] = {}
    result: dict[str, object] = {
        "received": False,
        "reply_delivered": False,
        "reply_failed": False,
    }

    class RustSourceAnnounceHandler:
        aspect_filter = "lxmf.delivery"

        def received_announce(
            self,
            destination_hash: bytes,
            announced_identity: object,
            app_data: bytes | None,
        ) -> None:
            del app_data
            if destination_hash.hex() == args.rust_source:
                source_identity["identity"] = announced_identity
                print(json.dumps({"source_announced": True}), flush=True)
                source_announced.set()

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
            display_name="OMEN Python ticket round-trip fixture",
            stamp_cost=None,
        )
        RNS.Transport.register_announce_handler(RustSourceAnnounceHandler())

        def reply_delivered(message: object) -> None:
            result.update(
                {
                    "reply_delivered": True,
                    "reply_message_id": message.message_id.hex(),
                }
            )
            completed.set()

        def reply_failed(message: object) -> None:
            result.update(
                {
                    "reply_failed": True,
                    "reply_message_id": (
                        message.message_id.hex()
                        if message.message_id is not None
                        else None
                    ),
                }
            )
            completed.set()

        def delivered(message: object) -> None:
            result.update(
                {
                    "received": True,
                    "received_signature_validated": bool(message.signature_validated),
                    "received_source": message.source_hash.hex(),
                }
            )
            ticket_entry = message.fields.get(LXMF.FIELD_TICKET)
            ticket_shape_valid = (
                isinstance(ticket_entry, list)
                and len(ticket_entry) >= 2
                and isinstance(ticket_entry[1], bytes)
                and len(ticket_entry[1]) == LXMF.LXMessage.TICKET_LENGTH
            )
            remembered = router.get_outbound_ticket(message.source_hash)
            result["ticket_shape_valid"] = ticket_shape_valid
            result["ticket_remembered"] = (
                ticket_shape_valid and remembered == ticket_entry[1]
            )
            announced_identity = source_identity.get("identity")
            if not result["ticket_remembered"] or announced_identity is None:
                result["reply_failed"] = True
                completed.set()
                return

            def send_reply() -> None:
                rust_destination = RNS.Destination(
                    announced_identity,
                    RNS.Destination.OUT,
                    RNS.Destination.SINGLE,
                    "lxmf",
                    "delivery",
                )
                reply = LXMF.LXMessage(
                    rust_destination,
                    destination,
                    content=REPLY_CONTENT,
                    title=REPLY_TITLE,
                    desired_method=LXMF.LXMessage.DIRECT,
                )
                reply.register_delivery_callback(reply_delivered)
                reply.register_failed_callback(reply_failed)
                router.handle_outbound(reply)
                result.update(
                    {
                        "reply_ticket_applied": reply.outbound_ticket == remembered,
                        "reply_ticket_cost": reply.stamp_value == LXMF.LXMessage.COST_TICKET,
                        "reply_stamp_matches": reply.stamp
                        == RNS.Identity.truncated_hash(remembered + reply.message_id),
                    }
                )

            threading.Thread(target=send_reply, daemon=True).start()

        router.register_delivery_callback(delivered)
        print(
            json.dumps(
                {
                    "ready": True,
                    "destination": destination.hash.hex(),
                    "identity": identity.hash.hex(),
                    "lxmf": LXMF.__version__,
                    "port": args.port,
                    "reply_content": REPLY_CONTENT,
                    "reply_title": REPLY_TITLE,
                    "rns": RNS.__version__,
                }
            ),
            flush=True,
        )
        if not source_announced.wait(10):
            raise TimeoutError("Python ticket peer did not learn the Rust source announce")
        if not completed.wait(25):
            raise TimeoutError("Python ticket reply did not reach a terminal result")
        required = (
            result["received"]
            and result.get("received_signature_validated")
            and result.get("ticket_shape_valid")
            and result.get("ticket_remembered")
            and result.get("reply_ticket_applied")
            and result.get("reply_ticket_cost")
            and result.get("reply_stamp_matches")
            and result["reply_delivered"]
            and not result["reply_failed"]
        )
        result["passed"] = bool(required)
        print(json.dumps(result, sort_keys=True), flush=True)
        return 0 if required else 1
    finally:
        if reticulum is not None:
            RNS.Reticulum.exit_handler()


if __name__ == "__main__":
    raise SystemExit(main())
