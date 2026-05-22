pipe=/tmp/closed-tty-pipe-in
rm -f "$pipe"
mkfifo "$pipe"
(sleep 60) >"$pipe" &
writer_pid=$!

cleanup() {
    kill "$writer_pid" 2>/dev/null || true
    rm -f "$pipe"
}

trap 'touch /tmp/closed-tty-pipe-hup; exit 0' HUP
trap cleanup EXIT INT TERM
touch /tmp/closed-tty-pipe-ready
while :; do
    sleep 0.1
done <"$pipe"
