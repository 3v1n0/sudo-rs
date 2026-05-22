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
const POLL_READY_SLEEP: &str = "0.05";
const POLL_EARLY_SLEEP: &str = "0.02";
const SUDO_PID_LIST: &str = "$(pidof sudo 2>/dev/null | tr ' ' '\\n' | sort -n | tr '\\n' ' ')";
const SUDO_DEV_LOGS_TAIL: &str = "ls -1 /tmp/sudo-dev-*.log 2>/dev/null | xargs -r -n1 sh -c 'echo \"=== $1 ===\"; tail -n 200 \"$1\"' sh || true;";

fn wait_for_file(
    file: &str,
    sleep: &str,
    on_timeout_message: &str,
    prefix: &str,
    include_ps: bool,
) -> String {
    let ps_dump = if include_ps {
        "ps -o pid,ppid,pgid,sid,tty,args -C sudo || true;"
    } else {
        ""
    };
    format!(
        "for _ in $(seq 1 {POLL_ATTEMPTS}); do
             [ -f {file} ] && break;
             sleep {sleep};
         done;
         [ -f {file} ] || {{
             echo '{on_timeout_message}';
             {ps_dump}
             cat /tmp/{prefix}.log;
             {sudo_dev_logs}
             exit 1;
         }};",
        sudo_dev_logs = SUDO_DEV_LOGS_TAIL
    )
}

fn wait_for_sudo_exit(before_sudo: &str, leak_message: &str, prefix: &str) -> String {
    format!(
        "for _ in $(seq 1 {POLL_ATTEMPTS}); do
             current_sudo=\"{sudo_list}\";
             [ \"$current_sudo\" = \"{before_sudo}\" ] && exit 0;
             sleep {POLL_READY_SLEEP};
         done;
         echo '{leak_message}';
         ps -o pid,ppid,pgid,sid,tty,args -C sudo || true;
         cat /tmp/{prefix}.log;
         {sudo_dev_logs}
         exit 1",
        sudo_list = SUDO_PID_LIST,
        sudo_dev_logs = SUDO_DEV_LOGS_TAIL
    )
}

fn assert_closed_tty_sends_hup(
    env: &sudo_test::Env,
    launcher_script: &str,
    target_script: &str,
    ready_file: &str,
    hup_file: &str,
    prefix: &str,
) {
    let wait_until_ready = wait_for_file(
        ready_file,
        POLL_READY_SLEEP,
        "target not ready",
        prefix,
        false,
    );
    let wait_for_hup = wait_for_file(
        hup_file,
        POLL_READY_SLEEP,
        "command did not receive SIGHUP",
        prefix,
        false,
    );
    let wait_for_sudo_exit = wait_for_sudo_exit(
        "$before_sudo",
        "sudo process leaked after tty close",
        prefix,
    );
    let command = format!(
        "rm -f {ready_file} {hup_file} /tmp/{prefix}.pid /tmp/{prefix}.log;
         before_sudo=\"{sudo_list}\";
         sh {launcher_script} sudo /bin/sh {target_script} >/tmp/{prefix}.log 2>&1 &
         echo $! >/tmp/{prefix}.pid;
         {wait_until_ready}
         kill -USR1 \"$(cat /tmp/{prefix}.pid)\" || true;
         {wait_for_hup}
         {wait_for_sudo_exit}",
        sudo_list = SUDO_PID_LIST
    );

    Command::new("sh")
        .args(["-c", &command])
        .tty(true)
        .output(env)
        .assert_success();
}

fn assert_early_closed_tty_sends_hup(
    env: &sudo_test::Env,
    launcher_script: &str,
    target_script: &str,
    started_file: &str,
    hup_file: &str,
    prefix: &str,
) {
    let wait_until_started = wait_for_file(
        started_file,
        POLL_EARLY_SLEEP,
        "target did not start",
        prefix,
        false,
    );
    let wait_for_hup = wait_for_file(
        hup_file,
        POLL_READY_SLEEP,
        "command did not receive SIGHUP after early tty close",
        prefix,
        false,
    );
    let wait_for_sudo_exit = wait_for_sudo_exit(
        "$before_sudo",
        "sudo process leaked after early tty close",
        prefix,
    );
    let command = format!(
        "rm -f {started_file} {hup_file} /tmp/{prefix}.pid /tmp/{prefix}.log;
         before_sudo=\"{sudo_list}\";
         sh {launcher_script} sudo /bin/sh {target_script} >/tmp/{prefix}.log 2>&1 &
         echo $! >/tmp/{prefix}.pid;
         {wait_until_started}
         kill -USR1 \"$(cat /tmp/{prefix}.pid)\" || true;
         {wait_for_hup}
         {wait_for_sudo_exit}",
        sudo_list = SUDO_PID_LIST
    );

    Command::new("sh")
        .args(["-c", &command])
        .tty(true)
        .output(env)
        .assert_success();
}

fn assert_closed_tty_closes_sudo(
    env: &sudo_test::Env,
    launcher_script: &str,
    target_script: &str,
    ready_file: &str,
    prefix: &str,
) {
    let wait_until_ready = wait_for_file(
        ready_file,
        POLL_READY_SLEEP,
        "target not ready",
        prefix,
        false,
    );
    let wait_for_sudo_exit = wait_for_sudo_exit(
        "$before_sudo",
        "sudo process leaked after tty close",
        prefix,
    );
    let command = format!(
        "rm -f {ready_file} /tmp/{prefix}.pid /tmp/{prefix}.log;
         before_sudo=\"{sudo_list}\";
         sh {launcher_script} sudo /bin/sh {target_script} >/tmp/{prefix}.log 2>&1 &
         echo $! >/tmp/{prefix}.pid;
         {wait_until_ready}
         kill -USR1 \"$(cat /tmp/{prefix}.pid)\" || true;
         {wait_for_sudo_exit}",
        sudo_list = SUDO_PID_LIST
    );

    Command::new("sh")
        .args(["-c", &command])
        .tty(true)
        .output(env)
        .assert_success();
}

fn assert_immediate_tty_close_after_sudo_spawn_cleans_sudo(
    env: &sudo_test::Env,
    launcher_script: &str,
    target_script: &str,
    prefix: &str,
) {
    let wait_for_sudo_spawn = format!(
        "for _ in $(seq 1 {POLL_ATTEMPTS}); do
             current_sudo=\"{sudo_list}\";
             [ \"$current_sudo\" != \"$before_sudo\" ] && break;
             sleep {POLL_EARLY_SLEEP};
         done;",
        sudo_list = SUDO_PID_LIST
    );
    let wait_for_sudo_exit = wait_for_sudo_exit(
        "$before_sudo",
        "sudo process leaked after immediate tty close",
        prefix,
    );
    let command = format!(
        "rm -f /tmp/{prefix}.pid /tmp/{prefix}.log;
         before_sudo=\"{sudo_list}\";
         sh {launcher_script} sudo /bin/sh {target_script} >/tmp/{prefix}.log 2>&1 &
         echo $! >/tmp/{prefix}.pid;
         {wait_for_sudo_spawn}
         kill -USR1 \"$(cat /tmp/{prefix}.pid)\" || true;
         {wait_for_sudo_exit}",
        sudo_list = SUDO_PID_LIST
    );

    Command::new("sh")
        .args(["-c", &command])
        .tty(true)
        .output(env)
        .assert_success();
}

#[test]
fn closed_user_tty_sends_hup_to_command() {
    if sudo_test::sudo_version() < sudo_test::ogsudo("1.9.18") {
        return;
    }

    let launcher_script = "/root/pty-launcher.sh";
    let target_script = "/root/closed-tty-target.sh";
    let ready_file = "/tmp/closed-tty-ready";
    let hup_file = "/tmp/closed-tty-hup";

    let env = Env([SUDOERS_ALL_ALL_NOPASSWD, "Defaults use_pty"])
        .file(launcher_script, include_str!("use_pty/pty-launcher.sh"))
        .file(target_script, include_str!("use_pty/closed-tty-target.sh"))
        .build();

    assert_closed_tty_sends_hup(
        &env,
        launcher_script,
        target_script,
        ready_file,
        hup_file,
        "closed-tty",
    );
}

#[test]
fn closed_user_tty_sends_hup_with_stdin_pipe() {
    let launcher_script = "/root/pty-launcher.sh";
    let target_script = "/root/closed-tty-target-pipe.sh";
    let ready_file = "/tmp/closed-tty-pipe-ready";

    let env = Env([SUDOERS_ALL_ALL_NOPASSWD, "Defaults use_pty"])
        .file(launcher_script, include_str!("use_pty/pty-launcher.sh"))
        .file(
            target_script,
            include_str!("use_pty/closed-tty-target-pipe.sh"),
        )
        .build();

    assert_closed_tty_closes_sudo(
        &env,
        launcher_script,
        target_script,
        ready_file,
        "closed-tty-pipe",
    );
}

#[test]
fn closed_user_tty_before_ready_still_sends_hup() {
    let launcher_script = "/root/pty-launcher.sh";
    let target_script = "/root/closed-tty-target-early.sh";
    let started_file = "/tmp/closed-tty-early-started";
    let hup_file = "/tmp/closed-tty-early-hup";

    let env = Env([SUDOERS_ALL_ALL_NOPASSWD, "Defaults use_pty"])
        .file(launcher_script, include_str!("use_pty/pty-launcher.sh"))
        .file(
            target_script,
            include_str!("use_pty/closed-tty-target-early.sh"),
        )
        .build();

    assert_early_closed_tty_sends_hup(
        &env,
        launcher_script,
        target_script,
        started_file,
        hup_file,
        "closed-tty-early",
    );
}

#[test]
fn closed_user_tty_right_after_sudo_spawn_sends_hup() {
    let launcher_script = "/root/pty-launcher.sh";
    let adapted_target = "/root/closed-tty-target-immediate.sh";

    let env = Env([SUDOERS_ALL_ALL_NOPASSWD, "Defaults use_pty"])
        .file(launcher_script, include_str!("use_pty/pty-launcher.sh"))
        .file(
            adapted_target,
            "trap 'touch /tmp/closed-tty-immediate-hup; exit 0' HUP; while :; do sleep 0.1; done",
        )
        .build();

    assert_immediate_tty_close_after_sudo_spawn_cleans_sudo(
        &env,
        launcher_script,
        adapted_target,
        "closed-tty-immediate",
    );
}
