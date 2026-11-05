#!/usr/bin/env bash
# tauri-action writes api.github.com asset URLs into latest.json while a release
# is a draft. Those are limited to 60 anonymous requests an hour per IP, so every
# installed app would share that budget. Point them at the public download URLs.
set -euo pipefail

tag="$1"
repository="${GH_REPO:-$GITHUB_REPOSITORY}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

gh release download "$tag" --pattern latest.json --dir "$work"
gh release view "$tag" --json assets >"$work/assets.json"

jq --slurpfile release "$work/assets.json" \
  --arg base "https://github.com/$repository/releases/download/$tag" '
  ($release[0].assets | map({key: .apiUrl, value: .name}) | from_entries) as $names
  | .platforms |= map_values(
      if $names[.url] then .url = "\($base)/\($names[.url])"
      else error("unknown asset: \(.url)") end)
' "$work/latest.json" >"$work/rewritten.json"

mv "$work/rewritten.json" "$work/latest.json"
gh release upload "$tag" "$work/latest.json" --clobber
