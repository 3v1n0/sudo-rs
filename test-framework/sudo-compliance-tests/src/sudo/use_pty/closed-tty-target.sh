#!/usr/bin/sh
# Target for closed-tty tests.
# Usage: closed-tty-target.sh <ready-file> <hup-file> [early|pipe]
# Touches <ready-file> when ready, touches <hup-file> on SIGHUP.
# Modes:
#   (default)  touch ready, then loop
#   early     touch ready, sleep 2, then loop (simulates slow startup)
#   pipe      redirect stdin from /dev/null, touch ready, then loop

trap 'touch "$2"; exit 0' HUP

case "$3" in
    early)  touch "$1"; sleep 2 ;;
    pipe)  exec 0</dev/null; touch "$1" ;;
    *)     touch "$1" ;;
esac

while :; do sleep 0.1; done
