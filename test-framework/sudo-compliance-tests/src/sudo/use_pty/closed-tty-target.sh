trap 'touch /tmp/closed-tty-hup; exit 0' HUP
touch /tmp/closed-tty-ready
while :; do
    sleep 0.1
done
