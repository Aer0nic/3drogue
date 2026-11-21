//! Making sure the game has a terminal to run in.
//!
//! Started from a terminal (or Windows' command prompt), the game simply runs
//! there. Double-clicked from a desktop:
//! - on Windows the game is a console program, so Windows opens a console
//!   window for it on its own;
//! - on Linux there is no terminal at all, so the game relaunches itself
//!   inside the first terminal emulator it can find.

/// Set on the relaunched copy, so it never tries to relaunch again.
const RELAUNCHED: &str = "GRAZZARK_IN_TERMINAL";

/// Returns true if this process should exit because the game has been
/// relaunched in a terminal window.
pub fn ensure_terminal(args: &[String]) -> bool {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use std::io::IsTerminal;
        if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
            return false;
        }
        if std::env::var_os(RELAUNCHED).is_some() {
            return false;
        }
        let graphical =
            std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some();
        if !graphical {
            return false;
        }
        let Ok(exe) = std::env::current_exe() else {
            return false;
        };
        let exe = exe.to_string_lossy().to_string();
        let mut cmd_line: Vec<String> = vec![exe];
        cmd_line.extend(args.iter().skip(1).cloned());
        // (terminal, the flag that introduces the command to run)
        let terminals: [(&str, &[&str]); 11] = [
            ("x-terminal-emulator", &["-e"]),
            ("gnome-terminal", &["--"]),
            ("konsole", &["-e"]),
            ("xfce4-terminal", &["-x"]),
            ("mate-terminal", &["-x"]),
            ("tilix", &["-e"]),
            ("kitty", &[]),
            ("alacritty", &["-e"]),
            ("wezterm", &["start", "--"]),
            ("foot", &[]),
            ("xterm", &["-e"]),
        ];
        for (term, flag) in terminals.iter() {
            let found = std::env::var_os("PATH")
                .map(|p| std::env::split_paths(&p).any(|d| d.join(term).is_file()))
                .unwrap_or(false);
            if !found {
                continue;
            }
            let spawned = std::process::Command::new(term)
                .args(flag.iter())
                .args(cmd_line.iter())
                .env(RELAUNCHED, "1")
                .spawn();
            if spawned.is_ok() {
                return true;
            }
        }
        false
    }
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    {
        let _ = (args, RELAUNCHED);
        false
    }
}

/// Keep a console window open long enough to read an error. Only needed on
/// Windows, where a double-clicked console closes the moment the game exits.
pub fn pause_before_exit() {
    #[cfg(windows)]
    {
        use std::io::{BufRead, Write};
        print!("\nPress Enter to close this window...");
        let _ = std::io::stdout().flush();
        let mut s = String::new();
        let _ = std::io::stdin().lock().read_line(&mut s);
    }
}

/// On Windows, make a freshly opened console big enough for the game.
pub fn grow_console() {
    #[cfg(windows)]
    {
        use crossterm::{execute, terminal};
        if let Ok((w, h)) = terminal::size() {
            if w < 120 || h < 36 {
                let _ = execute!(std::io::stdout(), terminal::SetSize(w.max(120), h.max(36)));
            }
        }
    }
}
