#!/bin/bash

set -euo pipefail

require_command() {
    local command_name="$1"

    if ! command -v "$command_name" >/dev/null 2>&1; then
        echo "Error: required command not found: $command_name" >&2
        exit 1
    fi
}

require_command wmctrl
require_command xprop

current_desktop="$(wmctrl -d | awk '$2 == "*" { print $1; exit }')"

if [ -z "$current_desktop" ]; then
    echo "Error: could not determine the current desktop" >&2
    exit 1
fi

active_window_hex="$(xprop -root _NET_ACTIVE_WINDOW | awk -F' # ' 'NF > 1 { print tolower($2) }')"

mapfile -t windows < <(
    wmctrl -lpG | awk -v desktop="$current_desktop" '
        $2 == desktop || $2 == -1 {
            title = ($9 == "" ? "[untitled]" : $9)
            for (i = 10; i <= NF; i++) {
                title = title " " $i
            }

            printf "%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n",
                tolower($1), $2, $3, $4, $5, $6, $7, title
        }
    '
)

if [ "${#windows[@]}" -eq 0 ]; then
    echo "No windows found on the current desktop." >&2
    exit 1
fi

echo "Windows on the current desktop:"
echo

for index in "${!windows[@]}"; do
    IFS=$'\t' read -r window_id desktop pid x y width height title <<< "${windows[$index]}"

    marker=" "
    if [ "$window_id" = "$active_window_hex" ]; then
        marker="*"
    fi

    printf "%2d) %s %s [%sx%s @ %s,%s]\n" \
        "$((index + 1))" \
        "$marker" \
        "$title" \
        "$width" \
        "$height" \
        "$x" \
        "$y"
done

echo
read -r -p "Select a window number to raise: " selection

if ! [[ "$selection" =~ ^[0-9]+$ ]]; then
    echo "Error: selection must be a number" >&2
    exit 1
fi

if [ "$selection" -lt 1 ] || [ "$selection" -gt "${#windows[@]}" ]; then
    echo "Error: selection out of range" >&2
    exit 1
fi

IFS=$'\t' read -r selected_window_id _ _ _ _ _ _ selected_title <<< "${windows[$((selection - 1))]}"

wmctrl -i -r "$selected_window_id" -b remove,below || true
wmctrl -i -a "$selected_window_id"

echo "Raised: $selected_title"
