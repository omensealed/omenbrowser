#!/usr/bin/env bash
set -euo pipefail

dist_dir="${1:-dist}"
tag="${GITHUB_REF_NAME:-${1:-}}"
repo="${GITHUB_REPOSITORY:-omensealed/omenbrowser}"
token="${GITHUB_TOKEN:?GITHUB_TOKEN is required}"

if [[ -z "$tag" || "$tag" == "$dist_dir" ]]; then
  echo "release upload needs GITHUB_REF_NAME set to the tag name" >&2
  exit 2
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required for release upload" >&2
  exit 127
fi

api="https://api.github.com/repos/${repo}"
auth_headers=(
  -H "Authorization: Bearer ${token}"
  -H "Accept: application/vnd.github+json"
)

release_json="$(mktemp)"
release_status="$(
  curl -sS -w '%{http_code}' -o "$release_json" \
    "${auth_headers[@]}" \
    "${api}/releases/tags/${tag}"
)"

if [[ "$release_status" == "404" ]]; then
  notes="$(cat <<EOF
Public release package build for ${tag}.

Downloads:
- Debian package for Debian/Ubuntu/Mint-compatible systems
- AppImage for portable desktop use
- Release tarball for manual isolated testing
- SHA-256 checksum files
EOF
)"
  jq -n \
    --arg tag "$tag" \
    --arg name "OMENbrowser_rs ${tag#v}" \
    --arg body "$notes" \
    '{tag_name:$tag, name:$name, body:$body, draft:false, prerelease:false, make_latest:"true"}' \
    > "$release_json.request"
  curl -fsS -X POST \
    "${auth_headers[@]}" \
    -H "Content-Type: application/json" \
    --data @"$release_json.request" \
    "${api}/releases" \
    > "$release_json"
elif [[ "$release_status" == "200" ]]; then
  release_id="$(jq -r '.id' "$release_json")"
  jq -n '{draft:false, prerelease:false, make_latest:"true"}' > "$release_json.request"
  curl -fsS -X PATCH \
    "${auth_headers[@]}" \
    -H "Content-Type: application/json" \
    --data @"$release_json.request" \
    "${api}/releases/${release_id}" \
    > "$release_json"
else
  echo "failed to read release ${tag}: HTTP ${release_status}" >&2
  cat "$release_json" >&2 || true
  exit 1
fi

release_id="$(jq -r '.id' "$release_json")"
upload_base="https://uploads.github.com/repos/${repo}/releases/${release_id}/assets"
version="${tag#v}"

mapfile -t assets < <(
  find "$dist_dir" -maxdepth 1 -type f \
    \( -name 'OMENbrowser_rs-latest.tar.gz' \
    -o -name 'OMENbrowser_rs-latest.tar.gz.sha256' \
    -o -name 'OMENbrowser_rs-latest.txt' \
    -o -name "omenbrowser-rs_${version}_amd64.deb" \
    -o -name "omenbrowser-rs_${version}_amd64.deb.sha256" \
    -o -name "OMENbrowser_rs-${version}-x86_64.AppImage" \
    -o -name "OMENbrowser_rs-${version}-x86_64.AppImage.sha256" \
    -o -name 'release-artifacts.sha256' \) \
    | sort
)

if [[ "${#assets[@]}" -eq 0 ]]; then
  echo "no release assets found in ${dist_dir}" >&2
  exit 1
fi

existing_json="$(mktemp)"
curl -fsS "${auth_headers[@]}" "${api}/releases/${release_id}/assets?per_page=100" > "$existing_json"

for path in "${assets[@]}"; do
  name="$(basename "$path")"
  existing_id="$(jq -r --arg name "$name" '.[] | select(.name == $name) | .id' "$existing_json" | head -n 1)"
  if [[ -n "$existing_id" ]]; then
    echo "replacing existing release asset: $name"
    curl -fsS -X DELETE "${auth_headers[@]}" "${api}/releases/assets/${existing_id}" >/dev/null
  fi

  encoded_name="$(python3 -c 'import urllib.parse,sys; print(urllib.parse.quote(sys.argv[1]))' "$name")"
  echo "uploading release asset: $name"
  curl -fsS -X POST \
    "${auth_headers[@]}" \
    -H "Content-Type: application/octet-stream" \
    --data-binary @"$path" \
    "${upload_base}?name=${encoded_name}" \
    | jq -r '.name + " -> " + .browser_download_url'
done
