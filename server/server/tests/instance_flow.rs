//! Instances (roadmap G6): the template's meadow with a portal next to the
//! plaza into a crypt marked `instance`. A player who steps on it lands in
//! a copy made for them, on the same connection (no second `Welcome`, the
//! same entity); leaving the game inside saves the character at the
//! crypt's exit, since copies never outlive the session; when its time
//! runs out, everyone inside goes to the exit. Real binary.
#![allow(clippy::unwrap_used, clippy::expect_used)] // test helpers

use base::{CharacterName, EntityId, Locale, TilePos};
use protocol::{ClientMsg, MAX_SERVER_FRAME, PROTOCOL_VERSION, ServerMsg};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// On the plaza, east of the spawn: the portal tile.
const PORTAL: TilePos = TilePos::new(23, 15);
/// Where the portal lands, inside the crypt (far from the meadow).
const INSIDE: TilePos = TilePos::new(1002, 2);
/// The crypt's exit, back on the plaza.
const EXIT: TilePos = TilePos::new(18, 15);

struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let path = e.path();
        if path.is_dir() {
            copy_dir(&path, &to.join(e.file_name()));
        } else {
            std::fs::copy(&path, to.join(e.file_name())).unwrap();
        }
    }
}

/// The template's content, a portal on the plaza and a 6x5 crypt.
fn content(dir: &Path) {
    let template = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../templates/new-game/content");
    copy_dir(&template, dir);
    let zone = dir.join("zones/meadow/zone.ron");
    let text = std::fs::read_to_string(&zone).unwrap();
    let text = text.replacen(
        "    regions: [",
        "    portals: [(min: (x: 23, y: 15), max: (x: 23, y: 15), to: (x: 1002, y: 2))],\n    regions: [",
        1,
    );
    std::fs::write(&zone, text).unwrap();
    let crypt = dir.join("zones/crypt");
    std::fs::create_dir_all(&crypt).unwrap();
    std::fs::write(
        crypt.join("zone.ron"),
        "(origin: (x: 1000, y: 0), view_radius: 20, spawn: (x: 1001, y: 1), \
         instance: Some((exit: (x: 18, y: 15), close_after_secs: 2, time_limit_secs: 4)))",
    )
    .unwrap();
    let row = "\"......\"";
    std::fs::write(
        crypt.join("map.ron"),
        format!("(legend: {{ '.': (floor: 1) }}, rows: [{}])", [row; 5].join(", ")),
    )
    .unwrap();
}

struct Client {
    s: TcpStream,
    buf: Vec<u8>,
    me: EntityId,
    pos: TilePos,
    welcomes: u32,
}

impl Client {
    async fn connect(port: u16, name: &str) -> Client {
        let mut s = None;
        for _ in 0..300 {
            if let Ok(c) = TcpStream::connect(("127.0.0.1", port)).await {
                s = Some(c);
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let mut c = Client {
            s: s.expect("server did not start"),
            buf: Vec::new(),
            me: EntityId::new(0),
            pos: TilePos::new(0, 0),
            welcomes: 0,
        };
        c.send(&ClientMsg::Hello {
            version: PROTOCOL_VERSION,
            locale: Locale::SOURCE,
        })
        .await;
        c.send(&ClientMsg::DevLogin {
            name: CharacterName::try_from(name).unwrap(),
        })
        .await;
        c.until(30, |c| c.welcomes == 1).await;
        c
    }

    async fn send(&mut self, m: &ClientMsg) {
        let mut b = Vec::new();
        protocol::encode(m, &mut b).unwrap();
        self.s.write_all(&b).await.unwrap();
    }

    async fn until(&mut self, secs: u64, mut done: impl FnMut(&Client) -> bool) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
        loop {
            while let Some((m, used)) =
                protocol::decode_frame::<ServerMsg>(&self.buf, MAX_SERVER_FRAME).unwrap()
            {
                self.buf.drain(..used);
                match m {
                    ServerMsg::Welcome { your_id, pos, .. } => {
                        self.welcomes += 1;
                        self.me = your_id;
                        self.pos = pos;
                    }
                    ServerMsg::Snapshot { entities } => {
                        if let Some(e) = entities.iter().find(|e| e.id == self.me) {
                            self.pos = e.pos;
                        }
                    }
                    ServerMsg::Diff(d) => {
                        for (id, pos, _) in d.moved {
                            if id == self.me {
                                self.pos = pos;
                            }
                        }
                    }
                    ServerMsg::Error { code, .. } => panic!("server error {code:?}"),
                    _ => {}
                }
                if done(self) {
                    return;
                }
            }
            let mut chunk = vec![0u8; 256 * 1024];
            let n = tokio::time::timeout_at(deadline, self.s.read(&mut chunk))
                .await
                .unwrap_or_else(|_| panic!("timed out at {:?}", self.pos))
                .unwrap();
            assert!(n > 0, "server closed the connection");
            self.buf.extend_from_slice(&chunk[..n]);
        }
    }
}

#[tokio::test]
async fn a_portal_opens_a_copy_and_leaving_inside_saves_at_the_exit() {
    let root: PathBuf = std::env::temp_dir().join(format!("server-instance-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (dir, data) = (root.join("content"), root.join("data"));
    content(&dir);
    let port = free_port();
    let _server = Process(
        Command::new(env!("CARGO_BIN_EXE_server"))
            .args([
                "--dev",
                "--address",
                &format!("127.0.0.1:{port}"),
                "--threads",
                "2",
            ])
            .arg("--data-dir")
            .arg(&data)
            .arg("--content")
            .arg(&dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut c = Client::connect(port, "Delver").await;
    let me = c.me;
    c.send(&ClientMsg::Move { destination: PORTAL }).await;
    c.until(30, |c| c.pos.x >= 1000).await;
    assert!(c.pos.distance(INSIDE) <= 1, "in the crypt: {:?}", c.pos);
    assert_eq!((c.welcomes, c.me), (1, me), "the same entry and entity");

    // Leaving inside: saved at the exit, not in a copy that will be gone.
    drop(c);
    tokio::time::sleep(Duration::from_secs(2)).await;
    let mut c = Client::connect(port, "Delver").await;
    assert!(c.pos.distance(EXIT) <= 2, "back at the exit: {:?}", c.pos);

    // A new copy; its time runs out with the player inside.
    c.send(&ClientMsg::Move { destination: PORTAL }).await;
    c.until(30, |c| c.pos.x >= 1000).await;
    c.until(30, |c| c.pos.x < 1000).await;
    assert!(c.pos.distance(EXIT) <= 2, "sent to the exit: {:?}", c.pos);
    assert_eq!(c.welcomes, 1);
    let _ = std::fs::remove_dir_all(&root);
}
