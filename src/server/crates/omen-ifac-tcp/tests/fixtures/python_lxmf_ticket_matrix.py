#!/usr/bin/env python3
"""Exercise Python LXMF ticket lifecycle with Rust-produced ticket material."""

from __future__ import annotations

import argparse
import json
import pathlib
import sys
import threading
import time


TICKET_BYTES = 16
MESSAGE_ID_BYTES = 32
STAMP_BYTES = 16
MAX_FIXTURE_BYTES = 64


def bounded_read(path: pathlib.Path, expected: int) -> bytes:
    data = path.read_bytes()
    if len(data) != expected or len(data) > MAX_FIXTURE_BYTES:
        raise ValueError(f"{path.name} has an invalid bounded fixture length")
    return data


def message_with_stamp(lxmf: object, message_id: bytes, stamp: bytes) -> object:
    message = object.__new__(lxmf.LXMessage)
    message.hash = message_id
    message.message_id = message_id
    message.stamp = stamp
    message.stamp_valid = None
    message.stamp_value = None
    return message


def isolated_router(lxmf: object, storage: pathlib.Path) -> object:
    storage.mkdir(parents=True, exist_ok=True)
    router = object.__new__(lxmf.LXMRouter)
    router.storagepath = str(storage)
    router.ticket_file_lock = threading.Lock()
    router.available_tickets = {
        "outbound": {},
        "inbound": {},
        "last_deliveries": {},
    }
    return router


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--rns-source", required=True, type=pathlib.Path)
    parser.add_argument("--lxmf-source", type=pathlib.Path)
    parser.add_argument("--expected-rns", required=True)
    parser.add_argument("--expected-lxmf", required=True)
    parser.add_argument("--root", required=True, type=pathlib.Path)
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

    ticket = bounded_read(args.root / "ticket.bin", TICKET_BYTES)
    message_id = bounded_read(args.root / "message-id.bin", MESSAGE_ID_BYTES)
    stamp = bounded_read(args.root / "stamp.bin", STAMP_BYTES)
    destination = bytes(range(TICKET_BYTES))
    router = isolated_router(LXMF, args.root / "python-ticket-store")

    first = router.generate_ticket(destination)
    second = router.generate_ticket(destination)
    reused_before_renewal = first is not None and second == first
    default_expiry_window = (
        first is not None
        and LXMF.LXMessage.TICKET_RENEW < first[0] - time.time() <= LXMF.LXMessage.TICKET_EXPIRY
    )

    router.available_tickets["last_deliveries"][destination] = time.time()
    throttled_after_delivery = router.generate_ticket(destination) is None
    router.available_tickets["last_deliveries"].clear()

    prior_ticket = first[1]
    router.available_tickets["inbound"][destination][prior_ticket] = [
        time.time() + LXMF.LXMessage.TICKET_RENEW - 1
    ]
    renewed = router.generate_ticket(destination)
    renewed_near_expiry = renewed is not None and renewed[1] != prior_ticket

    future_expiry = time.time() + 60
    router.remember_ticket(destination, [future_expiry, ticket])
    remembered_for_use = router.get_outbound_ticket(destination) == ticket
    remembered_expiry = router.get_outbound_ticket_expiry(destination)
    expiry_preserved = remembered_expiry is not None and abs(remembered_expiry - future_expiry) < 0.01

    valid = message_with_stamp(LXMF, message_id, stamp)
    rust_stamp_accepted = valid.validate_stamp(LXMF.LXMessage.COST_TICKET, tickets=[ticket])
    wrong = message_with_stamp(LXMF, message_id, stamp)
    wrong_ticket = bytearray(ticket)
    wrong_ticket[0] ^= 0xFF
    wrong_ticket_rejected = not wrong.validate_stamp(
        LXMF.LXMessage.COST_TICKET, tickets=[bytes(wrong_ticket)]
    )

    router.available_tickets["outbound"][destination] = [time.time() - 1, ticket]
    expired_outbound_rejected = router.get_outbound_ticket(destination) is None
    stale = bytes((value ^ 0xA5) for value in ticket)
    router.available_tickets["inbound"][destination] = {
        ticket: [time.time() + 60],
        stale: [time.time() - LXMF.LXMessage.TICKET_GRACE - 1],
    }
    active_only = router.get_inbound_tickets(destination) == [ticket]
    router.clean_available_tickets()
    expired_cleaned = stale not in router.available_tickets["inbound"][destination]

    checks = {
        "active_only": active_only,
        "default_expiry_window": default_expiry_window,
        "expired_cleaned": expired_cleaned,
        "expired_outbound_rejected": expired_outbound_rejected,
        "expiry_preserved": expiry_preserved,
        "remembered_for_use": remembered_for_use,
        "renewed_near_expiry": renewed_near_expiry,
        "reused_before_renewal": reused_before_renewal,
        "rust_stamp_accepted": rust_stamp_accepted,
        "throttled_after_delivery": throttled_after_delivery,
        "wrong_ticket_rejected": wrong_ticket_rejected,
    }
    if not all(checks.values()):
        raise RuntimeError(f"ticket lifecycle matrix failed: {checks}")

    print(
        json.dumps(
            {
                "checks": checks,
                "lxmf": LXMF.__version__,
                "rns": RNS.__version__,
                "ticket_bytes": len(ticket),
            },
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
