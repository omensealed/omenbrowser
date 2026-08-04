#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
test_root="$(mktemp -d "${TMPDIR:-/tmp}/omenchatd-private-service.XXXXXX")"
trap 'rm -rf "$test_root"' EXIT

parent="$test_root/operator parent"
home="$parent/server home"
fake_home="$test_root/user home"
fake_bin="$test_root/bin"
mkdir -p "$parent" "$home" "$fake_home" "$fake_bin"
chmod 0755 "$parent" "$home"

cat > "$fake_bin/systemctl" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
chmod 0755 "$fake_bin/systemctl"

PATH="$fake_bin:$PATH" \
HOME="$fake_home" \
XDG_CONFIG_HOME="$fake_home/config" \
  bash "$repo_root/scripts/install-omenchatd-user-service.sh" \
    --bin /bin/true \
    --home "$home" \
    --unit omenchatd-private-test >/dev/null

unit="$fake_home/config/systemd/user/omenchatd-private-test.service"
test "$(stat -c '%a' "$home")" = "700"
test "$(stat -c '%a' "$parent")" = "755"
test "$(stat -c '%a' "$unit")" = "600"
grep -q '^UMask=0077$' "$unit"

PATH="$fake_bin:$PATH" \
HOME="$fake_home" \
XDG_CONFIG_HOME="$fake_home/config" \
  bash "$repo_root/scripts/install-omenchatd-user-service.sh" \
    --home "$home" \
    --unit omenchatd-private-test \
    --uninstall >/dev/null

test -d "$home"
test "$(stat -c '%a' "$home")" = "700"
test "$(stat -c '%a' "$parent")" = "755"
test ! -e "$unit"

outside="$test_root/outside"
linked_home="$parent/linked home"
mkdir -p "$outside"
chmod 0755 "$outside"
ln -s "$outside" "$linked_home"
if PATH="$fake_bin:$PATH" \
  HOME="$fake_home" \
  XDG_CONFIG_HOME="$fake_home/config" \
    bash "$repo_root/scripts/install-omenchatd-user-service.sh" \
      --bin /bin/true \
      --home "$linked_home" \
      --unit omenchatd-private-test >/dev/null 2>&1; then
  echo "installer unexpectedly accepted a symlinked home" >&2
  exit 1
fi
test "$(stat -c '%a' "$outside")" = "755"

echo "omenchatd private service installer: pass"
