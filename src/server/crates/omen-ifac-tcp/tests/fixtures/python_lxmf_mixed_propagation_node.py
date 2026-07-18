#!/usr/bin/env python3
"""Isolated Python node for current-to-0.6 OMEN propagation evidence."""

from __future__ import annotations

import argparse
import json
import pathlib
import sys
import time


NETWORK_NAME = "omen-mixed-propagation"
PASSPHRASE = "public-test-fixture"


def storage_snapshot(root: pathlib.Path) -> tuple[tuple[str, int, int], ...]:
    if not root.exists():
        return ()
    return tuple(
        sorted(
            (
                str(path.relative_to(root)),
                path.stat().st_size,
                path.stat().st_mtime_ns,
            )
            for path in root.rglob("*")
            if path.is_file() and not path.is_symlink()
        )
    )


def wait_for_settled_storage(
    root: pathlib.Path,
    baseline: tuple[tuple[str, int, int], ...],
) -> tuple[int, int]:
    deadline = time.monotonic() + 5
    previous = None
    stable_samples = 0
    while time.monotonic() < deadline:
        current = storage_snapshot(root)
        if current != baseline and current == previous:
            stable_samples += 1
            if stable_samples >= 3:
                return len(current), sum(entry[1] for entry in current)
        else:
            stable_samples = 0
        previous = current
        time.sleep(0.05)
    raise TimeoutError("propagation storage did not change and settle after queue admission")


def verify_packages(source: pathlib.Path, expected_rns: str, expected_lxmf: str) -> None:
    sys.path.insert(0, str(source))
    import LXMF  # pylint: disable=import-outside-toplevel
    import RNS  # pylint: disable=import-outside-toplevel

    if RNS.__version__ != expected_rns or LXMF.__version__ != expected_lxmf:
        raise RuntimeError(
            "Python stack differs from requested interoperability versions: "
            f"rns={RNS.__version__} lxmf={LXMF.__version__}"
        )


def write_config(root: pathlib.Path, port: int) -> pathlib.Path:
    config_dir = root / "config"
    (root / "storage").mkdir(parents=True, exist_ok=True)
    config_dir.mkdir(parents=True, exist_ok=True)
    (config_dir / "config").write_text(
        f"""[reticulum]
  enable_transport = Yes
  share_instance = No
  instance_control_port = 0
  panic_on_interface_error = Yes

[logging]
  loglevel = 1

[interfaces]
  [[Mixed LXMF Propagation Server]]
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
    parser.add_argument("--python-source", required=True, type=pathlib.Path)
    parser.add_argument("--expected-rns", required=True)
    parser.add_argument("--expected-lxmf", required=True)
    parser.add_argument("--root", required=True, type=pathlib.Path)
    parser.add_argument("--port", required=True, type=int)
    parser.add_argument("--old-destination", required=True)
    parser.add_argument("--current-source", required=True)
    parser.add_argument("--exit-after-queued", action="store_true")
    parser.add_argument("--report-storage-settled", action="store_true")
    parser.add_argument("--require-stamp", action="store_true")
    args = parser.parse_args()

    verify_packages(args.python_source, args.expected_rns, args.expected_lxmf)
    config_dir = write_config(args.root, args.port)
    import LXMF  # pylint: disable=import-outside-toplevel
    import RNS  # pylint: disable=import-outside-toplevel

    reticulum = None
    old_seen = False
    current_seen = False

    class DeliveryAnnounceHandler:
        aspect_filter = "lxmf.delivery"

        def received_announce(
            self,
            destination_hash: bytes,
            announced_identity: object,
            app_data: bytes | None,
        ) -> None:
            del announced_identity, app_data
            nonlocal old_seen, current_seen
            value = destination_hash.hex()
            old_seen = old_seen or value == args.old_destination
            current_seen = current_seen or value == args.current_source

    try:
        reticulum = RNS.Reticulum(configdir=str(config_dir), loglevel=RNS.LOG_ERROR)
        router_identity = RNS.Identity.from_bytes(bytes(range(64)))
        (args.root / "lxmf").mkdir(exist_ok=True)
        router = LXMF.LXMRouter(
            identity=router_identity,
            storagepath=str(args.root / "lxmf"),
            autopeer=False,
            enforce_stamps=args.require_stamp,
            propagation_cost=1 if args.require_stamp else 0,
            propagation_cost_flexibility=0,
            name="OMEN mixed-version propagation fixture",
        )
        router.enable_propagation()
        RNS.Transport.register_announce_handler(DeliveryAnnounceHandler())
        initial_storage = storage_snapshot(args.root / "lxmf")

        print(
            json.dumps(
                {
                    "ready": True,
                    "propagation": router.propagation_destination.hash.hex(),
                    "port": args.port,
                    "rns": args.expected_rns,
                    "lxmf": args.expected_lxmf,
                    "restored_entries": len(router.propagation_entries),
                    "stamp_required": args.require_stamp,
                    "advertised_stamp_cost": int(router.propagation_stamp_cost),
                }
            ),
            flush=True,
        )

        announce_at = 0.0
        queued_at = None
        queued_count = 0
        deadline = time.monotonic() + 75
        while time.monotonic() < deadline:
            now = time.monotonic()
            if now >= announce_at:
                router.propagation_destination.announce(
                    app_data=router.get_propagation_node_app_data()
                )
                announce_at = now + 1.0

            entries = len(router.propagation_entries)
            if queued_at is None and entries:
                queued_at = now
                queued_count = entries
                print(
                    json.dumps(
                        {
                            "queued": True,
                            "entries": entries,
                            "old_announce_seen": old_seen,
                            "current_announce_seen": current_seen,
                        }
                    ),
                    flush=True,
                )
                if args.report_storage_settled:
                    files, stored_bytes = wait_for_settled_storage(
                        args.root / "lxmf", initial_storage
                    )
                    print(
                        json.dumps(
                            {
                                "storage_settled": True,
                                "files": files,
                                "stored_bytes_positive": stored_bytes > 0,
                            }
                        ),
                        flush=True,
                    )
                if args.exit_after_queued:
                    return 0
            elif queued_at is not None and entries == 0:
                print(
                    json.dumps(
                        {
                            "acknowledged": True,
                            "remaining": 0,
                            "queued_entries": queued_count,
                            "old_announce_seen": old_seen,
                            "current_announce_seen": current_seen,
                        }
                    ),
                    flush=True,
                )
                return 0
            time.sleep(0.05)

        raise TimeoutError(
            "mixed propagation fixture timed out before one queued transient was acknowledged"
        )
    finally:
        if reticulum is not None:
            RNS.Reticulum.exit_handler()


if __name__ == "__main__":
    raise SystemExit(main())
