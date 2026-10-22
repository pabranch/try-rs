use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

use chrono::Local;
use tempdir::TempDir;

#[test]
fn shows_help() {
    let p = spawn_try(None, &["--help"], &[]);

    assert!(p.stdout.contains("Usage: try-rs"));
}

#[test]
fn shows_version() {
    let p = spawn_try(None, &["--version"], &[]);

    assert!(
        p.stdout.contains("try-rs"),
        "version output should go to stdout"
    );
    assert!(
        !p.stderr.contains("try-rs 0."),
        "version should not appear on stderr"
    );
}

#[test]
fn invalid_shell_flag() {
    let p = spawn_try(None, &["--shell", "invalid_shell"], &[]);

    assert!(
        !p.status.success(),
        "invalid shell should exit with non-zero"
    );
}

#[test]
fn new_name() {
    // given
    let h = Harness::new(false);

    // when
    let p = h.run_try(&["foo"]);

    // then
    let expected_dir = h.tries_path().join("foo");
    assert_eq!(
        format!("cd '{}'", expected_dir.display()),
        p.stdout.trim(),
        "printed command is correct"
    );
    assert!(expected_dir.exists(), "try directory created");
    assert!(expected_dir.is_dir(), "try dir is a directory");
}

#[test]
fn exact_folder_exists() {
    // given
    let h = Harness::new(false);
    let existing = "existing";
    h.create_try_folder(existing);

    // when
    let p = h.run_try(&[existing]);

    // then
    let expected_dir = h.tries_path().join(existing);
    assert_eq!(
        format!("cd '{}'", expected_dir.display()),
        p.stdout.trim(),
        "printed command is correct"
    );
}

#[test]
fn folder_with_date_exists() {
    // given
    let h = Harness::new(false);
    let name = "2020-02-02 existing";
    h.create_try_folder(name);

    // when
    let p = h.run_try(&["existing"]);

    // then
    let expected_dir = h.tries_path().join(name);
    assert_eq!(
        format!("cd '{}'", expected_dir.display()),
        p.stdout.trim(),
        "printed command is correct"
    );
}

#[test]
fn new_name_with_date() {
    // given
    let h = Harness::new(true);
    let name = "new-name";

    // when
    let p = h.run_try(&["new-name"]);

    // then
    let expected_dir = h
        .tries_path()
        .join(format!("{} {}", Local::now().format("%Y-%m-%d"), name));
    assert_eq!(
        format!("cd '{}'", expected_dir.display()),
        p.stdout.trim(),
        "printed command is correct"
    );
}

#[test]
fn checkout_git_with_destination() {
    // given
    let h = Harness::new(false);

    let git_dir = create_git_origin(&h).expect("could not setup git origin");

    // when
    let p = h.run_try(&[&git_dir.display().to_string(), "foo"]);

    // then
    assert!(p.status.success());
    let expected_dir = h.tries_path().join("foo");
    assert_eq!(
        format!("cd '{}'", &expected_dir.display()),
        p.stdout.trim(),
        "printed command is correct"
    );
    assert!(expected_dir.exists(), "try directory created");
    assert!(expected_dir.is_dir(), "try dir is a directory");
    assert!(
        command(&expected_dir, "git", &["status"])
            .unwrap()
            .status
            .success(),
        "new directory should be a git repo"
    );
}

#[test]
fn checkout_git_without_destination() {
    // given
    let h = Harness::new(false);

    let git_dir = create_git_origin(&h).expect("could not setup git origin");

    // when
    let p = h.run_try(&[&git_dir.display().to_string()]);

    // then
    assert!(p.status.success());
    let expected_dir = h.tries_path().join(EXISTING_GIT_REPO_NAME);
    assert_eq!(
        format!("cd '{}'", expected_dir.display()),
        p.stdout.trim(),
        "printed command is correct"
    );
    assert!(expected_dir.exists(), "try directory created");
    assert!(expected_dir.is_dir(), "try dir is a directory");
    assert!(
        command(&expected_dir, "git", &["status"])
            .unwrap()
            .status
            .success(),
        "new directory should be a git repo"
    );
}

#[test]
fn new_worktree() {
    // given
    let h = Harness::new(false);
    let git_dir = create_git_origin(&h).expect("could not setup git origin");

    // when
    let mut cmd = built_try(Some(h.dir.path()), &git_dir);
    cmd.args(["--worktree", "my-brand-new-feature"]);
    let output = run(&mut cmd);

    // then
    let expected_dir = h.tries_path().join("my-brand-new-feature");
    assert!(output.status.success());
    assert!(
        output
            .stdout
            .contains(&format!("cd '{}'", expected_dir.display()))
    );
    assert!(expected_dir.exists());
    assert!(expected_dir.is_dir());
    assert!(
        output
            .stderr
            .starts_with("Creating worktree 'my-brand-new-feature'")
    );
    let git_status = command(&expected_dir, "git", &["worktree", "list"]).unwrap();
    assert!(
        git_status.status.success(),
        "new directory should be a git repo"
    );
    let canonical_expected_dir = expected_dir.canonicalize().unwrap_or(expected_dir);
    assert!(git_status.stdout.lines().any(|line| {
        line.starts_with(&format!("{}", canonical_expected_dir.display()))
            && line.ends_with("[my-brand-new-feature]")
    }));
}

const EXISTING_GIT_REPO_NAME: &str = "git-origin";

fn create_git_origin(h: &Harness) -> Result<PathBuf, String> {
    let dir = h.dir.path().join(format!("{}.git", EXISTING_GIT_REPO_NAME));
    fs::DirBuilder::new().create(&dir).unwrap();

    let content_file = dir.join("content.txt");
    let content = "this is some file content";
    fs::write(content_file, content).expect("could not create content file");

    command(&dir, "git", &["init"])?;
    command(&dir, "git", &["config", "user.email", "test@test.internal"])?;
    command(&dir, "git", &["config", "user.name", "Test"])?;
    command(&dir, "git", &["add", "."])?;
    command(&dir, "git", &["commit", "-m", "init"])?;

    Ok(dir)
}

fn command(dir: &PathBuf, cmd: &str, args: &[&str]) -> Result<Output, String> {
    let result = Command::new(cmd)
        .current_dir(dir)
        .args(args)
        .output()
        .map(|output| Output {
            status: output.status,
            stderr: String::from_utf8(output.stderr).expect("couldn't read stderr to string"),
            stdout: String::from_utf8(output.stdout).expect("couldn't read stdout to string"),
        });
    let result = match result {
        Ok(ok) => {
            if ok.status.success() {
                Ok(ok)
            } else {
                {
                    #[cfg(unix)]
                    let signal = ok.status.signal();
                    #[cfg(not(unix))]
                    let signal: Option<i32> = None;
                    Err(format!(
                        "command not successful (exit code {:?}, signal {:?}):\nstdout::{}\nstderr:\n{}",
                        ok.status.code(),
                        signal,
                        ok.stdout,
                        ok.stderr,
                    ))
                }
            }
        }
        Err(err) => Err(err.to_string()),
    };
    result.map_err(|err| {
        format!(
            "failed to run '{} {}' in '{}': {}",
            cmd,
            args.join(" "),
            dir.display(),
            err
        )
    })
}

#[derive(Debug)]
struct Output {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

/// Run a prepared command, capturing its output as [`Output`].
fn run(cmd: &mut Command) -> Output {
    let output = cmd.output().expect("failed to spawn process");
    Output {
        status: output.status,
        stdout: String::from_utf8(output.stdout).expect("couldn't read stdout to string"),
        stderr: String::from_utf8(output.stderr).expect("couldn't read stderr to string"),
    }
}

/// Apply the environment every `try-rs` spawn needs.
///
/// `SHELL` is cleared and `TRY_PATH` removed so the child can never reach the
/// real user's shell integration or tries directory, and `TRY_CONFIG_DIR` is
/// pinned to `config_dir` when one is given. Tests that do not care about
/// configuration (help, version, CLI parse errors) pass `None`.
fn sanitize(cmd: &mut Command, config_dir: Option<&Path>) {
    cmd.env("SHELL", "").env_remove("TRY_PATH");
    if let Some(dir) = config_dir {
        cmd.env("TRY_CONFIG_DIR", dir);
    }
}

/// A sanitized `cargo run -- ...` invocation of `try-rs`.
///
/// Arguments are appended by the caller so each test reads as its own command
/// line.
fn cargo_try(config_dir: Option<&Path>) -> Command {
    let mut cmd = Command::new("cargo");
    cmd.arg("run").arg("--");
    sanitize(&mut cmd, config_dir);
    cmd
}

/// A sanitized invocation of the already-built `try-rs` binary, run in `cwd`.
///
/// Needed where the working directory matters to `try-rs` itself (git
/// fixtures); `cargo run` cannot be used there because cargo would look for a
/// manifest in that directory.
fn built_try(config_dir: Option<&Path>, cwd: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_try-rs"));
    cmd.current_dir(cwd);
    sanitize(&mut cmd, config_dir);
    cmd
}

/// Spawn `try-rs` via `cargo run` with `args`.
///
/// `extra_env` is applied last and may override the sanitized defaults (e.g. to
/// set `HOME`).
fn spawn_try(config_dir: Option<&Path>, args: &[&str], extra_env: &[(&str, &str)]) -> Output {
    let mut cmd = cargo_try(config_dir);
    cmd.args(args);
    for (key, value) in extra_env {
        cmd.env(key, value);
    }
    run(&mut cmd)
}

struct Harness {
    dir: TempDir,
}

impl Harness {
    fn new(with_date_prefix: bool) -> Self {
        let dir = TempDir::new("try-test").expect("couldn't generate temp directory for test");

        let mut config = String::new();
        config.push_str(&format!(
            "tries_path = \"{}\"\n",
            dir.path().join("tries").display()
        ));
        if with_date_prefix {
            config.push_str("apply_date_prefix = true\n");
        }
        fs::write(dir.path().join("config.toml"), config).expect("could not create config file");

        Harness { dir }
    }

    fn run_try(&self, args: &[&str]) -> Output {
        spawn_try(Some(self.dir.path()), args, &[])
    }

    fn tries_path(&self) -> PathBuf {
        self.dir.path().join("tries").to_path_buf()
    }

    fn run_try_with_env(&self, args: &[&str], env_key: &str, env_val: &str) -> Output {
        spawn_try(Some(self.dir.path()), args, &[(env_key, env_val)])
    }

    fn create_try_folder(&self, name: &str) {
        fs::DirBuilder::new()
            .recursive(true)
            .create(self.tries_path().join(name))
            .expect("couldn't create 'existing' try");
    }
}

// ── Additional integration tests ───────────────────────────────────

#[test]
fn new_folder_is_created_on_disk() {
    let h = Harness::new(false);
    let p = h.run_try(&["brand-new"]);
    assert!(p.status.success());
    assert!(h.tries_path().join("brand-new").exists());
    assert!(h.tries_path().join("brand-new").is_dir());
}

#[test]
fn new_name_with_date_creates_dated_folder() {
    let h = Harness::new(true);
    let today = Local::now().format("%Y-%m-%d").to_string();
    let _ = h.run_try(&["dated-test"]);
    let expected = h.tries_path().join(format!("{} dated-test", today));
    assert!(expected.exists(), "dated folder should be created");
}

#[test]
fn multiple_new_folders() {
    let h = Harness::new(false);
    h.run_try(&["proj-a"]);
    h.run_try(&["proj-b"]);
    h.run_try(&["proj-c"]);
    assert!(h.tries_path().join("proj-a").exists());
    assert!(h.tries_path().join("proj-b").exists());
    assert!(h.tries_path().join("proj-c").exists());
}

#[test]
fn help_contains_expected_flags() {
    let output = spawn_try(None, &["--help"], &[]).stdout;
    assert!(output.contains("--setup"), "should document --setup flag");
    assert!(
        output.contains("--setup-stdout"),
        "should document --setup-stdout flag"
    );
    assert!(
        output.contains("--full-clone") || output.contains("-f"),
        "should document full clone"
    );
    assert!(
        output.contains("--worktree") || output.contains("-w"),
        "should document worktree flag"
    );
    assert!(
        output.contains("--inline-picker"),
        "should document inline picker flag"
    );
}

#[test]
fn inline_height_requires_inline_picker() {
    let p = spawn_try(None, &["--inline-height", "22"], &[]);

    assert!(
        !p.status.success(),
        "--inline-height should require --inline-picker"
    );

    let output = format!("{}\n{}", p.stdout, p.stderr);
    assert!(
        output.contains("--inline-picker"),
        "error should mention missing --inline-picker"
    );
}

#[test]
fn existing_folder_cd_does_not_recreate() {
    let h = Harness::new(false);
    h.create_try_folder("already");

    // Put a marker file inside
    fs::write(h.tries_path().join("already").join("marker.txt"), "test").unwrap();

    let p = h.run_try(&["already"]);

    assert!(p.stdout.contains("cd"));
    // Marker file should still be there (folder wasn't recreated)
    assert!(h.tries_path().join("already").join("marker.txt").exists());
}

#[test]
fn config_with_editor_field() {
    let dir = TempDir::new("try-editor-test").unwrap();
    let config = format!(
        "tries_path = \"{}\"\neditor = \"vim\"\n",
        dir.path().join("tries").display()
    );
    fs::write(dir.path().join("config.toml"), config).unwrap();

    let p = spawn_try(Some(dir.path()), &["editor-proj"], &[]);

    // It should create the folder successfully
    assert!(p.status.success() || !p.stdout.is_empty());
}

#[test]
fn config_with_theme() {
    let dir = TempDir::new("try-theme-test").unwrap();
    let config = format!(
        "tries_path = \"{}\"\ntheme = \"Dracula\"\n",
        dir.path().join("tries").display()
    );
    fs::write(dir.path().join("config.toml"), config).unwrap();

    let p = spawn_try(Some(dir.path()), &["themed-proj"], &[]);

    assert!(p.stdout.contains("cd"), "should produce cd command");
}

#[test]
fn config_with_invalid_theme_falls_back() {
    let dir = TempDir::new("try-bad-theme").unwrap();
    let config = format!(
        "tries_path = \"{}\"\ntheme = \"NonExistentTheme\"\n",
        dir.path().join("tries").display()
    );
    fs::write(dir.path().join("config.toml"), config).unwrap();

    let p = spawn_try(Some(dir.path()), &["fallback-proj"], &[]);

    // Should still work, just using default theme
    assert!(p.stdout.contains("cd"), "should still produce cd command");
}

#[test]
fn non_interactive_run_does_not_modify_rc_files() {
    // A non-interactive run (stdin is not a TTY, as with Command::output())
    // must never append a `source` line to the user's rc files: the optional
    // shell-integration prompt should be skipped instead of defaulting to yes.
    //
    // `HOME` only redirects the rc file lookup on unix; on Windows the profile
    // path comes from the Win32 known-folder API and `SHELL` is ignored in
    // favour of PowerShell, so it cannot be sandboxed from here (`USERPROFILE`
    // is not consulted, and pointing cargo's own home at a temp dir would make
    // `cargo run` re-fetch the registry). Besides the unix-only rc file check,
    // the test therefore asserts platform-neutral symptoms of the guard not
    // firing: no integration script in the sandboxed config dir and no prompt
    // on stderr. On Windows a regression shows up as a stray `try-rs.ps1` plus
    // a write to the real profile — the guard failing is the alarm.
    let home = TempDir::new("try-home-no-rc").unwrap();
    let config_dir = TempDir::new("try-config-no-rc").unwrap();

    #[cfg(unix)]
    let (shell, rc_file, original, integration_script) = {
        let bashrc = home.path().join(".bashrc");
        let original = "# user bashrc\n";
        fs::write(&bashrc, original).unwrap();
        (
            "/bin/bash",
            bashrc,
            original,
            config_dir.path().join("try-rs.bash"),
        )
    };
    #[cfg(windows)]
    let (shell, integration_script) = (
        "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
        config_dir.path().join("try-rs.ps1"),
    );

    let p = Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg("no-rc-proj")
        .env("HOME", home.path())
        .env("SHELL", shell)
        .env_remove("NU_VERSION")
        .env_remove("TRY_PATH")
        .env("TRY_CONFIG_DIR", config_dir.path())
        .output()
        .expect("failed to spawn");

    assert!(p.status.success());

    let stderr = String::from_utf8(p.stderr).unwrap();
    assert!(
        !stderr.contains("Detected shell"),
        "non-interactive run must not offer shell setup:\n{}",
        stderr
    );
    assert!(
        !integration_script.exists(),
        "non-interactive run must not write {}",
        integration_script.display()
    );

    #[cfg(unix)]
    assert_eq!(
        fs::read_to_string(&rc_file).unwrap(),
        original,
        "non-interactive run must not modify the rc file"
    );
}

#[test]
fn try_path_env_overrides_config() {
    let h = Harness::new(false);
    let override_dir = TempDir::new("try-override-path").unwrap();

    let p = h.run_try_with_env(
        &["env-proj"],
        "TRY_PATH",
        override_dir.path().to_str().unwrap(),
    );

    // The cd should point to the TRY_PATH override dir, not the config's tries_path
    assert!(
        p.stdout
            .contains(&override_dir.path().to_string_lossy().to_string()),
        "TRY_PATH env should override config tries_path"
    );
}
