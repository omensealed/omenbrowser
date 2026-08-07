#!/usr/bin/env python3
"""Isolated current-Python NomadNet page node for OMEN interoperability."""

from __future__ import annotations

import argparse
import importlib.metadata
import json
import os
import pathlib
import sys
import threading
import time


RNS_VERSION = "1.4.0"
NOMADNET_VERSION = "1.2.7"
NETWORK_NAME = "omen-ifac-vector"
PASSPHRASE = "public-test-fixture"
INDEX_PAGE = ">Current Python NomadNet\nempty request passed\n"
FORM_PAGE = ">Current Python Form\nfield=omen\nnext=/page/index.mu\n"
OVERSIZED_FORM_PAGE = ">Current Python Form\nfield_size=2048\nnext=/page/index.mu\n"
LARGE_PAGE = ">Current Python Large Response\n" + ("resource-response-line\n" * 16_384)
SLOW_RESPONSE_DELAY_SECONDS = 3
SOAK_REQUESTS_PER_LINK = 16
SOAK_TOTAL_REQUESTS = SOAK_REQUESTS_PER_LINK * 2


def verify_packages(source: pathlib.Path) -> None:
    sys.path.insert(0, str(source))
    actual_rns = importlib.metadata.version("rns")
    actual_nomadnet = importlib.metadata.version("nomadnet")
    if actual_rns != RNS_VERSION or actual_nomadnet != NOMADNET_VERSION:
        raise RuntimeError(
            "current Python stack differs from drift pins: "
            f"rns={actual_rns} nomadnet={actual_nomadnet}"
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
  [[Current NomadNet IFAC Server]]
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


class MessageRouter:
    def announce_propagation_node(self) -> None:
        return


class App:
    def __init__(self, root: pathlib.Path, identity: object) -> None:
        self.identity = identity
        self.node_announce_interval = 720
        self.page_refresh_interval = 0
        self.file_refresh_interval = 0
        self.node_announce_at_start = False
        self.node_name = "OMEN current Python NomadNet fixture"
        self.pagespath = str(root / "pages") + os.sep
        self.filespath = str(root / "files") + os.sep
        self.message_router = MessageRouter()
        self.peer_settings = {
            "display_name": "OMEN fixture",
            "served_page_requests": 0,
            "node_last_announce": 0,
            "node_connects": 0,
        }

    def save_peer_settings(self) -> None:
        return


def write_pages(root: pathlib.Path) -> None:
    pages = root / "pages"
    files = root / "files"
    pages.mkdir()
    files.mkdir()
    (pages / "index.mu").write_text(INDEX_PAGE, encoding="utf-8")
    form = pages / "form.mu"
    form.write_text(
        """#!/usr/bin/env python3
import os
print(">Current Python Form")
field = os.environ.get("field_name", "missing")
if len(field) > 64:
    print("field_size=" + str(len(field)))
else:
    print("field=" + field)
print("next=" + os.environ.get("var_next", "missing"))
""",
        encoding="utf-8",
    )
    form.chmod(0o700)
    (pages / "large.mu").write_text(LARGE_PAGE, encoding="utf-8")
    for name in ("timeout.mu", "cancel.mu"):
        slow = pages / name
        slow.write_text(
            f"""#!/usr/bin/env python3
import time
time.sleep({SLOW_RESPONSE_DELAY_SECONDS})
print(">Current Python Delayed Response")
""",
            encoding="utf-8",
        )
        slow.chmod(0o700)
    reuse = pages / "reuse.mu"
    reuse.write_text(
        """#!/usr/bin/env python3
import json
import os
import pathlib

state_path = pathlib.Path(__file__).with_name(".reuse-state.json")
state = json.loads(state_path.read_text(encoding="utf-8")) if state_path.exists() else {}
link_id = os.environ.get("link_id", "missing")
count = int(state.get("count", 0)) + 1
first_link = state.get("first_link", link_id)
same_link = count > 1 and link_id == first_link and link_id != "missing"
all_same = bool(state.get("all_same", True)) and (count == 1 or same_link)
state_path.write_text(
    json.dumps({"count": count, "first_link": first_link, "all_same": all_same}),
    encoding="utf-8",
)
print(">Current Python Repeated Request")
print("visit=" + str(count))
print("same_link=" + ("true" if same_link else "initial"))
""",
        encoding="utf-8",
    )
    reuse.chmod(0o700)
    measure = pages / "measure.mu"
    measure.write_text(
        """#!/usr/bin/env python3
import json
import os
import pathlib

state_path = pathlib.Path(__file__).with_name(".measure-state.json")
state = json.loads(state_path.read_text(encoding="utf-8")) if state_path.exists() else {}
link_id = os.environ.get("link_id", "missing")
count = int(state.get("count", 0)) + 1
first_link = state.get("first_link", link_id)
same_link = count > 1 and link_id == first_link and link_id != "missing"
all_same = bool(state.get("all_same", True)) and (count == 1 or same_link)
state_path.write_text(
    json.dumps({"count": count, "first_link": first_link, "all_same": all_same}),
    encoding="utf-8",
)
payload = os.environ.get("field_payload", "")
print(">Current Python Primitive Measurement")
print("request=" + str(count))
print("field_size=" + str(len(payload)))
""",
        encoding="utf-8",
    )
    measure.chmod(0o700)
    soak = pages / "soak.mu"
    soak.write_text(
        """#!/usr/bin/env python3
import json
import os
import pathlib

state_path = pathlib.Path(__file__).with_name(".soak-state.json")
state = json.loads(state_path.read_text(encoding="utf-8")) if state_path.exists() else {}
link_id = os.environ.get("link_id", "missing")
first_link = state.get("first_link")
second_link = state.get("second_link")
if first_link is None:
    first_link = link_id
if link_id == first_link:
    generation = 1
elif second_link is None:
    second_link = link_id
    generation = 2
elif link_id == second_link:
    generation = 2
else:
    generation = 3
count = int(state.get("count", 0)) + 1
generation_counts = state.get("generation_counts", {"1": 0, "2": 0, "3": 0})
generation_counts[str(generation)] = int(generation_counts.get(str(generation), 0)) + 1
state_path.write_text(
    json.dumps(
        {
            "count": count,
            "first_link": first_link,
            "second_link": second_link,
            "generation_counts": generation_counts,
        }
    ),
    encoding="utf-8",
)
payload = os.environ.get("field_payload", "")
print(">Current Python Keepalive Recovery Soak")
print("request=" + str(count))
print("generation=" + str(generation))
print("field_size=" + str(len(payload)))
""",
        encoding="utf-8",
    )
    soak.chmod(0o700)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--python-source", required=True, type=pathlib.Path)
    parser.add_argument("--root", required=True, type=pathlib.Path)
    parser.add_argument("--port", required=True, type=int)
    parser.add_argument(
        "--scenario",
        choices=("matrix", "faults", "reuse", "performance", "soak"),
        default="matrix",
    )
    args = parser.parse_args()

    verify_packages(args.python_source)
    args.root.mkdir(parents=True)
    write_pages(args.root)
    config_dir = write_config(args.root, args.port)
    import RNS  # pylint: disable=import-outside-toplevel
    from nomadnet.Node import Node  # pylint: disable=import-outside-toplevel

    reticulum = None
    stop_announces = threading.Event()
    announce_thread = None
    try:
        reticulum = RNS.Reticulum(configdir=str(config_dir), loglevel=RNS.LOG_ERROR)
        identity = RNS.Identity.from_bytes(bytes(range(64)))
        app = App(args.root, identity)
        node = Node(app)

        def announce_until_complete() -> None:
            while not stop_announces.is_set():
                node.announce()
                stop_announces.wait(0.5)

        announce_thread = threading.Thread(target=announce_until_complete, daemon=True)
        announce_thread.start()
        print(
            json.dumps(
                {
                    "ready": True,
                    "destination": node.destination.hash.hex(),
                    "port": args.port,
                    "rns": RNS_VERSION,
                    "nomadnet": NOMADNET_VERSION,
                }
            ),
            flush=True,
        )

        expected_requests = {
            "matrix": 4,
            "faults": 2,
            "reuse": 2,
            "performance": 18,
            "soak": SOAK_TOTAL_REQUESTS,
        }[args.scenario]
        deadline = time.monotonic() + 45
        soak_link_closed = False
        max_active_links = 0
        while (
            app.peer_settings["served_page_requests"] < expected_requests
            and time.monotonic() < deadline
        ):
            max_active_links = max(max_active_links, len(node.destination.links))
            if (
                args.scenario == "soak"
                and not soak_link_closed
                and app.peer_settings["served_page_requests"] >= SOAK_REQUESTS_PER_LINK
            ):
                # The initiating Rust caller has already received the final
                # response before it waits for this marker. Close the one live
                # server-side link and require the second half of the soak to
                # arrive over exactly one replacement generation.
                time.sleep(0.25)
                active_links = list(node.destination.links)
                if len(active_links) == 1:
                    active_links[0].teardown()
                    soak_link_closed = True
                    print(
                        json.dumps(
                            {
                                "recovery_ready": True,
                                "requests_before_close": SOAK_REQUESTS_PER_LINK,
                            }
                        ),
                        flush=True,
                    )
            time.sleep(0.05)
        served = int(app.peer_settings["served_page_requests"])
        if served == expected_requests:
            # Keep the node alive beyond every delayed handler so an accidental
            # automatic request replay is observable in the final count.
            time.sleep(1.0 if args.scenario == "matrix" else 3.5)
        reuse_state_path = args.root / "pages" / ".reuse-state.json"
        reuse_state = (
            json.loads(reuse_state_path.read_text(encoding="utf-8"))
            if reuse_state_path.exists()
            else {}
        )
        measure_state_path = args.root / "pages" / ".measure-state.json"
        measure_state = (
            json.loads(measure_state_path.read_text(encoding="utf-8"))
            if measure_state_path.exists()
            else {}
        )
        soak_state_path = args.root / "pages" / ".soak-state.json"
        soak_state = (
            json.loads(soak_state_path.read_text(encoding="utf-8"))
            if soak_state_path.exists()
            else {}
        )
        scenario_passed = served == expected_requests
        if args.scenario == "reuse":
            scenario_passed = (
                scenario_passed
                and reuse_state.get("count") == 2
                and reuse_state.get("all_same") is True
            )
        if args.scenario == "performance":
            scenario_passed = (
                scenario_passed
                and measure_state.get("count") == expected_requests
                and measure_state.get("all_same") is True
            )
        if args.scenario == "soak":
            generation_counts = soak_state.get("generation_counts", {})
            scenario_passed = (
                scenario_passed
                and soak_link_closed
                and soak_state.get("count") == SOAK_TOTAL_REQUESTS
                and generation_counts.get("1") == SOAK_REQUESTS_PER_LINK
                and generation_counts.get("2") == SOAK_REQUESTS_PER_LINK
                and int(generation_counts.get("3", 0)) == 0
                and max_active_links <= 1
            )
        result = {
            "index_bytes": len(INDEX_PAGE.encode("utf-8")),
            "form_bytes": len(FORM_PAGE.encode("utf-8")),
            "oversized_form_bytes": len(OVERSIZED_FORM_PAGE.encode("utf-8")),
            "large_page_bytes": len(LARGE_PAGE.encode("utf-8")),
            "measure_request_count": int(measure_state.get("count", 0)),
            "measure_same_link": measure_state.get("all_same") is True,
            "passed": scenario_passed,
            "reuse_request_count": int(reuse_state.get("count", 0)),
            "reuse_same_link": reuse_state.get("all_same") is True,
            "scenario": args.scenario,
            "served_page_requests": served,
            "soak_first_generation_requests": int(
                soak_state.get("generation_counts", {}).get("1", 0)
            ),
            "soak_max_active_links": max_active_links,
            "soak_recovery_performed": soak_link_closed,
            "soak_second_generation_requests": int(
                soak_state.get("generation_counts", {}).get("2", 0)
            ),
        }
        print(json.dumps(result, sort_keys=True), flush=True)
        return 0 if result["passed"] else 1
    finally:
        stop_announces.set()
        if announce_thread is not None:
            announce_thread.join(timeout=1)
        if reticulum is not None:
            RNS.Reticulum.exit_handler()


if __name__ == "__main__":
    raise SystemExit(main())
