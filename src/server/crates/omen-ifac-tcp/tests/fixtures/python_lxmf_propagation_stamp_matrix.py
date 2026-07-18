#!/usr/bin/env python3
"""Validate bounded Rust propagation stamps with an exact Python LXMF source."""

from __future__ import annotations

import argparse
import json
import pathlib
import sys


MAX_TRANSIENT_BYTES = 1024 * 1024
STAMP_BYTES = 32


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--rns-source", required=True, type=pathlib.Path)
    parser.add_argument("--lxmf-source", type=pathlib.Path)
    parser.add_argument("--expected-rns", required=True)
    parser.add_argument("--expected-lxmf", required=True)
    parser.add_argument("--root", required=True, type=pathlib.Path)
    parser.add_argument("--stamp-value", required=True, type=int)
    args = parser.parse_args()

    if args.lxmf_source is not None:
        sys.path.insert(0, str(args.lxmf_source))
    sys.path.insert(0, str(args.rns_source))

    import LXMF  # pylint: disable=import-outside-toplevel
    import RNS  # pylint: disable=import-outside-toplevel
    from LXMF import LXStamper  # pylint: disable=import-outside-toplevel

    if RNS.__version__ != args.expected_rns or LXMF.__version__ != args.expected_lxmf:
        raise RuntimeError(
            "Python stack differs from requested interoperability pins: "
            f"rns={RNS.__version__} lxmf={LXMF.__version__}"
        )
    if args.stamp_value < 0 or args.stamp_value >= 255:
        raise ValueError("fixture stamp value must leave one bounded rejection boundary")

    lxm_data = (args.root / "lxm-data.bin").read_bytes()
    stamp = (args.root / "stamp.bin").read_bytes()
    if not lxm_data or len(lxm_data) > MAX_TRANSIENT_BYTES:
        raise ValueError("fixture transient is empty or exceeds its byte budget")
    if len(stamp) != STAMP_BYTES:
        raise ValueError("fixture stamp must be exactly 32 bytes")

    transient_data = lxm_data + stamp
    accepted = LXStamper.validate_pn_stamps([transient_data], args.stamp_value)
    rejected = LXStamper.validate_pn_stamps([transient_data], args.stamp_value + 1)
    if len(accepted) != 1:
        raise RuntimeError("Python LXMF rejected the Rust stamp at its achieved value")
    if rejected:
        raise RuntimeError("Python LXMF accepted the Rust stamp above its achieved value")

    transient_id, accepted_lxm, python_value, accepted_stamp = accepted[0]
    if accepted_lxm != lxm_data or accepted_stamp != stamp:
        raise RuntimeError("Python LXMF changed accepted propagation material")
    if python_value != args.stamp_value:
        raise RuntimeError(
            "Rust/Python propagation stamp values differ: "
            f"rust={args.stamp_value} python={python_value}"
        )
    if transient_id != RNS.Identity.full_hash(lxm_data):
        raise RuntimeError("Python LXMF returned the wrong transient identifier")

    print(
        json.dumps(
            {
                "accepted_at_value": args.stamp_value,
                "rejected_at_value": args.stamp_value + 1,
                "rns": RNS.__version__,
                "lxmf": LXMF.__version__,
                "transient_bytes": len(lxm_data),
            },
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
