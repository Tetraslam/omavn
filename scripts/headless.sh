#!/usr/bin/env bash
# Run a command inside an invisible sway session (GPU-backed, with Xwayland),
# so games can be tested without touching the real desktop.
#
#   scripts/headless.sh start            start the session (idempotent)
#   scripts/headless.sh run <cmd...>     run a command inside it (background)
#   eval "$(scripts/headless.sh env)"    point this shell at the session
#   scripts/headless.sh shot <file.png>  screenshot the virtual output
#   scripts/headless.sh click <x> <y>    click at a position (needs wlrctl)
#   scripts/headless.sh key <wtype args> type or press keys (needs wtype)
#   scripts/headless.sh stop             tear it down
#
# Set OMAVN_HEADLESS_GPU=/dev/dri/renderDXXX to pick the GPU and
# OMAVN_HEADLESS_RES=WxH for the virtual output size.
set -euo pipefail

STATE="${XDG_RUNTIME_DIR:-/tmp}/omavn-headless"
mkdir -p "$STATE"

session_env() {
  export WAYLAND_DISPLAY="$(cat "$STATE/wayland")"
  export DISPLAY="$(cat "$STATE/display")"
  unset HYPRLAND_INSTANCE_SIGNATURE
  SWAYSOCK="$(ls "${XDG_RUNTIME_DIR:-/tmp}"/sway-ipc.*."$(cat "$STATE/pid")".sock 2>/dev/null | head -1)"
  export SWAYSOCK
  # Route audio into a sink nobody listens to.
  if ! pactl list short sinks | grep -q omavn_headless; then
    pactl load-module module-null-sink sink_name=omavn_headless >/dev/null
  fi
  export PULSE_SINK=omavn_headless SDL_AUDIODRIVER=pulseaudio
}

case "${1:-}" in
  start)
    if [[ -f "$STATE/pid" ]] && kill -0 "$(cat "$STATE/pid")" 2>/dev/null; then exit 0; fi
    cat >"$STATE/config" <<EOF
output HEADLESS-1 resolution ${OMAVN_HEADLESS_RES:-1920x1080}
default_border none
xwayland enable
exec sh -c 'echo \$DISPLAY > $STATE/display; echo \$WAYLAND_DISPLAY > $STATE/wayland'
EOF
    rm -f "$STATE/display" "$STATE/wayland"
    gpu="${OMAVN_HEADLESS_GPU:-$(ls /dev/dri/renderD* | head -1)}"
    env -u WAYLAND_DISPLAY -u DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
      WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDER_DRM_DEVICE="$gpu" \
      setsid sway -c "$STATE/config" --unsupported-gpu >"$STATE/sway.log" 2>&1 &
    echo $! >"$STATE/pid"
    for _ in $(seq 100); do [[ -s "$STATE/wayland" ]] && break; sleep 0.1; done
    [[ -s "$STATE/wayland" ]] || { tail -20 "$STATE/sway.log"; exit 1; }
    ;;
  env)
    session_env
    env | grep -E "^(WAYLAND_DISPLAY|DISPLAY|SWAYSOCK|PULSE_SINK|SDL_AUDIODRIVER)=" | sed "s/^/export /"
    ;;
  run)
    shift
    session_env
    setsid "$@" >"$STATE/last-run.log" 2>&1 &
    echo $!
    ;;
  shot)
    session_env
    grim "${2:?usage: shot <file.png>}"
    ;;
  click)
    session_env
    # Virtual pointer moves are relative: slam into the corner, then go.
    wlrctl pointer move -10000 -10000
    wlrctl pointer move "${2:?usage: click <x> <y>}" "${3:?}"
    wlrctl pointer click "${4:-left}"
    ;;
  key)
    session_env
    shift
    wtype "$@"
    ;;
  stop)
    [[ -f "$STATE/pid" ]] && kill "$(cat "$STATE/pid")" 2>/dev/null || true
    rm -f "$STATE/pid" "$STATE/display" "$STATE/wayland"
    ;;
  *)
    sed -n '2,14p' "$0"; exit 2 ;;
esac
