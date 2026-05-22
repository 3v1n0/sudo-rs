#!/usr/bin/sh
# Target for pty-leader revoke test.
# Usage: closed-tty-revoke-target.sh <ready-file> <detected-file>
# Ignores SIGHUP; detects when /dev/tty becomes unusable (EIO).

trap '' HUP

exec 3>/dev/tty || exit 1
echo x >&3 || exit 1

touch "$1"

while echo x >&3 2>/dev/null; do
    sleep 0.1
done

touch "$2"
exit 0
