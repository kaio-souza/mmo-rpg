//! Phase 10 criterion: the gateway and two zones run as three
//! processes linked by TCP; a player born in Aurelia walks down the road and
//! over the seam into the field without noticing: no second `Welcome`, the
//! same entity id, the walk goes on, and the gateway saves the character
//! under the zone it ends in. Real binary, repository content.
#![allow(clippy::unwrap_used, clippy::expect_used)] // test helpers

use base::{CharacterName, EntityId, Locale, TilePos};
use protocol::{ClientMsg, MAX_SERVER_FRAME, PROTOCOL_VERSION, ServerMsg};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Aurelia's summoning circle (`content/zones/aurelia/zone.ron`).
const CIRCLE: TilePos = TilePos::new(32, -44);
/// Down the plaza, through the south gate, along the road, onto the field:
/// the last step crosses the seam at y = 0 (`dev_field` starts there).
const ROUTE: [TilePos; 3] = [TilePos::new(31, -30), TilePos::new(31, -5), TilePos::new(31, 5)];

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

fn server(args: &[&str], content: &Path, data: &Path) -> Process {
    Process(
        Command::new(env!("CARGO_BIN_EXE_server"))
            .args(args)
            .arg("--data-dir")
            .arg(data)
            .arg("--content")
            .arg(content)
            .args(["--threads", "2", "--allow-missing-text"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    )
}

struct Client {
    s: TcpStream,
    buf: Vec<u8>,
    me: EntityId,
    pos: TilePos,
    welcomes: u32,
    snapshots: u32,
}

impl Client {
    async fn send(&mut self, m: &ClientMsg) {
        let mut b = Vec::new();
        protocol::encode(m, &mut b).unwrap();
        self.s.write_all(&b).await.unwrap();
    }

    /// Reads until `done` holds, tracking the player's own tile.
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
                        self.snapshots += 1;
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
async fn a_player_crosses_from_one_zone_process_to_another() {
    let data: PathBuf = std::env::temp_dir().join(format!("server-seam-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara/content");
    let (port, link) = (free_port(), free_port());
    let link_addr = format!("127.0.0.1:{link}");
    let _gateway = server(
        &[
            "--mode",
            "gateway",
            "--dev",
            "--address",
            &format!("127.0.0.1:{port}"),
            "--link-listen",
            &link_addr,
        ],
        &content,
        &data,
    );
    let _aurelia = server(
        &["--mode", "zone", "--zones", "aurelia", "--gateway", &link_addr],
        &content,
        &data,
    );
    let _field = server(
        &["--mode", "zone", "--zones", "dev_field", "--gateway", &link_addr],
        &content,
        &data,
    );

    let mut s = None;
    for _ in 0..300 {
        if let Ok(c) = TcpStream::connect(("127.0.0.1", port)).await {
            s = Some(c);
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let mut c = Client {
        s: s.expect("gateway did not start"),
        buf: Vec::new(),
        me: EntityId::new(0),
        pos: TilePos::new(0, 0),
        welcomes: 0,
        snapshots: 0,
    };
    c.send(&ClientMsg::Hello {
        version: PROTOCOL_VERSION,
        locale: Locale::SOURCE,
    })
    .await;
    c.send(&ClientMsg::DevLogin {
        name: CharacterName::try_from("Crosser").unwrap(),
    })
    .await;
    // The entry waits in the queue until the start zone's process links.
    c.until(30, |c| c.welcomes == 1).await;
    assert!(c.pos.distance(CIRCLE) <= 2, "born on the circle: {:?}", c.pos);
    let me = c.me;
    // Both zones up and hearing each other before the walk.
    tokio::time::sleep(Duration::from_secs(2)).await;

    for step in ROUTE {
        c.send(&ClientMsg::Move { destination: step }).await;
        c.until(30, |c| c.pos == step).await;
    }
    assert_eq!(c.welcomes, 1, "crossing the seam is not a new entry");
    assert_eq!(c.me, me, "the same entity on both sides");
    assert!(c.snapshots >= 2, "the field's process sent the view anew");

    // Leaving saves the character in the field, where it now stands.
    drop(c);
    tokio::time::sleep(Duration::from_secs(1)).await;
    let s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let mut c = Client {
        s: {
            let _ = s.set_nodelay(true);
            s
        },
        buf: Vec::new(),
        me: EntityId::new(0),
        pos: TilePos::new(0, 0),
        welcomes: 0,
        snapshots: 0,
    };
    c.send(&ClientMsg::Hello {
        version: PROTOCOL_VERSION,
        locale: Locale::SOURCE,
    })
    .await;
    c.send(&ClientMsg::DevLogin {
        name: CharacterName::try_from("Crosser").unwrap(),
    })
    .await;
    c.until(30, |c| c.welcomes == 1).await;
    assert_eq!(c.pos, ROUTE[2], "back on the field, in the field's process");
    let _ = std::fs::remove_dir_all(&data);
}
