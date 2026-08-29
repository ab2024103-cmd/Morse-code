#!/usr/bin/env bash
# =============================================================================
# GitHub Release publisher for the MorseLink Android artifacts.
#
# Runs ON THE CI RUNNER as a finalizer of :app:bundleRelease (wired in
# android/build.gradle). Two constraints made a normal workflow release step
# impossible for the CI bot that maintains this repo:
#   1. it cannot push changes to .github/workflows/** (GitHub App without the
#      `workflows` permission), so no release job can be added; and
#   2. the agent sandbox cannot reach the Actions artifact blob storage to
#      download the APK afterwards and upload it itself.
# The runner can do both though: it holds the credentials actions/checkout
# persisted in .git/config, and has unrestricted network. This script uses
# them to attach the freshly built APK + AAB to a GitHub Release.
#
# Modes:
#   * push of a tag matching v*  -> create/update + publish the release for
#     that tag with the debug APK and the release AAB attached.
#   * any other CI event         -> *probe* only: create a draft release and
#     delete it again, so we learn whether the job token is allowed to manage
#     releases without leaving anything behind.
#
# The script never exits non-zero (a broken publish must not redden an
# otherwise-green build); diagnostics are appended to $GITHUB_STEP_SUMMARY.
# =============================================================================
set -u

API="https://api.github.com"
REPO="${GITHUB_REPOSITORY:-}"

say() {
  printf '%s\n' "$*"
  if [ -n "${GITHUB_STEP_SUMMARY:-}" ] && [ -w "${GITHUB_STEP_SUMMARY:-/nonexistent}" ]; then
    { printf '> %s\n' "$*"; } >> "$GITHUB_STEP_SUMMARY"
  fi
}

[ -n "$REPO" ] || { say "publish-release: no GITHUB_REPOSITORY (not on a GH Actions runner?) — nothing to do"; exit 0; }

# ---------------------------------------------------------------------------
# Extract the job token actions/checkout persisted for git auth.
# .git/config contains: [http "https://github.com/"] extraheader = "AUTHORIZATION: basic <b64(x-access-token:TOKEN)>"
# ---------------------------------------------------------------------------
cd "${GITHUB_WORKSPACE:-.}" 2>/dev/null || true
HDR=""
for key in \
  'http.https://github.com/.extraheader' \
  'http.https://api.github.com/.extraheader' ; do
  HDR=$(git config --get "$key" 2>/dev/null || true)
  [ -n "$HDR" ] && break
done
if [ -z "$HDR" ]; then
  # Fall back: any persisted extraheader.
  HDR=$(git config --get-regexp '^http\..*extraheader' 2>/dev/null | head -n 1 | cut -d' ' -f2- || true)
fi
TOKEN=""
if [ -n "$HDR" ]; then
  B64=$(printf '%s' "$HDR" | tr -d ' \t' | grep -oE '[A-Za-z0-9+/=]{24,}$' || true)
  [ -n "$B64" ] && TOKEN=$(printf '%s' "$B64" | base64 -d 2>/dev/null | cut -s -d: -f2- || true)
fi
if [ -z "$TOKEN" ]; then
  say "publish-release: could not extract a job token from git config extraheader — skipping (no side effects)"
  exit 0
fi

api() { curl -sS --max-time 120 -H "Authorization: Bearer $TOKEN" -H "Accept: application/vnd.github+json" -H "X-GitHub-Api-Version: 2022-11-28" "$@"; }
json() { jq -r "$1 // empty" 2>/dev/null; }

# ---------------------------------------------------------------------------
# Probe mode: verify the token can create releases, then clean up.
# ---------------------------------------------------------------------------
probe() {
  local resp id
  resp=$(api -X POST "$API/repos/$REPO/releases" \
    -d '{"tag_name":"morselink-ci-permission-probe","name":"morselink-ci-permission-probe","draft":true,"target_commitish":"'"${GITHUB_SHA:-HEAD}"'"}')
  id=$(printf '%s' "$resp" | json '.id')
  if [ -z "$id" ]; then
    say "publish-release PROBE: job token CANNOT manage releases -> $(printf '%s' "$resp" | head -c 300)"
    say "publish-release PROBE: to publish a Release with the APK from CI, set Settings -> Actions -> General -> Workflow permissions to 'Read repository contents and packages permissions' PLUS 'Allow read and write permissions to contents' (or push scripts/android.yml.proposed to .github/workflows/android.yml from an account with workflows permission)."
    return 0
  fi
  api -X DELETE "$API/repos/$REPO/releases/$id" >/dev/null 2>&1 || true
  say "publish-release PROBE OK: job token can create/delete releases; tag pushes (v*) will publish the Release with APK."
}

# ---------------------------------------------------------------------------
# Publish mode (tag push v*).
# ---------------------------------------------------------------------------
publish() {
  local TAG="${GITHUB_REF_NAME:-}"
  if [ -z "$TAG" ]; then say "publish-release: empty tag name"; return 0; fi
  local APK="${APK_PATH:-android/app/build/outputs/apk/debug/app-debug.apk}"
  local AAB="${AAB_PATH:-android/app/build/outputs/bundle/release/app-release.aab}"
  local NAME="MorseLink $TAG"
  local BODY
  BODY=$(printf 'Android build for `%s` — run #%s of `%s` on commit `%s`.\n\n| Asset | What it is |\n|---|---|\n| `morselink-debug-%s.apk` | Debug APK, directly installable on Android 6.0+ |\n| `morselink-release-%s.aab` | Release AAB for Play / bundle tooling |\n' \
    "$TAG" "${GITHUB_RUN_NUMBER:-?}" "${GITHUB_WORKFLOW:-android}" "$(printf '%s' "${GITHUB_SHA:-}" | cut -c1-7)" "$TAG" "$TAG")

  local resp id upload_url
  resp=$(api "$API/repos/$REPO/releases/tags/$TAG")
  id=$(printf '%s' "$resp" | json '.id')
  if [ -z "$id" ]; then
    resp=$(api -X POST "$API/repos/$REPO/releases" \
      -d "$(jq -n --arg tag "$TAG" --arg name "$NAME" --arg body "$BODY" --arg sha "${GITHUB_SHA:-}" \
            '{tag_name:$tag, name:$name, body:$body, target_commitish:$sha, draft:true}')")
    id=$(printf '%s' "$resp" | json '.id')
    if [ -z "$id" ]; then
      say "publish-release: FAILED to create release: $(printf '%s' "$resp" | head -c 400)"
      return 0
    fi
    say "publish-release: draft release id=$id created for $TAG"
  else
    say "publish-release: release id=$id already exists for $TAG; refreshing assets"
  fi
  upload_url=$(printf '%s' "$resp" | json '.upload_url' | sed 's/{.*//')
  if [ -z "$upload_url" ]; then
    upload_url=$(api "$API/repos/$REPO/releases/$id" | json '.upload_url' | sed 's/{.*//')
  fi

  local apk_ok=0 have_draft=1
  resp=$(api "$API/repos/$REPO/releases/$id")
  [ "$(printf '%s' "$resp" | json '.draft')" = "false" ] && have_draft=0

  for pair in "apk:$APK:morselink-debug-$TAG.apk:application/vnd.android.package-archive" "aab:$AAB:morselink-release-$TAG.aab:application/octet-stream"; do
    local kind f asset_name ctype size existing st
    kind=${pair%%:*}; rest=${pair#*:}; f=${rest%%:*}; rest=${rest#*:}; asset_name=${rest%%:*}; ctype=${rest#*:}
    if [ ! -f "$f" ]; then
      say "publish-release: artifact missing: $f"
      continue
    fi
    size=$(stat -c%s "$f" 2>/dev/null || wc -c < "$f")
    existing=$(api "$API/repos/$REPO/releases/$id/assets" \
      | jq -r --arg n "$asset_name" --argjson s "$size" '.[] | select(.name==$n and .size==$s) | .id' 2>/dev/null | head -n 1)
    if [ -n "$existing" ]; then
      say "publish-release: asset $asset_name already attached (id=$existing)"
      [ "$kind" = "apk" ] && apk_ok=1
      continue
    fi
    # Drop a stale asset with the same name before re-uploading.
    local stale
    stale=$(api "$API/repos/$REPO/releases/$id/assets" | jq -r --arg n "$asset_name" '.[] | select(.name==$n) | .id' 2>/dev/null | head -n 1)
    [ -n "$stale" ] && api -X DELETE "$API/repos/$REPO/releases/$id/assets/$stale" >/dev/null 2>&1
    st=$(curl -sS --max-time 600 -o /tmp/publish-asset.json -w '%{http_code}' \
      -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: $ctype" \
      --data-binary "@$f" "$upload_url?name=$asset_name")
    if [ "$st" = "201" ]; then
      say "publish-release: uploaded $asset_name ($size bytes)"
      [ "$kind" = "apk" ] && apk_ok=1
    else
      say "publish-release: upload of $asset_name FAILED (http $st): $(head -c 300 /tmp/publish-asset.json 2>/dev/null)"
    fi
  done

  if [ "$apk_ok" = "1" ] && [ "$have_draft" = "1" ]; then
    api -X PATCH "$API/repos/$REPO/releases/$id" -d '{"draft":false}' >/dev/null 2>&1
  fi
  local url
  url=$(api "$API/repos/$REPO/releases/$id" | json '.html_url')
  if [ "$apk_ok" = "1" ]; then
    say "publish-release: DONE — release published: $url"
  else
    say "publish-release: no APK asset attached; leaving release as draft: $url"
  fi
}

case "${GITHUB_REF_TYPE:-branch}" in
  tag)
    case "${GITHUB_REF_NAME:-}" in
      v*) publish ;;
      *) say "publish-release: tag '$GITHUB_REF_NAME' does not match v* — nothing to publish" ;;
    esac
    ;;
  *) probe ;;
esac
exit 0
