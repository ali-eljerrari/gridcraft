#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
runtime_dir="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"

export XDG_RUNTIME_DIR="$runtime_dir"

# Prefer the active Wayland socket. The desktop session used by this machine is
# wayland-1, but selecting an existing socket keeps this usable after logins
# create a different display number.
if [[ -z "${WAYLAND_DISPLAY:-}" ]]; then
    for socket in wayland-1 wayland-0; do
        if [[ -S "$runtime_dir/$socket" ]]; then
            export WAYLAND_DISPLAY="$socket"
            break
        fi
    done
fi

if [[ -n "${WAYLAND_DISPLAY:-}" && -S "$runtime_dir/$WAYLAND_DISPLAY" ]]; then
    unset DISPLAY
elif [[ -S /tmp/.X11-unix/X0 ]]; then
    export DISPLAY=:0
    unset WAYLAND_DISPLAY
else
    printf 'GridCraft: no Wayland or X11 display was found.\n' >&2
    exit 1
fi

if (($# == 0)); then
    set -- --sample sales
fi

exec "${GRIDCRAFT_BIN:-$repo_dir/target/release/gridcraft}" "$@"
