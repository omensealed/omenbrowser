#!/usr/bin/env python3
"""Isolated Python LXMF peer receiving one stamped Resource-sized message."""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import sys
import threading


NETWORK_NAME = "omen-ifac-vector"
PASSPHRASE = "public-test-fixture"
STAMP_COST = 1
RESOURCE_TITLE = "OMEN Rust stamped Resource LXMF"
RESOURCE_BODY_BYTES = 64 * 1024
RESOURCE_BODY = "R" * RESOURCE_BODY_BYTES
RESOURCE_BODY_SHA256 = hashlib.sha256(RESOURCE_BODY.encode("utf-8")).hexdigest()
ATTACHMENT_FIELD = 0x05
ATTACHMENT_NAME = "lxmf-attachment-smoke.bin"
ATTACHMENT_BYTES = bytes(range(256)) * 8
ATTACHMENT_SHA256 = hashlib.sha256(ATTACHMENT_BYTES).hexdigest()


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
  [[Python LXMF Direct Resource Server]]
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

    reticulum = None
    source_announced = threading.Event()
    received = threading.Event()
    observation: dict[str, object] = {}

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
        content = text(message.content)
        fields = getattr(message, "fields", None) or {}
        attachments = fields.get(ATTACHMENT_FIELD, [])
        attachment_name = None
        attachment_bytes = b""
        if len(attachments) == 1 and len(attachments[0]) == 2:
            attachment_name = text(attachments[0][0])
            attachment_bytes = bytes(attachments[0][1])
        observation.update(
            {
                "attachment_bytes": len(attachment_bytes),
                "attachment_name": attachment_name,
                "attachment_sha256": hashlib.sha256(attachment_bytes).hexdigest(),
                "body_bytes": len(content.encode("utf-8")),
                "body_sha256": hashlib.sha256(content.encode("utf-8")).hexdigest(),
                "signature_validated": bool(message.signature_validated),
                "source_hash": message.source_hash.hex(),
                "stamp_valid": bool(message.stamp_valid),
                "stamp_value": int(message.stamp_value),
                "title": text(message.title),
            }
        )
        received.set()

    try:
        config_dir = write_config(args.root, args.port)
        reticulum = RNS.Reticulum(configdir=str(config_dir), loglevel=RNS.LOG_ERROR)
        identity = RNS.Identity.from_bytes(bytes(range(64)))
        (args.root / "lxmf").mkdir()
        router = LXMF.LXMRouter(
            storagepath=str(args.root / "lxmf"), autopeer=False, enforce_stamps=True
        )
        destination = router.register_delivery_identity(
            identity,
            display_name="OMEN Python direct Resource fixture",
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
            raise TimeoutError("Python direct-Resource peer did not learn Rust source announce")
        if not received.wait(30):
            raise TimeoutError("Python direct-Resource peer did not receive message")
        passed = (
            observation.get("title") == RESOURCE_TITLE
            and observation.get("body_bytes") == RESOURCE_BODY_BYTES
            and observation.get("body_sha256") == RESOURCE_BODY_SHA256
            and observation.get("attachment_name") == ATTACHMENT_NAME
            and observation.get("attachment_bytes") == len(ATTACHMENT_BYTES)
            and observation.get("attachment_sha256") == ATTACHMENT_SHA256
            and observation.get("source_hash") == args.rust_source
            and observation.get("signature_validated") is True
            and observation.get("stamp_valid") is True
            and int(observation.get("stamp_value", 0)) >= STAMP_COST
        )
        print(
            json.dumps(
                {
                    "attachment_bytes": observation.get("attachment_bytes"),
                    "attachment_name": observation.get("attachment_name"),
                    "attachment_sha256_match": observation.get("attachment_sha256")
                    == ATTACHMENT_SHA256,
                    "body_bytes": observation.get("body_bytes"),
                    "body_sha256_match": observation.get("body_sha256")
                    == RESOURCE_BODY_SHA256,
                    "passed": bool(passed),
                    "signature_validated": observation.get("signature_validated"),
                    "stamp_valid": observation.get("stamp_valid"),
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
