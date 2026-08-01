#!/usr/bin/env python3
"""Validate redacted, bounded LXMF invitation capability probe evidence."""

import argparse
import json
from pathlib import Path


ALLOWED_KEYS = {
    "report",
    "peer_destination_redacted",
    "announce_attempted",
    "announce_ok",
    "outcome",
    "supported",
    "error_category",
    "cancellation_requested",
    "cancel_after_ms",
    "elapsed_ms",
    "deadline_ms",
    "automatic_retries",
    "invitation_sent",
    "shutdown_ok",
}


def fail(message: str) -> None:
    raise SystemExit(message)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("report", type=Path)
    parser.add_argument("--expect", choices=("supported", "unsupported", "cancelled"), required=True)
    args = parser.parse_args()

    with args.report.open("r", encoding="utf-8") as handle:
        report = json.load(handle)
    if not isinstance(report, dict):
        fail("capability report must be a JSON object")
    unexpected = sorted(set(report) - ALLOWED_KEYS)
    if unexpected:
        fail(f"capability report contains unreviewed fields: {unexpected}")
    if report.get("report") != "native_lxmf_invitation_capability_probe":
        fail("wrong capability report kind")
    if report.get("peer_destination_redacted") is not True:
        fail("peer destination was not marked redacted")
    if report.get("automatic_retries") != 0 or report.get("invitation_sent") is not False:
        fail("probe violated no-retry/no-invitation invariants")
    if report.get("shutdown_ok") is not True:
        fail("runtime did not shut down cleanly")
    if report.get("deadline_ms") != 15_000:
        fail("probe deadline drifted")
    if not isinstance(report.get("elapsed_ms"), int) or report["elapsed_ms"] < 0:
        fail("probe elapsed time is invalid")

    if args.expect == "supported":
        if report.get("supported") is not True or report.get("outcome") != "supported":
            fail("current receiver did not prove capability support")
        if report.get("cancellation_requested") is not False:
            fail("ordinary support probe unexpectedly requested cancellation")
    elif args.expect == "unsupported":
        if report.get("supported") is not False:
            fail("prior receiver unexpectedly reported capability support")
        if report.get("cancellation_requested") is not False:
            fail("ordinary downgrade probe unexpectedly requested cancellation")
    else:
        if report.get("supported") is not False or report.get("error_category") != "cancelled":
            fail("cancelled probe was not classified as cancelled and unsupported")
        if report.get("cancellation_requested") is not True or report.get("cancel_after_ms") != 0:
            fail("cancelled probe did not record deterministic pre-cancellation")


if __name__ == "__main__":
    main()
