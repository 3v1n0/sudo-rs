use sudo_test::{
    Command, Env,
    helpers::{self, PRINT_PTY_OWNER, PsAuxEntry},
};

use crate::{OTHER_USERNAME, SUDOERS_ALL_ALL_NOPASSWD, USERNAME};

#[derive(Debug)]
struct Processes {
    original: PsAuxEntry,
    monitor: PsAuxEntry,
    command: PsAuxEntry,
}

fn fixture() -> Processes {
    let env = Env([SUDOERS_ALL_ALL_NOPASSWD, "Defaults use_pty"]).build();

    let child = Command::new("sudo")
        .args(["sh", "-c", "touch /tmp/barrier; sleep 3; true"])
        .tty(true)
        .spawn(&env);

    let ps_aux = Command::new("sh")
        .args([
            "-c",
            "until [ -f /tmp/barrier ]; do sleep 0.1; done; ps aux",
        ])
        .output(&env)
        .stdout();

    child.wait().assert_success();

    let entries = helpers::parse_ps_aux(&ps_aux);

    let mut sudo_related_processes = entries
        .into_iter()
        .filter(|entry| entry.command.contains("sh -c touch"))
        .collect::<Vec<_>>();

    sudo_related_processes.sort_by_key(|entry| entry.pid);

    let [original, monitor, command]: [PsAuxEntry; 3] = sudo_related_processes
        .try_into()
        .expect("expected 3 sudo-related processes");

    // sanity check
    let prefix = "sudo ";
    assert!(original.command.starts_with(prefix));
    assert!(monitor.command.starts_with(prefix));
    assert!(!command.command.starts_with(prefix));

    assert!(original.has_tty());
    assert!(monitor.has_tty());
    assert!(command.has_tty());

    Processes {
        original,
        monitor,
        command,
    }
}

#[test]
fn spawns_three_processes() {
    let _ = fixture();
}

#[test]
fn allocates_a_second_pty_which_is_assigned_to_the_command_process() {
    let Processes {
        original,
        monitor,
        command,
    } = fixture();

    assert_eq!(monitor.tty, command.tty);
    assert_ne!(original.tty, monitor.tty);
}

#[test]
fn process_state() {
    let Processes {
        original,
        monitor,
        command,
    } = fixture();

    assert!(original.is_in_the_foreground_process_group());
    assert!(command.is_in_the_foreground_process_group());

    assert!(original.is_session_leader());
    assert!(monitor.is_session_leader());
}

#[test]
fn terminal_is_restored() {
    let env = Env([SUDOERS_ALL_ALL_NOPASSWD, "Defaults use_pty"]).build();
    // Run `stty` before and after running sudo to check that the terminal configuration is
    // restored before sudo exits.
    let stdout = Command::new("sh")
        .args(["-c", "stty; sudo echo 'hello'; stty"])
        .tty(true)
        .output(&env)
        .stdout();

    assert_contains!(stdout, "hello");
    let (before, after) = stdout.split_once("hello").unwrap();
    assert_eq!(before.trim(), after.trim());
}

#[test]
fn pty_owner() {
    let env = Env([SUDOERS_ALL_ALL_NOPASSWD, "Defaults use_pty"])
        .user(USERNAME)
        .user(OTHER_USERNAME)
        .build();

    let stdout = Command::new("sudo")
        .as_user(USERNAME)
        .args(["-u", OTHER_USERNAME, "sh", "-c", PRINT_PTY_OWNER])
        .tty(true)
        .output(&env)
        .stdout();

    assert_eq!(stdout.trim(), format!("{OTHER_USERNAME} tty"));
}

#[test]
fn stdin_pipe() {
    let env = Env([SUDOERS_ALL_ALL_NOPASSWD, "Defaults use_pty"]).build();

    let stdout = Command::new("sh")
        .args(["-c", "echo 'hello world' | sudo grep -o hello"])
        .tty(true)
        .output(&env)
        .stdout();

    assert_eq!(stdout.trim(), "hello");
}

#[test]
fn stdout_pipe() {
    let env = Env([SUDOERS_ALL_ALL_NOPASSWD, "Defaults use_pty"]).build();

    let stdout = Command::new("sh")
        .args(["-c", "sudo echo 'hello world' | grep -o hello"])
        .tty(true)
        .output(&env)
        .stdout();

    assert_eq!(stdout.trim(), "hello");
}

#[test]
fn stderr_pipe() {
    let env = Env([SUDOERS_ALL_ALL_NOPASSWD, "Defaults use_pty"]).build();

    let output = Command::new("sh")
        .args([
            "-c",
            "2>/tmp/stderr.txt sudo sh -c '>&2 echo \"hello world\"'",
        ])
        .tty(true)
        .output(&env);

    assert!(output.stderr().is_empty());

    let stdout = Command::new("cat")
        .arg("/tmp/stderr.txt")
        .output(&env)
        .stdout();

    assert_eq!(stdout, "hello world");
}

#[test]
fn stdout_foreign_pty() {
    let env = Env([SUDOERS_ALL_ALL_NOPASSWD, "Defaults use_pty"]).build();

    // Everything is put in a single command with separators to keep the pts numbers predictable
    let output = Command::new("sh")
        .args([
            "-c",
            "socat - SYSTEM:'ls -l /proc/self/fd; echo @@@@; sudo ls -l /proc/self/fd',pty,setsid,ctty;
            echo ====;
            socat - SYSTEM:'ls -l /proc/self/fd; echo @@@@; sudo ls -l /proc/self/fd',pty",
        ])
        .tty(true)
        .output(&env);

    let stdout = output.stdout();
    let (own_term, foreign_term) = stdout.split_once("====").unwrap();

    let (own_term_in, own_term_sudo) = own_term.split_once("@@@@").unwrap();
    assert_contains!(own_term_in, " 0 -> /dev/pts/1");
    assert_contains!(own_term_in, " 1 -> /dev/pts/1");
    assert_contains!(own_term_in, " 2 -> /dev/pts/0");
    // pts/1 is our controlling tty, so it gets proxied.
    // pts/0 is a foreign pty, so it gets inherited
    assert_contains!(own_term_sudo, " 0 -> /dev/pts/2");
    assert_contains!(own_term_sudo, " 1 -> /dev/pts/2");
    assert_contains!(own_term_sudo, " 2 -> /dev/pts/0");

    let (foreign_term_in, foreign_term_sudo) = foreign_term.split_once("@@@@").unwrap();
    assert_contains!(foreign_term_in, " 0 -> /dev/pts/1");
    assert_contains!(foreign_term_in, " 1 -> /dev/pts/1");
    assert_contains!(foreign_term_in, " 2 -> /dev/pts/0");
    // pts/1 is not our controlling tty, so it gets inherited.
    // pts/0 is our controlling tty, so it gets proxied
    assert_contains!(foreign_term_sudo, " 0 -> /dev/pts/1");
    assert_contains!(foreign_term_sudo, " 1 -> /dev/pts/1");
    assert_contains!(foreign_term_sudo, " 2 -> /dev/pts/2");
}

#[test]
fn stdout_pipe_tty() {
    let env = Env([SUDOERS_ALL_ALL_NOPASSWD, "Defaults use_pty"]).build();

    let output = Command::new("sh")
        .args([
            "-c",
            "echo -n 'hello world' | socat -d0 STDIO SYSTEM:'sudo cat /dev/tty | cat',pty",
        ])
        .tty(true)
        .output(&env);

    assert_eq!(output.stdout(), "hello world");
}

const POLL_ATTEMPTS: u32 = 200;
const POLL_SLEEP: &str = "0.05";

fn wait_for(file: &str, message: &str) -> String {
    format!(
        "for _ in $(seq 1 {POLL_ATTEMPTS}); do
             [ -f {file} ] && break;
             sleep {POLL_SLEEP};
         done;
         [ -f {file} ] || {{ echo '{message}'; exit 1; }};"
    )
}

fn wait_for_sudo_exit() -> String {
    format!(
        "for _ in $(seq 1 {POLL_ATTEMPTS}); do
             pidof sudo >/dev/null 2>&1 || exit 0;
             sleep {POLL_SLEEP};
         done;
         echo 'sudo process leaked after tty close'; exit 1"
    )
}

/// Run a command inside a PTY launcher, wait for readiness, close the PTY via
/// SIGUSR1, then wait for an expected file to appear (e.g. a HUP marker or
/// EIO-detection marker) and for sudo to exit.
fn assert_closed_tty(
    env: &sudo_test::Env,
    target_script: &str,
    ready_file: &str,
    expected_file: &str,
) {
    let launcher = "/root/pty-launcher.sh";

    let command = format!(
        "rm -f {ready_file} {expected_file};
         sh {launcher} sudo /bin/sh {target_script} &
         {wait_ready}
         kill -USR1 $! || true;
         {wait_expected}
         {wait_sudo_exit}",
        wait_ready = wait_for(ready_file, "target not ready"),
        wait_expected = wait_for(expected_file, "expected event did not occur"),
        wait_sudo_exit = wait_for_sudo_exit(),
    );

    Command::new("sh")
        .args(["-c", &command])
        .tty(true)
        .output(env)
        .assert_success();
}

#[test]
fn closed_user_tty_sends_hup_to_command() {
    if sudo_test::is_original_sudo() && sudo_test::sudo_version() < sudo_test::ogsudo("1.9.18") {
        return;
    }

    let target = "/root/closed-tty-target.sh";
    let ready = "/tmp/closed-tty-ready";
    let hup = "/tmp/closed-tty-hup";

    let env = Env([SUDOERS_ALL_ALL_NOPASSWD, "Defaults use_pty"])
        .file(
            "/root/pty-launcher.sh",
            include_str!("use_pty/pty-launcher.sh"),
        )
        .file(target, include_str!("use_pty/closed-tty-target.sh"))
        .build();

    assert_closed_tty(&env, &format!("{target} {ready} {hup}"), ready, hup);
}

#[test]
fn closed_user_tty_sends_hup_with_stdin_pipe() {
    let target = "/root/closed-tty-target.sh";
    let ready = "/tmp/closed-tty-pipe-ready";
    let hup = "/tmp/closed-tty-pipe-hup";

    let env = Env([SUDOERS_ALL_ALL_NOPASSWD, "Defaults use_pty"])
        .file(
            "/root/pty-launcher.sh",
            include_str!("use_pty/pty-launcher.sh"),
        )
        .file(target, include_str!("use_pty/closed-tty-target.sh"))
        .build();

    assert_closed_tty(&env, &format!("{target} {ready} {hup} pipe"), ready, hup);
}

#[test]
fn closed_user_tty_before_ready_still_sends_hup() {
    let target = "/root/closed-tty-target.sh";
    let started = "/tmp/closed-tty-early-started";
    let hup = "/tmp/closed-tty-early-hup";

    let env = Env([SUDOERS_ALL_ALL_NOPASSWD, "Defaults use_pty"])
        .file(
            "/root/pty-launcher.sh",
            include_str!("use_pty/pty-launcher.sh"),
        )
        .file(target, include_str!("use_pty/closed-tty-target.sh"))
        .build();

    assert_closed_tty(
        &env,
        &format!("{target} {started} {hup} early"),
        started,
        hup,
    );
}

#[test]
fn closed_user_tty_right_after_sudo_spawn_sends_hup() {
    let target = "/root/closed-tty-target-immediate.sh";
    let hup = "/tmp/closed-tty-immediate-hup";

    let env = Env([SUDOERS_ALL_ALL_NOPASSWD, "Defaults use_pty"])
        .file(
            "/root/pty-launcher.sh",
            include_str!("use_pty/pty-launcher.sh"),
        )
        .file(
            target,
            "trap 'touch /tmp/closed-tty-immediate-hup; exit 0' HUP; while :; do sleep 0.1; done",
        )
        .build();

    // For the immediate case, we don't have a ready file - we close the tty
    // as soon as sudo spawns. Use a custom command that waits for sudo to
    // appear then sends SIGUSR1.
    let command = format!(
        "rm -f {hup};
         sh /root/pty-launcher.sh sudo /bin/sh {target} &
         # Wait for sudo to spawn
         for _ in $(seq 1 {POLL_ATTEMPTS}); do
             pidof sudo >/dev/null 2>&1 && break;
             sleep 0.02;
         done;
         kill -USR1 $! || true;
         {wait_hup}
         {wait_sudo_exit}",
        wait_hup = wait_for(
            hup,
            "command did not receive SIGHUP after immediate tty close"
        ),
        wait_sudo_exit = wait_for_sudo_exit(),
    );

    Command::new("sh")
        .args(["-c", &command])
        .tty(true)
        .output(&env)
        .assert_success();
}

/// When the user's tty is closed, sudo must close (revoke) its own pty leader so
/// that the command's terminal becomes dead (EIO). This test verifies that by
/// running a command that **ignores SIGHUP** and polls its terminal fd for EIO.
///
/// Without the pty-leader revoke fix the command's terminal stays alive
/// indefinitely and the test times out — this is the key discriminator.
#[test]
fn closed_user_tty_revokes_pty_leader() {
    let target = "/root/closed-tty-revoke-target.sh";
    let ready = "/tmp/pty-revoke-ready";
    let detected = "/tmp/pty-revoke-detected";

    let env = Env([SUDOERS_ALL_ALL_NOPASSWD, "Defaults use_pty"])
        .file(
            "/root/pty-launcher.sh",
            include_str!("use_pty/pty-launcher.sh"),
        )
        .file(target, include_str!("use_pty/closed-tty-revoke-target.sh"))
        .build();

    assert_closed_tty(
        &env,
        &format!("{target} {ready} {detected}"),
        ready,
        detected,
    );
}
