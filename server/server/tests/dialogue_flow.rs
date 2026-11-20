//! Phase 3 slice: an NPC defined only in `content/` is seen, talks in the
//! player's locale through the text pack, and refuses from afar. Runs the
//! real binary in dev mode with the repository content.
#![allow(clippy::unwrap_used, clippy::expect_used)] // test helpers

use base::{CharacterName, EntityId, Locale, TextRef, TilePos};
use i18n::Bundle;
use protocol::{ClientMsg, EntityKind, EntityName, ErrorCode, MAX_SERVER_FRAME, PROTOCOL_VERSION, ServerMsg};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Where `content/zones/dev_field/npcs.ron` puts the guide.
/// At Aurelia's south gate since the alpha map.
const GUIDE: TilePos = TilePos::new(36, -31);

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Client {
    s: TcpStream,
    buf: Vec<u8>,
}

impl Client {
    async fn send(&mut self, m: &ClientMsg) {
        let mut b = Vec::new();
        protocol::encode(m, &mut b).unwrap();
        self.s.write_all(&b).await.unwrap();
    }

    /// Next message for which `keep` returns `Some`, skipping the rest.
    async fn until<T>(&mut self, mut keep: impl FnMut(ServerMsg) -> Option<T>) -> T {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        loop {
            while let Some((m, used)) =
                protocol::decode_frame::<ServerMsg>(&self.buf, MAX_SERVER_FRAME).unwrap()
            {
                self.buf.drain(..used);
                if let Some(v) = keep(m) {
                    return v;
                }
            }
            let mut chunk = vec![0u8; 256 * 1024];
            let n = tokio::time::timeout_at(deadline, self.s.read(&mut chunk))
                .await
                .expect("server answer in time")
                .unwrap();
            assert!(n > 0, "server closed the connection");
            self.buf.extend_from_slice(&chunk[..n]);
        }
    }
}

#[tokio::test]
async fn npc_dialogue_in_the_players_locale() {
    let data: PathBuf = std::env::temp_dir().join(format!("server-dialogue-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara/content");
    let _server = Server(
        Command::new(env!("CARGO_BIN_EXE_server"))
            .args(["--dev", "--address", &format!("127.0.0.1:{port}")])
            .arg("--data-dir")
            .arg(&data)
            .arg("--content")
            .arg(content)
            .args(["--zone", "aurelia", "--threads", "2"])
            // Content with translations still missing runs in English.
            .arg("--allow-missing-text")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
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
        s: s.expect("server did not start"),
        buf: Vec::new(),
    };
    c.send(&ClientMsg::Hello {
        version: PROTOCOL_VERSION,
        locale: Locale::from_static("pt-BR"),
    })
    .await;
    c.send(&ClientMsg::DevLogin {
        name: CharacterName::try_from("Talker").unwrap(),
    })
    .await;
    let (me, hash) = c
        .until(|m| match m {
            ServerMsg::Welcome {
                your_id,
                content_hash,
                ..
            } => Some((your_id, content_hash)),
            _ => None,
        })
        .await;

    // The guide is in view (the snapshot follows `Welcome`).
    let npc = c
        .until(|m| {
            let entities = match m {
                ServerMsg::Snapshot { entities } => entities,
                ServerMsg::Diff(d) => d.entered,
                _ => return None,
            };
            entities
                .into_iter()
                .find(|e| e.kind == EntityKind::Npc && e.pos == GUIDE)
        })
        .await;
    // The text pack of the content that is running, in Portuguese.
    c.send(&ClientMsg::RequestTextPack {
        locale: Locale::from_static("pt-BR"),
    })
    .await;
    let bundle = c
        .until(|m| match m {
            // The repository's pack fits one part.
            ServerMsg::TextPack {
                locale,
                content_hash,
                part: 0,
                parts: 1,
                data,
            } => Some((locale, content_hash, data)),
            _ => None,
        })
        .await;
    assert_eq!((bundle.0, bundle.1), (Locale::from_static("pt-BR"), hash));
    let bundle = Bundle::from_pack(&bundle.2).unwrap();
    let say = |r: &TextRef| i18n::format(&bundle, r);

    // Its name, translated.
    let EntityName::Npc(name) = npc.name else {
        panic!("an NPC carries a text id")
    };
    assert_eq!(say(&TextRef::new(name)), "Guia");
    let guide: EntityId = npc.id;

    // From the spawn it is too far.
    c.send(&ClientMsg::Talk { target: guide }).await;
    let code = c
        .until(|m| match m {
            ServerMsg::Error { code, .. } => Some(code),
            _ => None,
        })
        .await;
    assert_eq!(code, ErrorCode::TooFar);

    // Next to it, the greeting and four options (one only shows because
    // the quest `first_steps` has not started).
    let next_to = TilePos::new(GUIDE.x - 1, GUIDE.y);
    c.send(&ClientMsg::Move { destination: next_to }).await;
    c.until(|m| match m {
        ServerMsg::Diff(d) => d
            .moved
            .iter()
            .any(|(id, pos, _)| *id == me && *pos == next_to)
            .then_some(()),
        _ => None,
    })
    .await;
    c.send(&ClientMsg::Talk { target: guide }).await;
    let (text, options) = c
        .until(|m| match m {
            ServerMsg::Dialogue { npc, text, options } if npc == guide => Some((text, options)),
            _ => None,
        })
        .await;
    assert!(say(&text).starts_with("Bem-vindo"), "{}", say(&text));
    assert_eq!(options.len(), 4);
    assert_eq!(say(&options[0]), "Onde estou?");

    // First option leads on; "Até logo." closes.
    c.send(&ClientMsg::ChooseOption { option: 0 }).await;
    let text = c
        .until(|m| match m {
            ServerMsg::Dialogue { text, .. } => Some(text),
            _ => None,
        })
        .await;
    assert!(say(&text).contains("Aurelia"), "{}", say(&text));
    c.send(&ClientMsg::ChooseOption { option: 1 }).await;
    let closed = c
        .until(|m| match m {
            ServerMsg::DialogueEnd { npc } => Some(npc),
            _ => None,
        })
        .await;
    assert_eq!(closed, guide);
    let _ = std::fs::remove_dir_all(&data);
}
