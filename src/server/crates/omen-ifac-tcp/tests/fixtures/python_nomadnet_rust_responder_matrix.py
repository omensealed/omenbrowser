#!/usr/bin/env python3
"""Python RNS requester for the Rust omenchatd NomadNet response matrix."""

from __future__ import annotations

import argparse
import json
import pathlib
import random
import sys
import threading
import time


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
  [[Rust NomadNet Portal]]
    type = TCPClientInterface
    enabled = Yes
    target_host = 127.0.0.1
    target_port = {port}
""",
        encoding="utf-8",
    )
    return config_dir


def wait_until(predicate, timeout: float, label: str) -> None:
    deadline = time.monotonic() + timeout
    while not predicate() and time.monotonic() < deadline:
        time.sleep(0.05)
    if not predicate():
        raise RuntimeError(f"timed out waiting for {label}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--rns-source", required=True, type=pathlib.Path)
    parser.add_argument("--expected-rns", required=True)
    parser.add_argument("--root", required=True, type=pathlib.Path)
    parser.add_argument("--port", required=True, type=int)
    parser.add_argument("--destination", required=True)
    parser.add_argument("--page", required=True, type=pathlib.Path)
    args = parser.parse_args()

    sys.path.insert(0, str(args.rns_source))
    import RNS  # pylint: disable=import-outside-toplevel

    actual_rns = str(getattr(RNS, "__version__", "unknown"))
    if actual_rns != args.expected_rns:
        raise RuntimeError(
            f"Python RNS source differs from requested lane: {actual_rns}"
        )

    args.root.mkdir(parents=True)
    config_dir = write_config(args.root, args.port)
    reticulum = None
    link = None
    try:
        reticulum = RNS.Reticulum(configdir=str(config_dir), loglevel=RNS.LOG_ERROR)
        destination_hash = bytes.fromhex(args.destination)
        deadline = time.monotonic() + 12
        while (
            not RNS.Transport.has_path(destination_hash)
            or RNS.Identity.recall(destination_hash) is None
        ) and time.monotonic() < deadline:
            RNS.Transport.request_path(destination_hash)
            time.sleep(0.2)
        if not RNS.Transport.has_path(destination_hash):
            raise RuntimeError("Rust portal path was not discovered")
        identity = RNS.Identity.recall(destination_hash)
        if identity is None:
            raise RuntimeError("Rust portal identity was not recalled")

        destination = RNS.Destination(
            identity,
            RNS.Destination.OUT,
            RNS.Destination.SINGLE,
            "nomadnetwork",
            "node",
        )
        established = threading.Event()
        closed = threading.Event()
        link = RNS.Link(destination)
        link.set_link_established_callback(lambda _link: established.set())
        link.set_link_closed_callback(lambda _link: closed.set())
        if not established.wait(10):
            raise RuntimeError("Rust portal Link did not activate")

        # Exercise both request primitives at the conservative public boundary
        # that OMEN uses for its production selector. Newer negotiated TCP MTUs
        # can otherwise make every bounded fixture request a direct packet.
        link.mdu = RNS.Reticulum.MDU

        rng = random.Random(0x0983)
        small_body = b">Python to Rust small response\nexact bytes\n"
        large_body = b">Python to Rust large response\n" + bytes(
            rng.randrange(33, 127) for _ in range(32768)
        )
        large_request_a = "".join(chr(rng.randrange(33, 127)) for _ in range(800))
        large_request_b = "".join(chr(rng.randrange(33, 127)) for _ in range(800))
        cases = (
            ("direct", "direct", None, small_body),
            ("direct", "resource", None, large_body),
            ("resource", "direct", {"field_payload": large_request_a}, small_body),
            ("resource", "resource", {"field_payload": large_request_b}, large_body),
        )

        observed = []
        request_ids: set[bytes] = set()
        for expected_request, expected_response, request_data, body in cases:
            args.page.write_bytes(body)
            completed = threading.Event()
            failed = threading.Event()
            holder: dict[str, object] = {}
            response_used_resource = threading.Event()

            def response_callback(receipt: object) -> None:
                holder["receipt"] = receipt
                completed.set()

            def failed_callback(receipt: object) -> None:
                holder["receipt"] = receipt
                failed.set()

            def progress_callback(receipt: object) -> None:
                if receipt.get_status() == RNS.RequestReceipt.RECEIVING:
                    response_used_resource.set()

            receipt = link.request(
                "/page/index.mu",
                data=request_data,
                response_callback=response_callback,
                failed_callback=failed_callback,
                progress_callback=progress_callback,
                timeout=12,
            )
            request_primitive = (
                "direct" if receipt.packet_receipt is not None else "resource"
            )
            if request_primitive != expected_request:
                raise RuntimeError(
                    f"request primitive mismatch: {request_primitive} != {expected_request}"
                )
            wait_until(
                lambda: completed.is_set() or failed.is_set() or closed.is_set(),
                15,
                "NomadNet response",
            )
            if failed.is_set() or closed.is_set() or not completed.is_set():
                raise RuntimeError("Rust NomadNet response did not complete")
            result = holder["receipt"]
            request_id = bytes(result.get_request_id())
            response_primitive = "resource" if response_used_resource.is_set() else "direct"
            response = bytes(result.get_response())
            if response_primitive != expected_response:
                raise RuntimeError(
                    f"response primitive mismatch: {response_primitive} != {expected_response}"
                )
            if response != body:
                raise RuntimeError("Rust NomadNet response bytes changed")
            if request_id in request_ids:
                raise RuntimeError("request correlation identifier was reused")
            request_ids.add(request_id)
            observed.append(
                {
                    "request": request_primitive,
                    "response": response_primitive,
                    "response_bytes": len(response),
                }
            )

        # Leave a bounded observation window for accidental responder replay.
        time.sleep(0.5)
        result = {
            "passed": len(observed) == 4 and len(request_ids) == 4,
            "rns": actual_rns,
            "matrix": observed,
            "requests": len(request_ids),
        }
        print(json.dumps(result, sort_keys=True), flush=True)
        return 0 if result["passed"] else 1
    finally:
        if link is not None and link.status != RNS.Link.CLOSED:
            link.teardown()
        if reticulum is not None:
            RNS.Reticulum.exit_handler()


if __name__ == "__main__":
    raise SystemExit(main())
