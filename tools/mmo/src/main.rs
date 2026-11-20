//! `mmo`: one command for a game. It runs the kit's other tools (`server`,
//! `client`, `editor`, `validate`) that sit next to it in the same folder,
//! so a game is made, checked, played and packaged without their flags.
//!
//! ```text
//! mmo new <folder> [--kit DIR]   copy the new-game template to <folder>
//! mmo edit                       the editor on this game
//! mmo check                      validate this game's content and assets
//! mmo play [--name NAME]         server (--dev) and client together
//! mmo publish [--out DIR]        check, then copy content, assets and the binaries to DIR
//! ```
//!
//! Every command but `new` runs from the game folder.

use clap::{Parser, Subcommand};
use std::fmt::Display;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode};
use std::time::{Duration, Instant};

/// The address `mmo play` uses: the server's default in `--dev`.
const PLAY_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7000);
/// How long `mmo play` waits for the server to answer.
const SERVER_WAIT: Duration = Duration::from_secs(20);

type Outcome<T = ()> = Result<T, String>;

#[derive(Parser)]
#[command(version, about = "One command for a game: new, edit, check, play, publish")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Copy the new-game template to a new folder.
    New {
        /// The folder to create (it must not exist, or be empty).
        folder: PathBuf,
        /// The kit's folder, when `mmo` does not run from inside it.
        #[arg(long)]
        kit: Option<PathBuf>,
    },
    /// Open the editor on this game.
    Edit,
    /// Validate this game's content and assets.
    Check,
    /// Run the server (dev mode) and the client on this game.
    Play {
        /// The player's name: 3 to 16 letters, digits or `_`.
        #[arg(long, default_value = "Dev")]
        name: String,
    },
    /// Check, then copy content, assets and the server and client binaries to a folder.
    Publish {
        /// Where the package goes; it must not exist yet.
        #[arg(long, default_value = "dist")]
        out: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Cmd::New { folder, kit } => new_game(&folder, kit.as_deref()),
        Cmd::Edit => game_here().and_then(|()| run("editor", &[])),
        Cmd::Check => check(),
        Cmd::Play { name } => play(&name),
        Cmd::Publish { out } => publish(&out),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("mmo: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn new_game(folder: &Path, kit: Option<&Path>) -> Outcome {
    let template = template_dir(kit)?;
    if folder.exists() && !is_empty_dir(folder)? {
        return Err(format!(
            "{} already exists and is not an empty folder",
            folder.display()
        ));
    }
    copy_dir(&template, folder)?;
    println!("created {}", folder.display());
    println!("next: cd {} && mmo play", folder.display());
    Ok(())
}

fn check() -> Outcome {
    game_here()?;
    run("validate", &["content", "--assets", "assets"])
}

fn play(name: &str) -> Outcome {
    game_here()?;
    if TcpStream::connect_timeout(&PLAY_ADDR, Duration::from_millis(200)).is_ok() {
        return Err(format!("something already listens on {PLAY_ADDR}: stop it first"));
    }
    let mut server = Command::new(tool("server"))
        .arg("--dev")
        .spawn()
        .map_err(|e| format!("cannot start the server: {e}"))?;
    let addr = PLAY_ADDR.to_string();
    let result = wait_for_server(&mut server)
        .and_then(|()| run("client", &["--dev", "--server", addr.as_str(), "--name", name]));
    // The server runs for the client only: it stops when the client does.
    let _ = server.kill();
    let _ = server.wait();
    result
}

fn publish(out: &Path) -> Outcome {
    game_here()?;
    run("validate", &["content", "--assets", "assets"])?;
    let mut binaries = Vec::new();
    for name in ["server", "client"] {
        let path = sibling(name).ok_or_else(|| {
            format!("the {name} binary is not next to mmo: build the kit with `cargo build --release`")
        })?;
        binaries.push((path, format!("{name}{}", std::env::consts::EXE_SUFFIX)));
    }
    if out.exists() {
        return Err(format!(
            "{} already exists: remove it or pass --out",
            out.display()
        ));
    }
    at(std::fs::create_dir_all(out), out)?;
    for dir in ["content", "assets"] {
        copy_dir(Path::new(dir), &out.join(dir))?;
    }
    for (from, file) in &binaries {
        at(std::fs::copy(from, out.join(file)), from)?;
    }
    println!("package in {}: content, assets, server, client", out.display());
    Ok(())
}

/// A kit tool: next to `mmo` (both are in `target/release`), else on PATH.
fn tool(name: &str) -> PathBuf {
    sibling(name).unwrap_or_else(|| PathBuf::from(name))
}

fn sibling(name: &str) -> Option<PathBuf> {
    let me = std::env::current_exe().ok()?;
    let path = me
        .parent()?
        .join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    path.is_file().then_some(path)
}

/// Runs a kit tool in the current folder and waits; a failure is an error.
fn run(name: &str, args: &[&str]) -> Outcome {
    let status = Command::new(tool(name))
        .args(args)
        .status()
        .map_err(|e| format!("cannot run {name}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{name} stopped: {status}"))
    }
}

/// The current folder must be a game: it has `content/` and `assets/`.
fn game_here() -> Outcome {
    for dir in ["content", "assets"] {
        if !Path::new(dir).is_dir() {
            return Err(format!(
                "no `{dir}/` folder here: run this from a game folder (see `mmo new`)"
            ));
        }
    }
    Ok(())
}

fn template_dir(kit: Option<&Path>) -> Outcome<PathBuf> {
    let found = match kit {
        Some(kit) => Some(kit.join("templates").join("new-game")),
        None => std::env::current_exe().ok().and_then(|me| {
            me.ancestors()
                .map(|a| a.join("templates").join("new-game"))
                .find(|t| t.is_dir())
        }),
    };
    found.filter(|t| t.is_dir()).ok_or_else(|| {
        "the new-game template was not found: run mmo from the kit (target/release), or pass --kit <kit folder>"
            .to_owned()
    })
}

/// Waits until the server accepts connections; fails if it stops first.
fn wait_for_server(server: &mut Child) -> Outcome {
    let started = Instant::now();
    loop {
        if let Some(status) = server.try_wait().map_err(|e| e.to_string())? {
            return Err(format!("the server stopped: {status}"));
        }
        if TcpStream::connect_timeout(&PLAY_ADDR, Duration::from_millis(200)).is_ok() {
            return Ok(());
        }
        if started.elapsed() > SERVER_WAIT {
            return Err(format!(
                "the server did not answer on {PLAY_ADDR} within {SERVER_WAIT:?}"
            ));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn is_empty_dir(path: &Path) -> Outcome<bool> {
    if !path.is_dir() {
        return Ok(false);
    }
    let mut entries = at(std::fs::read_dir(path), path)?;
    Ok(entries.next().is_none())
}

fn copy_dir(from: &Path, to: &Path) -> Outcome {
    at(std::fs::create_dir_all(to), to)?;
    for entry in at(std::fs::read_dir(from), from)? {
        let entry = at(entry, from)?;
        let target = to.join(entry.file_name());
        let source = entry.path();
        if at(entry.file_type(), &source)?.is_dir() {
            copy_dir(&source, &target)?;
        } else {
            at(std::fs::copy(&source, &target), &source)?;
        }
    }
    Ok(())
}

/// Adds the path to an I/O error's message.
fn at<T, E: Display>(result: Result<T, E>, path: &Path) -> Outcome<T> {
    result.map_err(|e| format!("{}: {e}", path.display()))
}
