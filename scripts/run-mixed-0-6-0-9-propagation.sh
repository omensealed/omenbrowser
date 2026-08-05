#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
readonly repo_root
readonly old_commit=${OMEN_MIXED_OLD_COMMIT:-5ba6683055fb6c59111919fbad1ac37f56a4c203}
readonly old_expected_version=${OMEN_MIXED_OLD_VERSION:-0.6.0-1}
readonly current_expected_version=0.9.7-5
readonly recover_unknown_sender=${OMEN_MIXED_RECOVER_UNKNOWN_SENDER:-false}
readonly python_rns_version=1.4.0
readonly python_lxmf_version=1.1.0
readonly network_name=omen-mixed-propagation

report_path=""
reverse=false
node_restart=false
node_crash=false
stamp_ticket=false
while (($#)); do
  case "$1" in
    --reverse)
      reverse=true
      shift
      ;;
    --node-restart)
      node_restart=true
      shift
      ;;
    --node-crash)
      node_crash=true
      shift
      ;;
    --stamp-ticket)
      stamp_ticket=true
      shift
      ;;
    --report)
      if (($# < 2)); then
        echo "--report requires a path" >&2
        exit 2
      fi
      report_path=$2
      shift 2
      ;;
    *)
      echo "usage: $0 [--reverse|--node-restart|--node-crash|--stamp-ticket] [--report /path/to/report.json]" >&2
      exit 2
      ;;
  esac
done
selected_special_modes=0
for mode in "$reverse" "$node_restart" "$node_crash" "$stamp_ticket"; do
  if [[ "$mode" == true ]]; then
    selected_special_modes=$((selected_special_modes + 1))
  fi
done
if ((selected_special_modes > 1)); then
  echo "--reverse, --node-restart, --node-crash, and --stamp-ticket are separate bounded cases" >&2
  exit 2
fi
if [[ "$recover_unknown_sender" != true && "$recover_unknown_sender" != false ]]; then
  echo "OMEN_MIXED_RECOVER_UNKNOWN_SENDER must be true or false" >&2
  exit 2
fi

temporary_root=$(mktemp -d "${TMPDIR:-/tmp}/omen-mixed-propagation.XXXXXX")
node_pid=""
sender_pid=""
recipient_pid=""
cleanup() {
  for pid in "$sender_pid" "$recipient_pid" "$node_pid"; do
    if [[ -n "$pid" ]]; then
      kill "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
    fi
  done
  rm -rf -- "$temporary_root"
}
trap cleanup EXIT INT TERM
trap 'status=$?; echo "mixed propagation harness failed at line $LINENO (status $status)" >&2' ERR

old_source="$temporary_root/old-source"
old_target=${OMEN_MIXED_OLD_TARGET_DIR:-$temporary_root/old-target}
old_app="$temporary_root/old-app"
current_app="$temporary_root/current-app"
mkdir -p -- "$old_source" "$old_target" "$old_app" "$current_app"

git -C "$repo_root" cat-file -e "$old_commit^{commit}"
git -C "$repo_root" archive "$old_commit" | tar -x -C "$old_source"
CARGO_TARGET_DIR="$old_target" cargo build --locked \
  --manifest-path "$old_source/Cargo.toml" \
  --no-default-features --features native-network --bin omenbrowser_rs
cargo build --locked --manifest-path "$repo_root/Cargo.toml" \
  --no-default-features --features native-network --bin omenbrowser_rs

old_bin="$old_target/debug/omenbrowser_rs"
current_bin="${CARGO_TARGET_DIR:-$repo_root/target}/debug/omenbrowser_rs"
old_version=$($old_bin --version | awk '{print $2}')
current_version=$($current_bin --version | awk '{print $2}')
[[ "$old_version" == "$old_expected_version" ]]
[[ "$current_version" == "$current_expected_version" ]]

python3 -m venv "$temporary_root/venv"
python="$temporary_root/venv/bin/python"
"$python" -m pip install --disable-pip-version-check --no-input --quiet \
  "rns==$python_rns_version" "lxmf==$python_lxmf_version"
python_source=$($python -c 'import site; print(site.getsitepackages()[0])')

"$old_bin" --generate-native-identity mixed-old-propagation \
  --app-root "$old_app" --output "$temporary_root/old-identity.json" \
  >"$temporary_root/old-identity.stdout" 2>"$temporary_root/old-identity.stderr"
"$current_bin" --generate-native-identity mixed-current-propagation \
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

old_destination=$($python -c \
  'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["local"]["local_lxmf_destination_hash"])' \
  "$temporary_root/old-discovery.json")
current_destination=$($python -c \
  'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["local"]["local_lxmf_destination_hash"])' \
  "$temporary_root/current-discovery.json")
port=$($python -c \
  'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')

node_script="$repo_root/src/server/crates/omen-ifac-tcp/tests/fixtures/python_lxmf_mixed_propagation_node.py"
initial_node_log="$temporary_root/node.jsonl"
active_node_log=$initial_node_log
node_phase_args=()
if [[ "$node_restart" == true ]]; then
  node_phase_args+=(--exit-after-queued)
elif [[ "$node_crash" == true ]]; then
  node_phase_args+=(--report-storage-settled)
elif [[ "$stamp_ticket" == true ]]; then
  node_phase_args+=(--require-stamp)
fi
"$python" "$node_script" --python-source "$python_source" \
  --expected-rns "$python_rns_version" --expected-lxmf "$python_lxmf_version" \
  --root "$temporary_root/node" --port "$port" \
  --old-destination "$old_destination" --current-source "$current_destination" \
  "${node_phase_args[@]}" \
  >"$initial_node_log" 2>"$temporary_root/node.stderr" &
node_pid=$!

$python - "$initial_node_log" "$node_pid" <<'PY'
import json, os, pathlib, sys, time
path, pid = pathlib.Path(sys.argv[1]), int(sys.argv[2])
deadline = time.monotonic() + 12
while time.monotonic() < deadline:
    os.kill(pid, 0)
    if path.exists():
        for line in path.read_text(encoding="utf-8").splitlines():
            if json.loads(line).get("ready") is True:
                raise SystemExit(0)
    time.sleep(0.05)
raise TimeoutError("mixed Python propagation node did not become ready")
PY
propagation_destination=$($python -c \
  'import json,sys; print(json.loads(open(sys.argv[1], encoding="utf-8").readline())["propagation"])' \
  "$initial_node_log")

passphrase_file="$temporary_root/passphrase"
printf '%s\n' public-test-fixture >"$passphrase_file"
chmod 600 "$passphrase_file"
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

if [[ "$reverse" == true ]]; then
  sender_bin=$old_bin
  sender_label=old
  sender_destination=$current_destination
  sender_common=("${common_old[@]}")
  recipient_bin=$current_bin
  recipient_label=current
  recipient_common=("${common_current[@]}")
  direction="${old_expected_version}_to_${current_expected_version}"
else
  sender_bin=$current_bin
  sender_label=current
  sender_destination=$old_destination
  sender_common=("${common_current[@]}")
  recipient_bin=$old_bin
  recipient_label=old
  recipient_common=("${common_old[@]}")
  direction="${current_expected_version}_to_${old_expected_version}"
fi

# Put the sender online before the recipient announces. This warms only the
# sender's isolated known-destination cache; the propagated message below is
# still attempted exactly once.
"$sender_bin" --lxmf-interop --lxmf-wait 8 "${sender_common[@]}" \
  --output "$temporary_root/$sender_label-warm.json" \
  >"$temporary_root/$sender_label-warm.stdout" \
  2>"$temporary_root/$sender_label-warm.stderr" &
sender_pid=$!
sleep 1
"$recipient_bin" --lxmf-interop --lxmf-wait 5 "${recipient_common[@]}" \
  --output "$temporary_root/$recipient_label-warm.json" \
  >"$temporary_root/$recipient_label-warm.stdout" \
  2>"$temporary_root/$recipient_label-warm.stderr" &
recipient_pid=$!
wait "$recipient_pid"
recipient_pid=""
wait "$sender_pid"
sender_pid=""

# Keep the recipient online for a fresh authenticated announce while the
# sender constructs and submits one propagated message.
"$recipient_bin" --lxmf-interop --lxmf-wait 25 "${recipient_common[@]}" \
  --output "$temporary_root/$recipient_label-receive.json" \
  >"$temporary_root/$recipient_label-receive.stdout" \
  2>"$temporary_root/$recipient_label-receive.stderr" &
recipient_pid=$!
sleep 2
sender_policy_args=()
if [[ "$stamp_ticket" == true ]]; then
  sender_policy_args+=(--lxmf-include-ticket)
fi
"$sender_bin" --lxmf-interop --send-lxmf-smoke "$sender_destination" \
  --lxmf-smoke-method propagated --propagation-node "$propagation_destination" \
  "${sender_policy_args[@]}" \
  --lxmf-wait 20 "${sender_common[@]}" \
  --output "$temporary_root/$sender_label-send.json" \
  >"$temporary_root/$sender_label-send.stdout" \
  2>"$temporary_root/$sender_label-send.stderr" &
sender_pid=$!
set +e
wait "$sender_pid"
sender_status=$?
set -e
sender_pid=""
set +e
wait "$recipient_pid"
recipient_receive_status=$?
set -e
recipient_pid=""

$python - "$initial_node_log" \
  "$temporary_root/$sender_label-send.json" "$sender_label" \
  "$sender_status" "$recipient_receive_status" <<'PY'
import json, pathlib, sys, time
node, report = pathlib.Path(sys.argv[1]), json.loads(pathlib.Path(sys.argv[2]).read_text())
sender = sys.argv[3]
sender_status, recipient_status = int(sys.argv[4]), int(sys.argv[5])
if report.get("send", {}).get("ok") is not True:
    diagnostic = {
        "classification": report.get("classification", {}).get("outcome"),
        "reason": report.get("classification", {}).get("reason"),
        "send": {
            key: report.get("send", {}).get(key)
            for key in ("ok", "skipped", "error", "stage_hint", "first_failed_stage")
            if report.get("send", {}).get(key) is not None
        },
        "readiness_first_failed_stage": next(
            (
                step.get("stage")
                for step in report.get("readiness_probe", {}).get("steps", [])
                if step.get("ok") is not True
            ),
            None,
        ),
        "sender_exit_status": sender_status,
        "recipient_receive_exit_status": recipient_status,
    }
    raise RuntimeError(
        f"{sender} application did not submit propagated LXMF: "
        + json.dumps(diagnostic, sort_keys=True)
    )
if report.get("send", {}).get("transport_method") != "propagated":
    raise RuntimeError(f"{sender} application did not select propagated delivery")
deadline = time.monotonic() + 8
while time.monotonic() < deadline:
    lines = [json.loads(line) for line in node.read_text().splitlines() if line]
    if any(line.get("queued") is True and line.get("entries") == 1 for line in lines):
        raise SystemExit(0)
    time.sleep(0.05)
raise TimeoutError("Python node did not report one queued transient")
PY

node_first_exit_status=0
if [[ "$node_restart" == true || "$node_crash" == true ]]; then
  if [[ "$node_crash" == true ]]; then
    $python - "$initial_node_log" "$node_pid" <<'PY'
import json, os, pathlib, sys, time
path, pid = pathlib.Path(sys.argv[1]), int(sys.argv[2])
deadline = time.monotonic() + 8
while time.monotonic() < deadline:
    os.kill(pid, 0)
    lines = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()]
    settled = next((line for line in lines if line.get("storage_settled") is True), None)
    if settled is not None:
        if settled.get("files", 0) < 1 or settled.get("stored_bytes_positive") is not True:
            raise RuntimeError("queued propagation storage did not produce bounded persistence evidence")
        raise SystemExit(0)
    time.sleep(0.05)
raise TimeoutError("queued propagation storage did not settle before abrupt termination")
PY
    kill -KILL "$node_pid"
    trap - ERR
    set +e
    wait "$node_pid" 2>/dev/null
    node_first_exit_status=$?
    set -e
    trap 'status=$?; echo "mixed propagation harness failed at line $LINENO (status $status)" >&2' ERR
    if ((node_first_exit_status == 0)); then
      echo "abrupt propagation node unexpectedly exited successfully" >&2
      exit 1
    fi
  else
    wait "$node_pid"
    node_first_exit_status=$?
  fi
  node_pid=""
  restart_port=$($python -c \
    'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')
  active_node_log="$temporary_root/node-recovery.jsonl"
  "$python" "$node_script" --python-source "$python_source" \
    --expected-rns "$python_rns_version" --expected-lxmf "$python_lxmf_version" \
    --root "$temporary_root/node" --port "$restart_port" \
    --old-destination "$old_destination" --current-source "$current_destination" \
    >"$active_node_log" 2>"$temporary_root/node-recovery.stderr" &
  node_pid=$!
  $python - "$active_node_log" "$node_pid" "$propagation_destination" <<'PY'
import json, os, pathlib, sys, time
path, pid, expected = pathlib.Path(sys.argv[1]), int(sys.argv[2]), sys.argv[3]
deadline = time.monotonic() + 12
while time.monotonic() < deadline:
    os.kill(pid, 0)
    if path.exists():
        lines = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()]
        ready = next((line for line in lines if line.get("ready") is True), None)
        queued = next((line for line in lines if line.get("queued") is True), None)
        if ready is not None and queued is not None:
            if ready.get("propagation") != expected:
                raise RuntimeError("restarted propagation identity changed")
            if ready.get("restored_entries") != 1 or queued.get("entries") != 1:
                raise RuntimeError("restarted propagation node did not restore one transient")
            raise SystemExit(0)
    time.sleep(0.05)
raise TimeoutError("restarted Python propagation node did not restore the queued transient")
PY
  port=$restart_port
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
  sender_common=("${common_current[@]}")
  recipient_common=("${common_old[@]}")
fi

# The recipient reconnects with the same identity and explicitly syncs one
# transient. Its production implementation must decode and acknowledge it.
sync_report="$temporary_root/$recipient_label-sync.json"
initial_deferred=false
if [[ "$reverse" == true || "$recover_unknown_sender" == true ]]; then
  set +e
  "$recipient_bin" --lxmf-sync-propagation --sync-limit 1 \
    --propagation-node "$propagation_destination" "${recipient_common[@]}" \
    --output "$temporary_root/$recipient_label-sync-initial.json" \
    >"$temporary_root/$recipient_label-sync-initial.stdout" \
    2>"$temporary_root/$recipient_label-sync-initial.stderr"
  initial_sync_status=$?
  set -e
  $python - "$temporary_root/$recipient_label-sync-initial.json" \
    "$initial_sync_status" <<'PY'
import json, pathlib, sys
report = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
messages = [event for event in report.get("sync_events", []) if event.get("kind") == "message_received"]
decode = next(
    (
        event
        for event in report.get("sync_events", [])
        if event.get("kind") == "propagation_sync" and event.get("stage") == "decode"
    ),
    {},
)
counts = decode.get("counts", {})
if (
    int(sys.argv[2]) != 0
    or report.get("sync", {}).get("ok") is not True
    or messages
    or counts.get("decoded") != 0
    or counts.get("deferred") != 1
    or counts.get("sender_path_requests") != 1
):
    raise RuntimeError("initial sync did not preserve one unauthenticated transient")
PY
  initial_deferred=true

  # The first sync deliberately leaves the transient unacknowledged. Learn a
  # fresh authenticated sender announce without resending the logical message,
  # then retry the retained transient from a new recipient process.
  "$recipient_bin" --lxmf-interop --lxmf-wait 8 "${recipient_common[@]}" \
    --output "$temporary_root/$recipient_label-recovery-warm.json" \
    >"$temporary_root/$recipient_label-recovery-warm.stdout" \
    2>"$temporary_root/$recipient_label-recovery-warm.stderr" &
  recipient_pid=$!
  sleep 1
  "$sender_bin" --lxmf-interop --lxmf-wait 5 "${sender_common[@]}" \
    --output "$temporary_root/$sender_label-recovery-announce.json" \
    >"$temporary_root/$sender_label-recovery-announce.stdout" \
    2>"$temporary_root/$sender_label-recovery-announce.stderr" &
  sender_pid=$!
  wait "$sender_pid"
  sender_pid=""
  wait "$recipient_pid"
  recipient_pid=""
fi

sync_attempt=0
while ((sync_attempt < 2)); do
  sync_attempt=$((sync_attempt + 1))
  set +e
  "$recipient_bin" --lxmf-sync-propagation --sync-limit 1 \
    --propagation-node "$propagation_destination" "${recipient_common[@]}" \
    --output "$sync_report" \
    >"$temporary_root/$recipient_label-sync.stdout" \
    2>"$temporary_root/$recipient_label-sync.stderr"
  recipient_sync_status=$?
  set -e
  if ((recipient_sync_status == 0)); then
    break
  fi
  if ((sync_attempt >= 2)); then
    break
  fi
  set +e
  $python - "$sync_report" <<'PY'
import json, pathlib, sys
report = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
messages = [event for event in report.get("sync_events", []) if event.get("kind") == "message_received"]
error = report.get("sync", {}).get("error") or ""
if not messages and "link activation timed out" in error:
    raise SystemExit(0)
raise SystemExit(1)
PY
  retryable_sync=$?
  set -e
  if ((retryable_sync != 0)); then
    break
  fi
done

set +e
wait "$node_pid"
node_status=$?
set -e
node_pid=""

summary="$temporary_root/summary.json"
$python - "$temporary_root/$sender_label-send.json" \
  "$sync_report" \
  "$temporary_root/node.jsonl" "$summary" "$old_commit" "$old_version" \
  "$current_version" "$python_rns_version" "$python_lxmf_version" \
  "$direction" "$sender_label" "$recipient_label" "$sender_status" \
  "$recipient_receive_status" "$recipient_sync_status" "$node_status" \
  "$initial_deferred" "$sync_attempt" "$active_node_log" "$node_restart" \
  "$node_crash" "$node_first_exit_status" "$stamp_ticket" <<'PY'
import json, pathlib, re, sys
send = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
sync = json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))
node_before = [json.loads(line) for line in pathlib.Path(sys.argv[3]).read_text(encoding="utf-8").splitlines()]
node_after = [json.loads(line) for line in pathlib.Path(sys.argv[19]).read_text(encoding="utf-8").splitlines()]
messages = [event for event in sync.get("sync_events", []) if event.get("kind") == "message_received"]
if sync.get("sync", {}).get("ok") is not True or len(messages) != 1:
    def redacted_detail(event):
        detail = event.get("detail")
        if not isinstance(detail, str):
            return None
        return re.sub(r"(?i)\b[0-9a-f]{32,128}\b", "<redacted-hash>", detail)

    diagnostic = {
        "sender_exit_status": int(sys.argv[13]),
        "recipient_receive_exit_status": int(sys.argv[14]),
        "recipient_sync_exit_status": int(sys.argv[15]),
        "node_exit_status": int(sys.argv[16]),
        "sync": sync.get("sync"),
        "sync_events": [
            dict({
                key: event.get(key)
                for key in (
                    "kind",
                    "stage",
                    "status",
                    "has_path",
                    "known_app_data",
                    "link_state",
                    "transfer_state",
                    "counts",
                )
                if event.get(key) is not None
            }, **({"detail": redacted_detail(event)} if redacted_detail(event) else {}))
            for event in sync.get("sync_events", [])
        ],
    }
    raise RuntimeError(
        f"{sys.argv[12]} application did not sync exactly one propagated message: "
        + json.dumps(diagnostic, sort_keys=True)
    )
message = messages[0].get("message", {})
if message.get("transport_method") != "propagated" or message.get("incoming") is not True:
    raise RuntimeError("recipient did not classify the synced message as propagated inbound")
if message.get("peer_hash") != send.get("local", {}).get("local_lxmf_destination_hash"):
    raise RuntimeError("synced sender did not match the sending application identity")
stamp_ticket = sys.argv[23] == "true"
send_result = send.get("send", {})
stamp_cost = send_result.get("native_lxmf_propagation_stamp_cost")
stamp_value = send_result.get("native_lxmf_propagation_stamp_value")
stamp_attempts = send_result.get("native_lxmf_propagation_stamp_attempts")
ticket_value = message.get("fields", {}).get("native_lxmf_reply_ticket")
initial_ready = next(entry for entry in node_before if entry.get("ready") is True)
advertised_stamp_cost = initial_ready.get("advertised_stamp_cost")
stamp_policy_verified = not stamp_ticket or (
    initial_ready.get("stamp_required") is True
    and isinstance(advertised_stamp_cost, int)
    and advertised_stamp_cost > 0
    and stamp_cost == str(advertised_stamp_cost)
    and isinstance(stamp_value, str)
    and stamp_value.isdigit()
    and int(stamp_value) >= 1
    and isinstance(stamp_attempts, str)
    and stamp_attempts.isdigit()
    and int(stamp_attempts) >= 1
)
ticket_wire_verified = not stamp_ticket or (
    send_result.get("include_ticket") is True
    and send_result.get("native_lxmf_include_ticket") == "true"
    and send_result.get("native_lxmf_reply_ticket_offered") == "true"
    and isinstance(ticket_value, str)
    and re.fullmatch(r"[0-9a-f]{32}", ticket_value) is not None
)
if not stamp_policy_verified or not ticket_wire_verified:
    raise RuntimeError(
        "mixed propagation stamp/ticket policy evidence was incomplete: "
        + json.dumps(
            {
                "advertised_stamp_cost": advertised_stamp_cost,
                "stamp_cost_matches": stamp_cost == str(advertised_stamp_cost),
                "stamp_value_numeric": isinstance(stamp_value, str) and stamp_value.isdigit(),
                "stamp_attempts_numeric": isinstance(stamp_attempts, str) and stamp_attempts.isdigit(),
                "include_ticket": send_result.get("include_ticket"),
                "ticket_requested": send_result.get("native_lxmf_include_ticket"),
                "ticket_offered": send_result.get("native_lxmf_reply_ticket_offered"),
                "received_ticket_present": isinstance(ticket_value, str),
                "received_ticket_shape_valid": isinstance(ticket_value, str)
                and re.fullmatch(r"[0-9a-f]{32}", ticket_value) is not None,
            },
            sort_keys=True,
        )
    )
queued = next((entry for entry in node_before if entry.get("queued") is True), None)
ack = next((entry for entry in node_after if entry.get("acknowledged") is True), None)
if queued is None or queued.get("entries") != 1 or ack is None or ack.get("remaining") != 0:
    raise RuntimeError("Python propagation node did not observe queue and acknowledgement boundaries")
node_crashed = sys.argv[21] == "true"
node_restarted = sys.argv[20] == "true" or node_crashed
active_ready = next(entry for entry in node_after if entry.get("ready") is True)
identity_stable = initial_ready.get("propagation") == active_ready.get("propagation")
restart_preserved = not node_restarted or active_ready.get("restored_entries") == 1
summary = {
    "status": "pass",
    "old_source_commit": sys.argv[5],
    "old_application_version": sys.argv[6],
    "current_application_version": sys.argv[7],
    "node": f"python-rns=={sys.argv[8]}/lxmf=={sys.argv[9]}",
    "direction": sys.argv[10],
    "propagated_submit": True,
    "queued_transients": 1,
    f"{sys.argv[12]}_sync_messages": 1,
    "sender_identity_matched": True,
    "message_shape_verified": len(message.get("title", "")) == 32 and len(message.get("content", "")) == 102,
    "acknowledged": True,
    "remaining_transients": 0,
    "sender_exit_success": int(sys.argv[13]) == 0,
    "recipient_receive_exit_success": int(sys.argv[14]) == 0,
    "recipient_sync_exit_success": int(sys.argv[15]) == 0,
    "node_exit_success": int(sys.argv[16]) == 0,
    "initial_unknown_sender_deferred": sys.argv[17] == "true",
    "recipient_sync_attempts": int(sys.argv[18]),
    "node_restarted": node_restarted,
    "node_crashed": node_crashed,
    "abrupt_exit_observed": not node_crashed or int(sys.argv[22]) != 0,
    "propagation_identity_stable": identity_stable,
    "restarted_queue_preserved": restart_preserved,
    "stamp_policy_required": stamp_ticket,
    "stamp_policy_verified": stamp_policy_verified,
    "ticket_included": stamp_ticket,
    "ticket_wire_verified": ticket_wire_verified,
    "old_announce_seen": queued.get("old_announce_seen") is True,
    "current_announce_seen": queued.get("current_announce_seen") is True,
}
if not all((
    summary["message_shape_verified"],
    summary["old_announce_seen"],
    summary["current_announce_seen"],
    summary["sender_exit_success"],
    summary["recipient_receive_exit_success"],
    summary["recipient_sync_exit_success"],
    summary["node_exit_success"],
    summary["propagation_identity_stable"],
    summary["restarted_queue_preserved"],
    summary["abrupt_exit_observed"],
    summary["stamp_policy_verified"],
    summary["ticket_wire_verified"],
)):
    raise RuntimeError("mixed propagation message or announce evidence was incomplete")
pathlib.Path(sys.argv[4]).write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
PY

if [[ -n "$report_path" ]]; then
  mkdir -p -- "$(dirname -- "$report_path")"
  cp -- "$summary" "$report_path"
fi
cat "$summary"
echo "mixed OMENbrowser $direction propagated LXMF: pass"
