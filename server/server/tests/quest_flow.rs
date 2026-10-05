//! Phase 3 criterion: a quest made only of content (dialogue
//! conditions and effects, items) is played through the real
//! binary; the character keeps coins, items and the quest after logging in
//! again; editing `content/` while the server runs reaches the client as
//! `TextPackChanged` without a restart, and broken content is not loaded.

use base::{CharacterName, EntityId, Locale, TextRef, TilePos};
use i18n::Bundle;
use protocol::{
    ClientMsg, EntityKind, ItemInfo, MAX_SERVER_FRAME, ObjectiveView, PROTOCOL_VERSION, QuestEntry,
    QuestState, ServerMsg,
};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Aurelia since the alpha map: the guide at the south gate, the herbalist
/// in the market quarter (out of sight from the circle).
const GUIDE: TilePos = TilePos::new(36, -31);
const HERBALIST: TilePos = TilePos::new(-13, -56);
/// On the main street between them: a `Move` goes at most 32 tiles.
const STREET: TilePos = TilePos::new(14, -45);

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
    /// Arrived but not asked for yet, in order: messages for one step may
    /// come before the step that waits for them.
    stash: Vec<ServerMsg>,
    me: EntityId,
    hash: u64,
    bundle: Option<Bundle>,
    items: Vec<ItemInfo>,
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let target = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), target).unwrap();
        }
    }
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
            stash: Vec::new(),
            me: EntityId::new(0),
            hash: 0,
            bundle: None,
            items: Vec::new(),
        };
        c.send(&ClientMsg::Hello {
            version: PROTOCOL_VERSION,
            locale: Locale::EN,
        })
        .await;
        c.send(&ClientMsg::DevLogin {
            name: CharacterName::try_from(name).unwrap(),
        })
        .await;
        let (me, hash) = c
            .until(|m| match m {
                ServerMsg::Welcome {
                    your_id,
                    content_hash,
                    ..
                } => Some((*your_id, *content_hash)),
                _ => None,
            })
            .await;
        (c.me, c.hash) = (me, hash);
        c.items = c.until(definitions).await;
        c.text_pack().await;
        c
    }

    async fn send(&mut self, m: &ClientMsg) {
        let mut b = Vec::new();
        protocol::encode(m, &mut b).unwrap();
        self.s.write_all(&b).await.unwrap();
    }

    /// First message (stashed or new) for which `keep` returns `Some`;
    /// the others stay stashed.
    async fn until<T>(&mut self, mut keep: impl FnMut(&ServerMsg) -> Option<T>) -> T {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        for i in 0..self.stash.len() {
            if let Some(v) = keep(&self.stash[i]) {
                self.stash.remove(i);
                return v;
            }
        }
        loop {
            while let Some((m, used)) =
                protocol::decode_frame::<ServerMsg>(&self.buf, MAX_SERVER_FRAME).unwrap()
            {
                self.buf.drain(..used);
                if let Some(v) = keep(&m) {
                    return v;
                }
                self.stash.push(m);
            }
            let mut chunk = vec![0u8; 256 * 1024];
            let n = tokio::time::timeout_at(deadline, self.s.read(&mut chunk))
                .await
                .unwrap_or_else(|_| {
                    let log: Vec<&ServerMsg> = self
                        .stash
                        .iter()
                        .filter(|m| !matches!(m, ServerMsg::Diff(_) | ServerMsg::Chunk { .. }))
                        .collect();
                    panic!("no answer in time; waiting with {log:#?}")
                })
                .unwrap();
            assert!(n > 0, "server closed the connection");
            self.buf.extend_from_slice(&chunk[..n]);
        }
    }

    /// Asks for the English pack of the running content and joins its parts.
    async fn text_pack(&mut self) {
        self.send(&ClientMsg::RequestTextPack { locale: Locale::EN })
            .await;
        let hash = self.hash;
        let mut data = Vec::new();
        loop {
            let (part, parts, piece) = self
                .until(|m| match m {
                    ServerMsg::TextPack {
                        content_hash,
                        part,
                        parts,
                        data,
                        ..
                    } if *content_hash == hash => Some((*part, *parts, data.clone())),
                    _ => None,
                })
                .await;
            data.extend(piece);
            if part + 1 == parts {
                break;
            }
        }
        self.bundle = Some(Bundle::from_pack(&data).unwrap());
    }

    fn say(&self, id: base::TextId) -> String {
        i18n::format(self.bundle.as_ref().unwrap(), &TextRef::new(id))
    }

    async fn walk_next_to(&mut self, npc: TilePos) {
        self.walk(TilePos::new(npc.x + 1, npc.y)).await;
    }

    async fn walk(&mut self, to: TilePos) {
        self.send(&ClientMsg::Move { destination: to }).await;
        let me = self.me;
        self.until(|m| match m {
            ServerMsg::Diff(d) => d
                .moved
                .iter()
                .any(|(id, p, _)| *id == me && *p == to)
                .then_some(()),
            _ => None,
        })
        .await;
    }

    /// Talks to the NPC on `tile`; returns the node text and options.
    async fn talk(&mut self, npc: EntityId) -> (String, Vec<String>) {
        self.send(&ClientMsg::Talk { target: npc }).await;
        self.node().await
    }

    async fn node(&mut self) -> (String, Vec<String>) {
        let (text, options) = self
            .until(|m| match m {
                ServerMsg::Dialogue { text, options, .. } => Some((text.clone(), options.clone())),
                _ => None,
            })
            .await;
        (
            self.say(text.id),
            options.iter().map(|o| self.say(o.id)).collect(),
        )
    }

    /// Picks the option with this text.
    async fn pick(&mut self, options: &[String], text: &str) {
        let i = options
            .iter()
            .position(|o| o == text)
            .unwrap_or_else(|| panic!("no `{text}` in {options:?}"));
        self.send(&ClientMsg::ChooseOption { option: i as u16 }).await;
    }

    /// Waits for a backpack with these coins and items (by English name).
    async fn inventory(&mut self, coins: u64, items: &[(&str, u32)]) {
        let names: Vec<String> = self.items.iter().map(|d| self.say(d.name)).collect();
        let want: Vec<(String, u32)> = items.iter().map(|(n, q)| ((*n).to_owned(), *q)).collect();
        self.until(|m| match m {
            ServerMsg::Inventory { coins: c, slots, .. } => {
                let have: Vec<(String, u32)> = slots
                    .iter()
                    .flatten()
                    .map(|s| (names[s.item.index()].clone(), s.qty))
                    .collect();
                (*c == coins && have == want).then_some(())
            }
            _ => None,
        })
        .await
    }

    /// Waits for a quest log that passes `ok`.
    async fn quests(&mut self, mut ok: impl FnMut(&[QuestEntry]) -> bool) -> Vec<QuestEntry> {
        self.until(|m| match m {
            ServerMsg::QuestLog { quests } if ok(quests) => Some(quests.clone()),
            _ => None,
        })
        .await
    }
}

fn definitions(m: &ServerMsg) -> Option<Vec<ItemInfo>> {
    match m {
        ServerMsg::Definitions { items, .. } => Some(items.clone()),
        _ => None,
    }
}

/// The NPC that comes into view on `tile` (other than `not`).
async fn npc_at(c: &mut Client, tile: TilePos, not: Option<EntityId>) -> EntityId {
    c.until(|m| {
        let entities = match m {
            ServerMsg::Snapshot { entities } => entities,
            ServerMsg::Diff(d) => &d.entered,
            _ => return None,
        };
        entities
            .iter()
            .find(|e| e.kind == EntityKind::Npc && e.pos == tile && Some(e.id) != not)
            .map(|e| e.id)
    })
    .await
}

#[tokio::test]
async fn quest_from_content_saved_and_reloaded() {
    let root: PathBuf = std::env::temp_dir().join(format!("server-quest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let content = root.join("content");
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara/content"),
        &content,
    );
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    // `--dev` watches the content folder.
    let _server = Server(
        Command::new(env!("CARGO_BIN_EXE_server"))
            .args(["--dev", "--address", &format!("127.0.0.1:{port}")])
            .arg("--data-dir")
            .arg(root.join("data"))
            .arg("--content")
            .arg(&content)
            .args(["--zone", "aurelia", "--threads", "2"])
            // Content with translations still missing runs in English.
            .arg("--allow-missing-text")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut c = Client::connect(port, "Quester").await;
    // Every item of `content/` arrives in `Definitions`: one per file, so
    // new items never break this test.
    let item_files = std::fs::read_dir(content.join("items"))
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .is_ok_and(|e| e.path().extension().is_some_and(|x| x == "ron"))
        })
        .count();
    assert_eq!(c.items.len(), item_files);
    // The guide is in sight of the circle.
    let guide = npc_at(&mut c, GUIDE, None).await;
    c.inventory(0, &[]).await;

    // The guide offers the quest.
    c.walk_next_to(GUIDE).await;
    let (_, options) = c.talk(guide).await;
    c.pick(&options, "Is there anything I can do?").await;
    let (_, options) = c.node().await;
    c.pick(&options, "I will do it.").await;
    let log = c.quests(|q| !q.is_empty()).await;
    assert_eq!(c.say(log[0].name), "First steps");
    assert_eq!(log[0].state, QuestState::Active);
    assert!(matches!(
        log[0].objectives[0],
        ObjectiveView::Talk { done: false, .. }
    ));
    let (_, options) = c.node().await;
    assert_eq!(options, ["Farewell."]);

    // The herbalist tops the herbs up to three (TopUpItem); talking counts too.
    c.walk(STREET).await;
    c.walk_next_to(HERBALIST).await;
    let herbalist = npc_at(&mut c, HERBALIST, None).await;
    let (_, options) = c.talk(herbalist).await;
    // The gift, the alchemy lessons, the letter to Fenwick and goodbye.
    assert_eq!(options.len(), 4, "{options:?}");
    c.pick(&options, "The guide sent me for glowing herbs.").await;
    let (gift, _) = c.node().await;
    assert!(gift.starts_with("Another newcomer"), "{gift}");
    c.inventory(0, &[("Glowing herb", 3)]).await;
    c.quests(|q| {
        q.first().is_some_and(|q| {
            matches!(q.objectives[0], ObjectiveView::Talk { done: true, .. })
                && matches!(q.objectives[1], ObjectiveView::Item { have: 3, need: 3, .. })
        })
    })
    .await;
    // The gift option is gone now that the herbs are there.
    let (_, options) = c.talk(herbalist).await;
    assert_eq!(
        options,
        [
            "Could you teach me potions?",
            "Do you need anything carried?",
            "Thank you. Goodbye."
        ]
    );

    // Back at the guide: deliver, get paid.
    c.walk(STREET).await;
    c.walk_next_to(GUIDE).await;
    let (_, options) = c.talk(guide).await;
    assert!(!options.contains(&"Is there anything I can do?".to_owned()));
    c.pick(&options, "I brought the herbs.").await;
    let (thanks, _) = c.node().await;
    assert!(thanks.starts_with("Well done"), "{thanks}");
    c.inventory(150, &[("Travel bread", 2)]).await;
    c.quests(|q| q.first().is_some_and(|q| q.state == QuestState::Completed))
        .await;

    // Logging in again: coins, items and the quest were saved.
    drop(c);
    tokio::time::sleep(Duration::from_millis(500)).await;
    let mut c = Client::connect(port, "Quester").await;
    c.inventory(150, &[("Travel bread", 2)]).await;
    c.quests(|q| q.len() == 1 && q[0].state == QuestState::Completed)
        .await;
    let guide = npc_at(&mut c, GUIDE, None).await;

    // Edit a line while the server runs: the client hears of it.
    let npcs = content.join("text/en/npcs.ron");
    let text = std::fs::read_to_string(&npcs).unwrap();
    std::fs::write(
        &npcs,
        text.replace("Welcome, newcomer.", "Welcome back, newcomer."),
    )
    .unwrap();
    let hash = c
        .until(|m| match m {
            ServerMsg::TextPackChanged { content_hash } => Some(*content_hash),
            _ => None,
        })
        .await;
    assert_ne!(hash, c.hash);
    c.hash = hash;
    c.items = c.until(definitions).await;
    c.text_pack().await;
    // Players keep their data through the reload; NPCs spawn again.
    c.inventory(150, &[("Travel bread", 2)]).await;
    let new_guide = npc_at(&mut c, GUIDE, Some(guide)).await;
    // Still next to the guide: the character came back where it left.
    let (greeting, _) = c.talk(new_guide).await;
    assert!(greeting.starts_with("Welcome back, newcomer."), "{greeting}");

    // Broken content is not loaded: no new hash, and the guide still talks.
    std::fs::write(content.join("items/broken.ron"), "(kind: Nonsense)").unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;
    c.send(&ClientMsg::Talk { target: new_guide }).await;
    let reloaded = c
        .until(|m| match m {
            ServerMsg::TextPackChanged { .. } => Some(true),
            ServerMsg::Dialogue { .. } => Some(false),
            _ => None,
        })
        .await;
    assert!(!reloaded, "broken content must not be loaded");
    let _ = std::fs::remove_dir_all(&root);
}
