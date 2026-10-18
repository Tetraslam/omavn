#!/usr/bin/env bash
# Compatibility sweep: install each download with omavn, launch it in the
# invisible test session, and check it is still running and drawing
# something. Writes results.jsonl, a markdown table, screenshots, and a
# contact sheet (sheet.png) to eyeball: an error dialog also "draws".
#
#   scripts/compat.sh [-o OUT] [-w SECONDS] <download>...
#
# Every game is installed into OUT/home (a throwaway OMAVN_HOME), so your
# real library is untouched. Uses the omavn on PATH unless OMAVN_BIN is set.
set -uo pipefail

out=/tmp/omavn-compat
wait_secs=45
while getopts "o:w:" opt; do
  case $opt in
    o) out=$OPTARG ;;
    w) wait_secs=$OPTARG ;;
    *) sed -n '2,9p' "$0"; exit 2 ;;
  esac
done
shift $((OPTIND - 1))
(($#)) || { sed -n '2,9p' "$0"; exit 2; }

here="$(cd "$(dirname "$0")" && pwd)"
omavn="${OMAVN_BIN:-omavn}"
mkdir -p "$out/shots" "$out/logs"
out="$(cd "$out" && pwd)"
"$here/headless.sh" start
eval "$("$here/headless.sh" env)"

# Only omavn and the games it starts carry this marker, so cleanup can't
# hit the test session or anything else.
export OMAVN_HOME="$out/home"
marker="OMAVN_COMPAT_RUN=$$-$RANDOM"
run() { env "$marker" "$@"; }

# Kill everything a game left behind.
reap() {
  local pids=()
  for env in /proc/[0-9]*/environ; do
    local pid=${env#/proc/}
    pid=${pid%/environ}
    [[ $pid == "$$" ]] && continue
    grep -qaF "$marker" "$env" 2>/dev/null && pids+=("$pid")
  done
  ((${#pids[@]})) || return 0
  kill "${pids[@]}" 2>/dev/null
  sleep 3
  kill -9 "${pids[@]}" 2>/dev/null
  return 0
}

json() { jq -c -n "$@"; }

: >"$out/results.jsonl"
for src in "$@"; do
  name=$(basename "$src")
  slug=$(printf '%s' "$name" | tr -c '[:alnum:]._-' '_')
  echo "== $name"
  start=$SECONDS
  if ! run "$omavn" --json install "$src" --offline >"$out/logs/$slug.install.json" 2>"$out/logs/$slug.install.err"; then
    err=$(jq -r 'select(.event=="error") | .message' "$out/logs/$slug.install.err" 2>/dev/null | tail -1)
    json --arg n "$name" --arg e "${err:-install failed}" --argjson t $((SECONDS - start)) \
      '{download:$n, install:"fail", seconds:$t, error:$e}' >>"$out/results.jsonl"
    reap
    continue
  fi
  install_secs=$((SECONDS - start))
  id=$(jq -r .id "$out/logs/$slug.install.json")
  engine=$(jq -r .engine "$out/logs/$slug.install.json")
  runner=$(jq -r .runner "$out/logs/$slug.install.json")
  warns=$(jq -rs '[.[] | select(.event=="warn") | .message] | join("; ")' "$out/logs/$slug.install.err")

  setsid env "$marker" "$omavn" run "$id" >"$out/logs/$slug.run.out" 2>&1 &
  # Games take a while to start (and some open with a black intro), so look
  # a few times and keep the most varied screenshot.
  shot="$out/shots/$slug.png"
  spread=0
  for t in 1 2 3; do
    sleep $((wait_secs / 3))
    grim "$out/shots/.try.png" 2>/dev/null || continue
    s=$(magick "$out/shots/.try.png" -colorspace Gray -format '%[fx:standard_deviation]' info: 2>/dev/null || echo 0)
    if awk -v a="$s" -v b="$spread" 'BEGIN { exit !(a >= b) }'; then
      spread=$s
      mv "$out/shots/.try.png" "$shot"
    fi
  done
  rm -f "$out/shots/.try.png"
  alive=false
  # Still running = something besides the omavn wrapper carries the marker.
  for env in /proc/[0-9]*/environ; do
    grep -qaF "$marker" "$env" 2>/dev/null || continue
    comm=$(cat "${env%/environ}/comm" 2>/dev/null)
    [[ $comm == omavn ]] || { alive=true; break; }
  done
  # A blank or single-color screen has (almost) no variation.
  drawing=$(awk -v s="$spread" 'BEGIN { print (s > 0.03) ? "true" : "false" }')
  cp "$OMAVN_HOME/data/games/$id/last-run.log" "$out/logs/$slug.game.log" 2>/dev/null
  reap
  json --arg n "$name" --arg id "$id" --arg e "$engine" --arg r "$runner" \
    --argjson t "$install_secs" --argjson a "$alive" --argjson d "$drawing" \
    --arg s "$spread" --arg w "$warns" --arg shot "shots/$slug.png" \
    '{download:$n, install:"ok", id:$id, engine:$e, runner:$r, seconds:$t,
      running:$a, drawing:$d, spread:($s|tonumber), warnings:$w, screenshot:$shot}' \
    >>"$out/results.jsonl"
  tail -1 "$out/results.jsonl"
done

{
  echo "| download | install | engine | runner | running | drawing | notes |"
  echo "| --- | --- | --- | --- | --- | --- | --- |"
  jq -r '"| \(.download) | \(.install) (\(.seconds)s) | \(.engine // "") | \(.runner // "") | \(.running // "") | \(.drawing // "") | \(.error // .warnings // "") |"' \
    "$out/results.jsonl"
} >"$out/results.md"
# Error dialogs count as "drawing", so leave a sheet for a human glance.
if compgen -G "$out/shots/*.png" >/dev/null; then
  magick montage "$out"/shots/*.png -tile 4x -geometry 480x300+4+4 "$out/sheet.png" 2>/dev/null
fi
cat "$out/results.md"
echo "screenshots: $out/sheet.png"
