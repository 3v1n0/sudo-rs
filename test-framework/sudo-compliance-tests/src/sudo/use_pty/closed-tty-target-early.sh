trap 'touch /tmp/closed-tty-early-hup; exit 0' HUP
touch /tmp/closed-tty-early-started
sleep 2
while :; do
    sleep 0.1
done
