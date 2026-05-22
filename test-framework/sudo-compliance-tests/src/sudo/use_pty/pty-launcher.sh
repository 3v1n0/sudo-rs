#!/usr/bin/env sh
# Create a PTY, run a command inside it, and close the PTY on SIGUSR1.

if [ $# -lt 1 ]; then
    echo "usage: ${0##*/} <program> [args...]" >&2
    exit 2
fi

fifo="/tmp/pty-launcher.$$.fifo"
mkfifo "$fifo" || exit 1

socat "OPEN:$fifo,rdonly" "EXEC:$*,pty,setsid,ctty,stderr" &
socat_pid=$!

exec 3>"$fifo"
rm -f "$fifo"

tty_closed=0
close_tty() {
    if [ "$tty_closed" -eq 0 ]; then
        tty_closed=1
        exec 3>&-
        kill -9 "$socat_pid" 2>/dev/null || true
    fi
}

trap close_tty USR1
trap close_tty TERM INT

wait "$socat_pid" 2>/dev/null
exit $?
