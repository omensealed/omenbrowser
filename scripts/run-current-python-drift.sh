#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
readonly repo_root
readonly rns_version=1.3.8
readonly lxmf_version=1.0.1
readonly nomadnet_version=1.2.7
readonly msgpack_version=1.2.1

report_path=""
case $# in
  0)
    ;;
  2)
    if [[ "$1" != "--report" ]]; then
      echo "usage: $0 [--report /path/to/report.json]" >&2
      exit 2
    fi
    report_path=$2
    ;;
  *)
    echo "usage: $0 [--report /path/to/report.json]" >&2
    exit 2
    ;;
esac

temporary_root=$(mktemp -d "${TMPDIR:-/tmp}/omen-current-python-drift.XXXXXX")
cleanup() {
  rm -rf -- "$temporary_root"
}
trap cleanup EXIT

python3 -m venv "$temporary_root/venv"
python="$temporary_root/venv/bin/python"
"$python" -m pip install --disable-pip-version-check --no-input --quiet \
  "rns==$rns_version" \
  "lxmf==$lxmf_version" \
  "nomadnet==$nomadnet_version" \
  "msgpack==$msgpack_version"

site_packages=$("$python" -c 'import site; print(site.getsitepackages()[0])')
stack_json=$("$python" -c 'import importlib.metadata as m, json, platform; resolved=sorted(f"{d.metadata.get('"'"'Name'"'"', '"'"'unknown'"'"')}=={d.version}" for d in m.distributions()); print(json.dumps({"lane":"informational","python":platform.python_version(),"pip":m.version("pip"),"rns":m.version("rns"),"lxmf":m.version("lxmf"),"nomadnet":m.version("nomadnet"),"msgpack":m.version("msgpack"),"resolved_distributions":resolved}, sort_keys=True))')
echo "current Python stack: $stack_json"
"$python" -c 'import RNS, LXMF, nomadnet; print("current Python stack import: pass")'

"$python" "$repo_root/scripts/verify-ifac-python-vector.py" \
  --rns-source "$site_packages" \
  --rns-version "$rns_version"

OMEN_PYTHON_RNS_SOURCE="$site_packages" \
OMEN_PYTHON_RNS_VERSION="$rns_version" \
  cargo test --locked --manifest-path "$repo_root/src/server/Cargo.toml" \
  -p omen-ifac-tcp --test pinned_python_tcp -- \
  --ignored --nocapture --test-threads=1

OMEN_PYTHON_RNS_SOURCE="$site_packages" \
OMEN_PYTHON_RNS_VERSION="$rns_version" \
  cargo test --locked --manifest-path "$repo_root/src/server/Cargo.toml" \
  -p omen-ifac-tcp --test pinned_python_reticulum -- \
  --ignored --nocapture --test-threads=1

OMEN_PYTHON_RNS_SOURCE="$site_packages" \
OMEN_PYTHON_RNS_VERSION="$rns_version" \
  cargo test --locked --manifest-path "$repo_root/Cargo.toml" \
  --lib --no-default-features --features desktop-product \
  current_python_lxmf -- \
  --ignored --nocapture --test-threads=1

OMEN_PYTHON_NOMADNET_SOURCE="$site_packages" \
  cargo test --locked --manifest-path "$repo_root/Cargo.toml" \
  --lib --no-default-features --features desktop-product \
  current_python_nomadnet_request_response_primitive_matrix_preserves_exact_bytes -- \
  --ignored --nocapture --test-threads=1

OMEN_PYTHON_NOMADNET_SOURCE="$site_packages" \
  cargo test --locked --manifest-path "$repo_root/Cargo.toml" \
  --lib --no-default-features --features desktop-product \
  current_python_nomadnet_timeout_and_cancellation_are_bounded_without_replay -- \
  --ignored --nocapture --test-threads=1

OMEN_PYTHON_NOMADNET_SOURCE="$site_packages" \
  cargo test --locked --manifest-path "$repo_root/Cargo.toml" \
  --lib --no-default-features --features desktop-product \
  current_python_nomadnet_repeated_requests_reuse_one_active_link -- \
  --ignored --nocapture --test-threads=1

OMEN_PYTHON_NOMADNET_SOURCE="$site_packages" \
  cargo test --locked --manifest-path "$repo_root/Cargo.toml" \
  --lib --no-default-features --features desktop-product \
  current_python_nomadnet_measures_direct_and_request_resource_on_one_link -- \
  --ignored --nocapture --test-threads=1

OMEN_PYTHON_NOMADNET_SOURCE="$site_packages" \
  cargo test --locked --manifest-path "$repo_root/Cargo.toml" \
  --lib --no-default-features --features desktop-product \
  current_python_nomadnet_retained_link_keepalive_and_recovery_are_bounded -- \
  --ignored --nocapture --test-threads=1

release_measurement="$temporary_root/nomadnet-release-measurement.json"
OMEN_PYTHON_NOMADNET_SOURCE="$site_packages" \
OMEN_REQUIRE_OPTIMIZED_NOMADNET_MEASUREMENT=1 \
OMEN_NOMADNET_MEASUREMENT_REPORT="$release_measurement" \
  cargo test --locked --release --manifest-path "$repo_root/Cargo.toml" \
  --lib --no-default-features --features desktop-product \
  current_python_nomadnet_measures_direct_and_request_resource_on_one_link -- \
  --ignored --nocapture --test-threads=1

if [[ -n "$report_path" ]]; then
  mkdir -p -- "$(dirname -- "$report_path")"
  "$python" -c 'import json, pathlib, sys; data=json.loads(sys.argv[2]); data["status"]="pass"; data["checks"]=["reticulum_vectors", "ifac_tcp", "reticulum_link_proof", "bidirectional_rust_python_lxmf_direct_delivery", "python_propagation_node_rust_sync_ack", "rust_python_propagation_stamp_boundaries", "network_propagation_stamp_accept_reject", "first_send_direct_policy_discovery", "stamped_direct_resource_delivery", "direct_stamp_accept_reject", "ticket_issue_use_expiry_reuse", "live_ticket_roundtrip", "nomadnet_request_response_primitive_matrix", "nomadnet_timeout_cancellation_no_replay", "nomadnet_repeated_request_link_reuse", "nomadnet_direct_request_resource_measurement", "nomadnet_retained_link_keepalive_recovery_soak", "nomadnet_release_direct_request_resource_measurement"]; data["measurements"]={"nomadnet_release": json.loads(pathlib.Path(sys.argv[3]).read_text(encoding="utf-8"))}; pathlib.Path(sys.argv[1]).write_text(json.dumps(data, indent=2, sort_keys=True)+"\n", encoding="utf-8")' \
    "$report_path" "$stack_json" "$release_measurement"
fi

echo "current Python drift interoperability: pass (informational; RNS $rns_version / LXMF $lxmf_version / NomadNet $nomadnet_version)"
