trap '' HUP

# Open /dev/tty and confirm it is alive before signaling readiness
exec 3>/dev/tty || exit 1
echo x >&3
echo x >&3

touch /tmp/pty-revoke-ready

# Poll: when the pty leader is closed, writes to /dev/tty fail with EIO
while echo x >&3 2>/dev/null; do
    sleep 0.1
done

touch /tmp/pty-revoke-detected
exit 0
