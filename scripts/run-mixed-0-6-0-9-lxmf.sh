#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
readonly repo_root
readonly old_commit=${OMEN_MIXED_OLD_COMMIT:-5ba6683055fb6c59111919fbad1ac37f56a4c203}
readonly old_expected_version=${OMEN_MIXED_OLD_VERSION:-0.6.0-1}
readonly current_expected_version=0.9.7-5
readonly gateway_rns_version=1.4.0
readonly network_name=omen-mixed-version

report_path=""
resource_fixture=false
restart_fixture=false
while (( $# > 0 )); do
  case "$1" in
    --report)
      if (( $# < 2 )); then
        echo "--report requires a path" >&2
        exit 2
      fi
      report_path=$2
      shift 2
      ;;
    --resource)
      resource_fixture=true
      shift
      ;;
    --restart)
      restart_fixture=true
      shift
      ;;
    *)
      echo "usage: $0 [--resource|--restart] [--report /path/to/report.json]" >&2
      exit 2
      ;;
  esac
done
if [[ "$resource_fixture" == true && "$restart_fixture" == true ]]; then
  echo "--resource and --restart are separate bounded compatibility cases" >&2
  exit 2
fi

temporary_root=$(mktemp -d "${TMPDIR:-/tmp}/omen-mixed-lxmf.XXXXXX")
gateway_pid=""
old_pid=""
current_pid=""
cleanup() {
  for pid in "$old_pid" "$current_pid" "$gateway_pid"; do
    if [[ -n "$pid" ]]; then
      kill "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
    fi
  done
  rm -rf -- "$temporary_root"
}
trap cleanup EXIT INT TERM

old_source="$temporary_root/old-source"
old_target=${OMEN_MIXED_OLD_TARGET_DIR:-$temporary_root/old-target}
current_source=""
current_target=""
old_app="$temporary_root/old-app"
current_app="$temporary_root/current-app"
mkdir -p -- "$old_source" "$old_target" "$old_app" "$current_app"

git -C "$repo_root" cat-file -e "$old_commit^{commit}"
git -C "$repo_root" archive "$old_commit" | tar -x -C "$old_source"

if [[ "$resource_fixture" == true ]]; then
  old_target="${old_target}-resource-fixture"
  current_source="$temporary_root/current-source"
  current_target="$temporary_root/current-target"
  mkdir -p -- "$current_source"
  tar -C "$repo_root" \
    --exclude=.git --exclude=target --exclude=src/server/target \
    -cf - . | tar -C "$current_source" -xf -
  git -C "$old_source" apply \
    "$repo_root/fixtures/lxmf/mixed_application_resource_driver.patch"
  git -C "$current_source" apply \
    "$repo_root/fixtures/lxmf/mixed_application_resource_driver.patch"
fi

CARGO_TARGET_DIR="$old_target" cargo build --locked \
  --manifest-path "$old_source/Cargo.toml" \
  --no-default-features --features native-network --bin omenbrowser_rs
if [[ "$resource_fixture" == true ]]; then
  CARGO_TARGET_DIR="$current_target" cargo build --locked \
    --manifest-path "$current_source/Cargo.toml" \
    --no-default-features --features native-network --bin omenbrowser_rs
else
  cargo build --locked --manifest-path "$repo_root/Cargo.toml" \
    --no-default-features --features native-network --bin omenbrowser_rs
fi

old_bin="$old_target/debug/omenbrowser_rs"
if [[ "$resource_fixture" == true ]]; then
  current_bin="$current_target/debug/omenbrowser_rs"
  expected_content_bytes=65536
  transfer_fixture=resource
else
  current_bin="${CARGO_TARGET_DIR:-$repo_root/target}/debug/omenbrowser_rs"
  expected_content_bytes=102
  transfer_fixture=link_packet
fi
old_version=$($old_bin --version | awk '{print $2}')
current_version=$($current_bin --version | awk '{print $2}')
if [[ "$old_version" != "$old_expected_version" ]]; then
  echo "mixed LXMF old application version mismatch: $old_version" >&2
  exit 1
fi
if [[ "$current_version" != "$current_expected_version" ]]; then
  echo "mixed LXMF current application version mismatch: $current_version" >&2
  exit 1
fi

python3 -m venv "$temporary_root/venv"
python="$temporary_root/venv/bin/python"
"$python" -m pip install --disable-pip-version-check --no-input --quiet \
  "rns==$gateway_rns_version"

"$old_bin" --generate-native-identity mixed-old \
  --app-root "$old_app" --output "$temporary_root/old-identity.json" \
  >"$temporary_root/old-identity.stdout" 2>"$temporary_root/old-identity.stderr"
"$current_bin" --generate-native-identity mixed-current \
  --app-root "$current_app" --output "$temporary_root/current-identity.json" \
  >"$temporary_root/current-identity.stdout" 2>"$temporary_root/current-identity.stderr"

old_identity="$old_app/identities/default_identity"
current_identity="$current_app/identities/default_identity"
"$old_bin" --lxmf-interop --lxmf-wait 1 --backend reticulum \
  --identity "$old_identity" --app-root "$old_app" \
  --output "$temporary_root/old-discovery.json" >/dev/null 2>&1
"$current_bin" --lxmf-interop --lxmf-wait 1 --backend reticulum \
  --identity "$current_identity" --app-root "$current_app" \
  --output "$temporary_root/current-discovery.json" >/dev/null 2>&1

old_destination=$("$python" -c \
  'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["local"]["local_lxmf_destination_hash"])' \
  "$temporary_root/old-discovery.json")
current_destination=$("$python" -c \
  'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["local"]["local_lxmf_destination_hash"])' \
  "$temporary_root/current-discovery.json")

port=$("$python" -c \
  'import socket; sock=socket.socket(); sock.bind(("127.0.0.1", 0)); print(sock.getsockname()[1]); sock.close()')
passphrase_file="$temporary_root/passphrase"
printf '%s\n' public-test-fixture >"$passphrase_file"
chmod 600 "$passphrase_file"

gateway_config="$temporary_root/gateway/config"
mkdir -p -- "$gateway_config" "$temporary_root/gateway/storage"
"$python" - "$gateway_config/config" "$port" <<'PY'
import pathlib
import sys

pathlib.Path(sys.argv[1]).write_text(
    f"""[reticulum]
  enable_transport = Yes
  share_instance = No
  instance_control_port = 0
  panic_on_interface_error = Yes

[logging]
  loglevel = 1

[interfaces]
  [[Mixed LXMF TCP Server]]
    type = TCPServerInterface
    enabled = Yes
    listen_ip = 127.0.0.1
    listen_port = {sys.argv[2]}
    network_name = omen-mixed-version
    passphrase = public-test-fixture
""",
    encoding="utf-8",
)
PY

"$temporary_root/venv/bin/rnsd" --config "$gateway_config" --quiet \
  >"$temporary_root/gateway.stdout" 2>"$temporary_root/gateway.stderr" &
gateway_pid=$!
"$python" - "$port" "$gateway_pid" <<'PY'
import os
import socket
import sys
import time

port = int(sys.argv[1])
pid = int(sys.argv[2])
deadline = time.monotonic() + 8
while time.monotonic() < deadline:
    try:
        os.kill(pid, 0)
        with socket.create_connection(("127.0.0.1", port), timeout=0.2):
            break
    except (ConnectionRefusedError, OSError):
        time.sleep(0.05)
else:
    raise TimeoutError("isolated Python Reticulum gateway did not become ready")
PY

common_old=(
  --backend reticulum --identity "$old_identity" --app-root "$old_app"
  --tcp-client "127.0.0.1:$port" --network-name "$network_name"
  --passphrase-file "$passphrase_file"
)
common_current=(
  --backend reticulum --identity "$current_identity" --app-root "$current_app"
  --tcp-client "127.0.0.1:$port" --network-name "$network_name"
  --passphrase-file "$passphrase_file"
)

# Keep both runtimes alive long enough to exchange authenticated announces before
# the actual send. No state outside the two temporary application roots is used.
"$old_bin" --lxmf-interop --lxmf-wait 5 "${common_old[@]}" \
  --output "$temporary_root/old-warm.json" \
  >"$temporary_root/old-warm.stdout" 2>"$temporary_root/old-warm.stderr" &
old_pid=$!
"$current_bin" --lxmf-interop --lxmf-wait 5 "${common_current[@]}" \
  --output "$temporary_root/current-warm.json" \
  >"$temporary_root/current-warm.stdout" 2>"$temporary_root/current-warm.stderr" &
current_pid=$!
wait "$old_pid"
old_pid=""
wait "$current_pid"
current_pid=""

exchange_successful_attempt=0
exchange_elapsed_ms=0
run_directional_round() {
  local prefix=$1
  local started_ns
  started_ns=$(date +%s%N)

  # Current -> old: keep the old receive-only runtime online before the current
  # sender opens its link. This avoids simultaneous cross-link activation.
  "$old_bin" --lxmf-interop --lxmf-wait 20 "${common_old[@]}" \
    --output "$temporary_root/$prefix-old-receiver.json" \
    >"$temporary_root/$prefix-old-receiver.stdout" \
    2>"$temporary_root/$prefix-old-receiver.stderr" &
  old_pid=$!
  sleep 2
  "$current_bin" --lxmf-interop --send-lxmf-smoke "$old_destination" \
    --lxmf-wait 15 "${common_current[@]}" \
    --output "$temporary_root/$prefix-current-sender.json" \
    >"$temporary_root/$prefix-current-sender.stdout" \
    2>"$temporary_root/$prefix-current-sender.stderr" &
  current_pid=$!
  wait "$current_pid"
  current_pid=""
  wait "$old_pid"
  old_pid=""

  # Old -> current: use a new current receive-only process, then start the old
  # sender. Each logical message is attempted exactly once.
  "$current_bin" --lxmf-interop --lxmf-wait 20 "${common_current[@]}" \
    --output "$temporary_root/$prefix-current-receiver.json" \
    >"$temporary_root/$prefix-current-receiver.stdout" \
    2>"$temporary_root/$prefix-current-receiver.stderr" &
  current_pid=$!
  sleep 2
  "$old_bin" --lxmf-interop --send-lxmf-smoke "$current_destination" \
    --lxmf-wait 15 "${common_old[@]}" \
    --output "$temporary_root/$prefix-old-sender.json" \
    >"$temporary_root/$prefix-old-sender.stdout" \
    2>"$temporary_root/$prefix-old-sender.stderr" &
  old_pid=$!
  wait "$old_pid"
  old_pid=""
  wait "$current_pid"
  current_pid=""

  "$python" - "$prefix" "$temporary_root" \
    "$old_expected_version" "$current_expected_version" <<'PY'
import json
import pathlib
import sys

prefix = sys.argv[1]
root = pathlib.Path(sys.argv[2])
old_version = sys.argv[3]
current_version = sys.argv[4]

def load(name):
    return json.loads((root / f"{prefix}-{name}.json").read_text(encoding="utf-8"))

def combine(version, sender, receiver):
    if sender.get("send", {}).get("ok") is not True:
        raise RuntimeError(f"{version} directional sender failed")
    sender_wait = sender.get("wait", {})
    wait = dict(receiver.get("wait", {}))
    received = [event for event in wait.get("events", []) if event.get("event") == "message_received"]
    if wait.get("inbound_messages") != 1 or len(received) != 1:
        raise RuntimeError(f"{version} directional receiver did not admit exactly one message")
    wait["inbound_reply_match_state"] = "matched_peer_reply"
    wait["proof_match_state"] = sender_wait.get("proof_match_state")
    sender["wait"] = wait
    sender["classification"] = {
        "outcome": "pass",
        "reason": "directional sender and receiver evidence correlated",
        "wait_status": wait.get("status"),
        "inbound_reply_match_state": "matched_peer_reply",
        "proof_match_state": sender_wait.get("proof_match_state"),
    }
    return sender

old = combine(old_version, load("old-sender"), load("old-receiver"))
current = combine(current_version, load("current-sender"), load("current-receiver"))
(root / f"{prefix}-old-report.json").write_text(
    json.dumps(old), encoding="utf-8"
)
(root / f"{prefix}-current-report.json").write_text(
    json.dumps(current), encoding="utf-8"
)
PY
  exchange_successful_attempt=1
  exchange_elapsed_ms=$((($(date +%s%N) - started_ns) / 1000000))
}

run_directional_round initial
initial_successful_attempt=$exchange_successful_attempt
initial_elapsed_ms=$exchange_elapsed_ms

restart_successful_attempt=0
restart_elapsed_ms=0
final_prefix=initial
if [[ "$restart_fixture" == true ]]; then
  # Both successful processes have exited. Reopen the same application,
  # identity, configuration, and Reticulum roots before a second exchange.
  "$old_bin" --lxmf-interop --lxmf-wait 5 "${common_old[@]}" \
    --output "$temporary_root/restart-old-warm.json" \
    >"$temporary_root/restart-old-warm.stdout" \
    2>"$temporary_root/restart-old-warm.stderr" &
  old_pid=$!
  "$current_bin" --lxmf-interop --lxmf-wait 5 "${common_current[@]}" \
    --output "$temporary_root/restart-current-warm.json" \
    >"$temporary_root/restart-current-warm.stdout" \
    2>"$temporary_root/restart-current-warm.stderr" &
  current_pid=$!
  wait "$old_pid"
  old_pid=""
  wait "$current_pid"
  current_pid=""

  run_directional_round restart
  restart_successful_attempt=$exchange_successful_attempt
  restart_elapsed_ms=$exchange_elapsed_ms
  final_prefix=restart
fi

summary="$temporary_root/summary.json"
"$python" - "$temporary_root/initial-old-report.json" \
  "$temporary_root/initial-current-report.json" \
  "$temporary_root/$final_prefix-old-report.json" \
  "$temporary_root/$final_prefix-current-report.json" \
  "$summary" "$old_commit" "$old_version" "$current_version" \
  "$gateway_rns_version" "$initial_elapsed_ms" \
  "$initial_successful_attempt" "$expected_content_bytes" "$transfer_fixture" \
  "$restart_fixture" "$restart_elapsed_ms" "$restart_successful_attempt" <<'PY'
import json
import pathlib
import sys

initial_old = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
initial_current = json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))
final_old = json.loads(pathlib.Path(sys.argv[3]).read_text(encoding="utf-8"))
final_current = json.loads(pathlib.Path(sys.argv[4]).read_text(encoding="utf-8"))

expected_content_bytes = int(sys.argv[12])
transfer_fixture = sys.argv[13]
restart_fixture = sys.argv[14] == "true"
link_packet_mdu = 431
if transfer_fixture == "resource" and expected_content_bytes <= link_packet_mdu:
    raise RuntimeError("resource fixture does not exceed the application Link-packet MDU")

def validate(label, report):
    if report.get("classification", {}).get("outcome") != "pass":
        raise RuntimeError(f"{label} classification did not pass")
    if report.get("send", {}).get("ok") is not True:
        raise RuntimeError(f"{label} direct send did not succeed")
    if report.get("send", {}).get("transport_method") != "direct":
        raise RuntimeError(f"{label} did not use direct delivery")
    wait = report.get("wait", {})
    if wait.get("status") != "observed" or wait.get("inbound_messages", 0) < 1:
        raise RuntimeError(f"{label} did not observe the reciprocal message")
    if wait.get("inbound_reply_match_state") != "matched_peer_reply":
        raise RuntimeError(f"{label} did not bind the reciprocal message to its peer")
    received = [event for event in wait.get("events", []) if event.get("event") == "message_received"]
    if (len(received) != 1 or received[0].get("title_len") != 32
            or received[0].get("content_len") != expected_content_bytes):
        raise RuntimeError(f"{label} received an unexpected message shape")
    return ({
        "direct_send": True,
        "reciprocal_message_count": wait["inbound_messages"],
        "message_shape_verified": True,
        "packet_proof_observed": wait.get("proof_match_state") == "matched_packet_proof",
        "received_content_bytes": expected_content_bytes,
    }, report["send"].get("message_id"), received[0].get("message_id"),
        report.get("local", {}).get("local_lxmf_destination_hash"))

def validate_round(label, old, current):
    old_summary, old_sent, old_received, old_destination = validate(
        f"{label} {sys.argv[7]}", old
    )
    current_summary, current_sent, current_received, current_destination = validate(
        f"{label} {sys.argv[8]}", current
    )
    if not all((old_sent, old_received, current_sent, current_received)):
        raise RuntimeError(f"{label} did not expose message identifiers for correlation")
    if old_received != current_sent or current_received != old_sent:
        raise RuntimeError(f"{label} reciprocal inbound message IDs did not match peer sends")
    return {
        "old_to_current": old_summary,
        "current_to_old": current_summary,
        "reciprocal_message_ids_correlated": True,
    }, (old_sent, old_received, old_destination), (
        current_sent, current_received, current_destination
    )

initial, initial_old_ids, initial_current_ids = validate_round(
    "initial", initial_old, initial_current
)
final, final_old_ids, final_current_ids = validate_round("restart", final_old, final_current)

restart = None
if restart_fixture:
    destinations_preserved = (
        initial_old_ids[2] == final_old_ids[2]
        and initial_current_ids[2] == final_current_ids[2]
        and all((initial_old_ids[2], initial_current_ids[2]))
    )
    outbound_ids_changed = (
        initial_old_ids[0] != final_old_ids[0]
        and initial_current_ids[0] != final_current_ids[0]
    )
    inbound_ids_changed = (
        initial_old_ids[1] != final_old_ids[1]
        and initial_current_ids[1] != final_current_ids[1]
    )
    if not destinations_preserved:
        raise RuntimeError("application destinations changed after reopening state roots")
    if not outbound_ids_changed or not inbound_ids_changed:
        raise RuntimeError("restart exchange reused a prior logical message identifier")
    restart = {
        **final,
        "state_roots_reused": True,
        "identity_destinations_preserved": True,
        "outbound_message_ids_changed": True,
        "inbound_message_ids_changed": True,
        "duplicate_inbound_messages": False,
        "elapsed_ms": int(sys.argv[15]),
        "successful_attempt": int(sys.argv[16]),
    }

summary = {
    "status": "pass",
    "old_source_commit": sys.argv[6],
    "old_application_version": sys.argv[7],
    "current_application_version": sys.argv[8],
    "gateway": f"python-rns=={sys.argv[9]}",
    "elapsed_ms": int(sys.argv[10]),
    "successful_attempt": int(sys.argv[11]),
    "transfer_fixture": transfer_fixture,
    "link_packet_mdu": link_packet_mdu,
    "fixture_exceeds_link_packet_mdu": expected_content_bytes > link_packet_mdu,
    **initial,
}
if restart is not None:
    summary["restart"] = restart
pathlib.Path(sys.argv[5]).write_text(
    json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
)
PY

if [[ -n "$report_path" ]]; then
  mkdir -p -- "$(dirname -- "$report_path")"
  cp -- "$summary" "$report_path"
fi
cat "$summary"
if [[ "$restart_fixture" == true ]]; then
  echo "mixed OMENbrowser $old_version/$current_version restart/state reopening: pass"
else
  echo "mixed OMENbrowser $old_version/$current_version direct LXMF $transfer_fixture interoperability: pass"
fi
