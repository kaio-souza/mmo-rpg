//! "Play" and "New game" buttons: the editor runs the kit's `mmo` (next
//! to it in the same folder, else on PATH) so a designer never opens a
//! terminal. Play runs `mmo play` on this game; New game runs `mmo new`
//! beside this game's folder and opens a second editor on the new one.

use std::io::{BufRead, BufReader};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A start that fails sooner than this is reported to the page.
const EARLY_EXIT: Duration = Duration::from_millis(1500);
/// How long a new editor may take to listen.
const EDITOR_WAIT: Duration = Duration::from_secs(10);
/// Lines of a failed start shown to the designer.
const TAIL_LINES: usize = 6;

/// The game being played, if any, and the last lines it wrote.
#[derive(Default)]
pub struct Play {
    child: Option<Child>,
    tail: Arc<Mutex<Vec<String>>>,
}

impl Play {
    /// Whether `mmo play` still runs (a finished one is forgotten).
    pub fn running(&mut self) -> bool {
        let done = match self.child.as_mut().map(Child::try_wait) {
            Some(Ok(None)) => false,
            Some(_) => true,
            None => return false,
        };
        if done {
            self.child = None;
        }
        !done
    }

    /// Starts `mmo play` in the game folder; a start that fails at once
    /// (port taken, binaries missing) returns its last lines.
    pub fn start(&mut self, game: &Path) -> Result<(), String> {
        if self.running() {
            return Err("the game is already running: close its window first".into());
        }
        let mut child = Command::new(kit_tool("mmo"))
            .arg("play")
            .current_dir(game)
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot run mmo: {e}"))?;
        self.tail = Arc::new(Mutex::new(Vec::new()));
        if let Some(err) = child.stderr.take() {
            let tail = Arc::clone(&self.tail);
            std::thread::spawn(move || {
                for line in BufReader::new(err).lines().map_while(Result::ok) {
                    // The terminal still shows everything the game writes.
                    eprintln!("{line}");
                    if let Ok(mut t) = tail.lock() {
                        t.push(line);
                        let extra = t.len().saturating_sub(TAIL_LINES);
                        t.drain(..extra);
                    }
                }
            });
        }
        let started = Instant::now();
        while started.elapsed() < EARLY_EXIT {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                // Give the reader a moment to take the last lines.
                std::thread::sleep(Duration::from_millis(100));
                let tail = self.tail.lock().map(|t| t.join("\n")).unwrap_or_default();
                return Err(if tail.is_empty() {
                    format!("the game stopped: {status}")
                } else {
                    tail
                });
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        self.child = Some(child);
        Ok(())
    }
}

/// Folder names a new game may have: what ids allow, so any system takes it.
pub fn valid_name(name: &str) -> bool {
    (1..=40).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

/// Where a new game called `name` goes: beside this game's folder.
pub fn new_game_dir(game: &Path, name: &str) -> Result<PathBuf, String> {
    if !valid_name(name) {
        return Err(format!("`{name}`: use only a-z, 0-9, _ and -"));
    }
    let parent = game.parent().ok_or("this game's folder has no parent folder")?;
    let dir = parent.join(name);
    if dir.exists() {
        return Err(format!("{} already exists", dir.display()));
    }
    Ok(dir)
}

/// Copies the template to `dir` with `mmo new`, then opens an editor on
/// it; returns that editor's address.
pub fn new_game(dir: &Path) -> Result<String, String> {
    let out = Command::new(kit_tool("mmo"))
        .arg("new")
        .arg(dir)
        .output()
        .map_err(|e| format!("cannot run mmo: {e}"))?;
    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stderr);
        return Err(msg.trim().trim_start_matches("mmo: ").to_owned());
    }
    let port = free_port()?;
    Command::new(kit_tool("editor"))
        .args(["--port", &port.to_string()])
        .current_dir(dir)
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot open the editor on the new game: {e}"))?;
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let started = Instant::now();
    while started.elapsed() < EDITOR_WAIT {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
            return Ok(format!("http://{addr}"));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err(format!(
        "the game was created in {}, but its editor did not answer",
        dir.display()
    ))
}

fn free_port() -> Result<u16, String> {
    let l = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
    l.local_addr().map(|a| a.port()).map_err(|e| e.to_string())
}

/// A kit binary next to the editor (both are in `target/release`), else
/// the bare name for PATH.
fn kit_tool(name: &str) -> PathBuf {
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe()
        .ok()
        .and_then(|me| me.parent().map(|d| d.join(&file)))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(name))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn names_are_plain_folder_names() {
        assert!(valid_name("my_game-2"));
        for bad in ["", "../x", "a/b", "Game", "jogo novo", &"a".repeat(41)] {
            assert!(!valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn a_new_game_goes_beside_this_one() {
        let game = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara");
        let dir = new_game_dir(&game, "zz_not_there").unwrap();
        assert_eq!(dir, game.parent().unwrap().join("zz_not_there"));
        assert!(new_game_dir(&game, "riftara").is_err());
    }
}
