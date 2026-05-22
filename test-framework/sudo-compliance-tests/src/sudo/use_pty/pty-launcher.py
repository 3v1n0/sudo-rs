#!/usr/bin/env python3
import fcntl
import os
import pty
import signal
import sys
import termios
import time


if len(sys.argv) < 2:
    print("usage: pty-launcher.py <program> [args...]", file=sys.stderr)
    sys.exit(2)

master_fd, slave_fd = pty.openpty()
child_pid = os.fork()

if child_pid == 0:
    try:
        os.setsid()
        fcntl.ioctl(slave_fd, termios.TIOCSCTTY, 0)
        os.dup2(slave_fd, 0)
        os.dup2(slave_fd, 1)
        os.dup2(slave_fd, 2)
        os.close(master_fd)
        os.close(slave_fd)
        os.execvp(sys.argv[1], sys.argv[1:])
    except Exception as err:
        print(f"pty-launcher child failed: {err}", file=sys.stderr)
        os._exit(127)

os.close(slave_fd)
tty_closed = False


def close_user_tty(_signum=None, _frame=None):
    global tty_closed
    if not tty_closed:
        try:
            os.close(master_fd)
        except OSError:
            pass
        tty_closed = True


def terminate(_signum, _frame):
    close_user_tty()
    try:
        os.kill(child_pid, signal.SIGTERM)
    except ProcessLookupError:
        pass


signal.signal(signal.SIGUSR1, close_user_tty)
signal.signal(signal.SIGTERM, terminate)
signal.signal(signal.SIGINT, terminate)

while True:
    pid, status = os.waitpid(child_pid, os.WNOHANG)
    if pid == child_pid:
        if os.WIFEXITED(status):
            sys.exit(os.WEXITSTATUS(status))
        if os.WIFSIGNALED(status):
            os.kill(os.getpid(), os.WTERMSIG(status))
        sys.exit(1)
    time.sleep(0.05)
