//! Before the public alpha: a character is deleted from the
//! selection screen and its name freed; an announced restart counts down,
//! saves everyone and stops the server; the hourly backup writes checked
//! copies. Runs the real binary over TLS.
#![allow(clippy::unwrap_used, clippy::expect_used)] // test helpers

use base::{AccountName, CharacterName, Locale};
use persistence::{Repository, Role};
use protocol::{AdminCommand, ClientMsg, ErrorCode, MAX_SERVER_FRAME, PROTOCOL_VERSION, Password, ServerMsg};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

struct Server {
    child: Child,
}

impl Server {
    fn start(data: &Path, port: u16) -> Server {
        let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara/content");
        let child = Command::new(env!("CARGO_BIN_EXE_server"))
            .args(["--address", &format!("127.0.0.1:{port}")])
            .arg("--data-dir")
            .arg(data)
            .arg("--content")
            .arg(content)
            .args(["--zone", "dev_field", "--threads", "2"])
            .args(["--backup-every-secs", "2"])
            // Content with translations still missing runs in English.
            .arg("--allow-missing-text")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        Server { child }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Client {
    s: TlsStream<TcpStream>,
    buf: Vec<u8>,
}

impl Client {
    async fn connect(port: u16, cert: &Path) -> Client {
        let mut roots = rustls::RootCertStore::empty();
        for c in CertificateDer::pem_file_iter(cert).unwrap() {
            roots.add(c.unwrap()).unwrap();
        }
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let cfg = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let tcp = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let s = TlsConnector::from(Arc::new(cfg))
            .connect(ServerName::try_from("localhost").unwrap(), tcp)
            .await
            .unwrap();
        let mut c = Client { s, buf: Vec::new() };
        c.send(&ClientMsg::Hello {
            version: PROTOCOL_VERSION,
            locale: Locale::from_static("pt-BR"),
        })
        .await;
        c
    }

    async fn send(&mut self, m: &ClientMsg) {
        let mut b = Vec::new();
        protocol::encode(m, &mut b).unwrap();
        self.s.write_all(&b).await.unwrap();
        self.s.flush().await.unwrap();
    }

    /// Next message that is not a chunk or a pong.
    async fn recv(&mut self) -> ServerMsg {
        loop {
            if let Some((m, used)) = protocol::decode_frame::<ServerMsg>(&self.buf, MAX_SERVER_FRAME).unwrap()
            {
                self.buf.drain(..used);
                match m {
                    ServerMsg::Chunk { .. }
                    | ServerMsg::Pong { .. }
                    | ServerMsg::Appearances(_)
                    | ServerMsg::Challenge { .. } => continue,
                    m => return m,
                }
            }
            let mut chunk = vec![0u8; 64 * 1024];
            let n = tokio::time::timeout(Duration::from_secs(20), self.s.read(&mut chunk))
                .await
                .expect("server answer")
                .unwrap();
            assert!(n > 0, "server closed the connection");
            self.buf.extend_from_slice(&chunk[..n]);
        }
    }

    /// Skips game messages until an error arrives; `None` if the server
    /// closes first or nothing comes within `secs`.
    async fn error_within(&mut self, secs: u64) -> Option<ErrorCode> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
        loop {
            while let Some((m, used)) =
                protocol::decode_frame::<ServerMsg>(&self.buf, MAX_SERVER_FRAME).unwrap()
            {
                self.buf.drain(..used);
                if let ServerMsg::Error { code, .. } = m {
                    return Some(code);
                }
            }
            let mut chunk = vec![0u8; 64 * 1024];
            let n = match tokio::time::timeout_at(deadline, self.s.read(&mut chunk)).await {
                Ok(Ok(n)) if n > 0 => n,
                _ => return None,
            };
            self.buf.extend_from_slice(&chunk[..n]);
        }
    }

    /// Logs in with `password`; returns the characters.
    async fn login(&mut self, password: &str) -> Vec<protocol::CharacterSummary> {
        self.send(&ClientMsg::Login {
            account: account(),
            password: password_of(password),
            device: None,
            proof: 0,
        })
        .await;
        match self.recv().await {
            ServerMsg::Session { characters, .. } => characters,
            other => panic!("expected a session, got {other:?}"),
        }
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn wait_ready(port: u16) {
    for _ in 0..300 {
        if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("server did not start");
}

fn account() -> AccountName {
    AccountName::try_from("tester").unwrap()
}

fn password_of(s: &str) -> Password {
    Password::try_from(s.to_owned()).unwrap()
}

async fn db(data: &Path) -> Repository {
    Repository::open(&format!("sqlite:{}", data.join("game.db").display())).unwrap()
}

fn name(s: &str) -> CharacterName {
    CharacterName::try_from(s).unwrap()
}

impl Client {
    async fn create(&mut self, n: &str) -> Vec<protocol::CharacterSummary> {
        self.send(&ClientMsg::CreateCharacter {
            look: base::Look::default(),
            name: name(n),
        })
        .await;
        match self.recv().await {
            ServerMsg::Characters { characters } => characters,
            other => panic!("expected the characters, got {other:?}"),
        }
    }

    /// The first `Maintenance` within `secs`, skipping everything else.
    async fn maintenance_within(&mut self, secs: u64) -> Option<u32> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
        loop {
            while let Some((m, used)) =
                protocol::decode_frame::<ServerMsg>(&self.buf, MAX_SERVER_FRAME).unwrap()
            {
                self.buf.drain(..used);
                if let ServerMsg::Maintenance { in_secs } = m {
                    return Some(in_secs);
                }
            }
            let mut chunk = vec![0u8; 64 * 1024];
            let n = match tokio::time::timeout_at(deadline, self.s.read(&mut chunk)).await {
                Ok(Ok(n)) if n > 0 => n,
                _ => return None,
            };
            self.buf.extend_from_slice(&chunk[..n]);
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn delete_a_character_then_restart_announced() {
    let data: PathBuf = std::env::temp_dir().join(format!("server-launch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let cert = data.join("tls/cert.pem");
    let port = free_port();
    let mut server = Server::start(&data, port);
    wait_ready(port).await;

    let mut c = Client::connect(port, &cert).await;
    c.send(&ClientMsg::Register {
        account: account(),
        password: password_of("right password"),
        proof: 0,
    })
    .await;
    let ServerMsg::Session { .. } = c.recv().await else {
        panic!("no session")
    };
    c.create("Alfa").await;
    let list = c.create("Bravo").await;
    let alfa = list.iter().find(|s| s.name == name("Alfa")).unwrap().id;

    // The typed name must match.
    c.send(&ClientMsg::DeleteCharacter {
        character: alfa,
        name: name("Bravo"),
    })
    .await;
    assert_eq!(c.error_within(10).await, Some(ErrorCode::InvalidName));
    c.send(&ClientMsg::DeleteCharacter {
        character: alfa,
        name: name("alfa"),
    })
    .await;
    let ServerMsg::Characters { characters } = c.recv().await else {
        panic!("no list after the delete")
    };
    assert_eq!(characters.len(), 1);
    assert_eq!(characters[0].name, name("Bravo"));
    // Gone for good: entering it is refused, the name is free again.
    c.send(&ClientMsg::Enter { character: alfa }).await;
    assert_eq!(c.error_within(10).await, Some(ErrorCode::NoSuchCharacter));
    assert_eq!(c.create("ALFA").await.len(), 2);

    // An admin announces a restart in 12 s from the game.
    let r = db(&data).await;
    let id = r.account(&account()).await.unwrap().unwrap().id;
    r.set_role(id, Role::Admin, "test", "tester", 0).await.unwrap();
    drop(r);
    let mut a = Client::connect(port, &cert).await;
    let list = a.login("right password").await;
    let bravo = list.iter().find(|s| s.name == name("Bravo")).unwrap().id;
    a.send(&ClientMsg::Enter { character: bravo }).await;
    loop {
        if let ServerMsg::Welcome { .. } = a.recv().await {
            break;
        }
    }
    a.send(&ClientMsg::Admin(AdminCommand::Maintenance { secs: 12 }))
        .await;
    let first = a.maintenance_within(10).await.expect("announced when scheduled");
    assert!((10..=12).contains(&first), "{first}");
    let ten = a.maintenance_within(10).await.expect("announced at 10 s");
    assert!(ten <= 10, "{ten}");

    // At zero the server saves everyone and stops by itself.
    let mut stopped = None;
    for _ in 0..200 {
        if let Some(status) = server.child.try_wait().unwrap() {
            stopped = Some(status);
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let status = stopped.expect("the server stops at the end of the countdown");
    assert!(status.success(), "{status}");

    // The hourly backup (every 2 s here) left checked copies.
    let copies: Vec<PathBuf> = std::fs::read_dir(data.join("backup"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert!(!copies.is_empty(), "no backup written");
    for copy in &copies {
        let (accounts, _) = Repository::verify_backup(&copy.display().to_string()).unwrap();
        assert_eq!(accounts, 1);
    }
    let r = db(&data).await;
    let saved = r
        .characters(r.account(&account()).await.unwrap().unwrap().id)
        .await
        .unwrap();
    assert_eq!(saved.len(), 2, "the deleted one stays hidden after the restart");
    let _ = std::fs::remove_dir_all(data);
}

impl Client {
    /// Registers `account` with one character.
    async fn register(port: u16, cert: &Path, account: &str, character: &str) {
        let mut c = Client::connect(port, cert).await;
        c.send(&ClientMsg::Register {
            account: AccountName::try_from(account).unwrap(),
            password: password_of("right password"),
            proof: 0,
        })
        .await;
        let ServerMsg::Session { .. } = c.recv().await else {
            panic!("no session")
        };
        assert_eq!(c.create(character).await.len(), 1);
    }

    /// Logs into `account`, enters its character; returns the entity id.
    async fn play(port: u16, cert: &Path, account: &str) -> (Client, base::EntityId) {
        let mut c = Client::connect(port, cert).await;
        c.send(&ClientMsg::Login {
            account: AccountName::try_from(account).unwrap(),
            password: password_of("right password"),
            device: None,
            proof: 0,
        })
        .await;
        let ServerMsg::Session { characters, .. } = c.recv().await else {
            panic!("no session")
        };
        c.send(&ClientMsg::Enter {
            character: characters[0].id,
        })
        .await;
        loop {
            if let ServerMsg::Welcome { your_id, .. } = c.recv().await {
                return (c, your_id);
            }
        }
    }

    async fn until<T>(&mut self, mut pick: impl FnMut(ServerMsg) -> Option<T>) -> T {
        loop {
            if let Some(t) = pick(self.recv().await) {
                return t;
            }
        }
    }
}

/// The receiver of a trade logs out right after it and the
/// server is killed before any checkpoint. The leave saves both sides in one
/// transaction, so the piece ends with the receiver (never refused on its
/// side, never two holders) and the coins add up.
#[tokio::test(flavor = "multi_thread")]
async fn a_trade_then_a_logout_keeps_both_sides() {
    let data: PathBuf = std::env::temp_dir().join(format!("server-trade-leave-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let cert = data.join("tls/cert.pem");
    let port = free_port();
    let server = Server::start(&data, port);
    wait_ready(port).await;
    Client::register(port, &cert, "giver", "Giver").await;
    Client::register(port, &cert, "taker", "Taker").await;
    drop(server);

    // The giver holds a sword with identity and 500 copper; both stand at
    // the crossroads camp.
    let r = db(&data).await;
    let (item, _) = r.reserve_item_ids(1).await.unwrap();
    let (giver, _, _) = r.character_by_name(&name("Giver")).await.unwrap().unwrap();
    let (taker, _, _) = r.character_by_name(&name("Taker")).await.unwrap().unwrap();
    let mut gs = r.load_character(giver).await.unwrap().unwrap().sheet;
    gs.coins = 500;
    gs.items.push(persistence::SavedItem {
        id: item,
        item: "iron_sword".into(),
        equipped: None,
        slot: 0,
        hidden: vec![],
        appraised: false,
        durability: (0, 0),
        crafter: None,
        rarity: None,
        mastery: 0,
        power: None,
    });
    let ts = r.load_character(taker).await.unwrap().unwrap().sheet;
    let at = |x, sheet, id| persistence::CharacterSave {
        id,
        zone: "dev_field".into(),
        pos: base::TilePos::new(x, 30),
        sheet,
    };
    let refused = r
        .save_zone_batch(vec![at(28, gs, giver), at(29, ts, taker)], 1)
        .await
        .unwrap();
    assert!(refused.is_empty());
    drop(r);

    let mut server = Server::start(&data, port);
    wait_ready(port).await;
    let (mut g, gid) = Client::play(port, &cert, "giver").await;
    let (mut t, _) = Client::play(port, &cert, "taker").await;
    t.send(&ClientMsg::TradeRequest { target: gid }).await;
    let from = g
        .until(|m| match m {
            ServerMsg::Invite {
                from,
                kind: protocol::InviteKind::Trade,
            } => Some(from),
            _ => None,
        })
        .await;
    g.send(&ClientMsg::TradeRequest { target: from }).await;
    g.send(&ClientMsg::TradeOffer {
        items: vec![(0, 1)],
        coins: 200,
    })
    .await;
    t.until(|m| match m {
        ServerMsg::Trade(v) if !v.theirs.is_empty() && v.theirs_coins == 200 => Some(()),
        _ => None,
    })
    .await;
    g.send(&ClientMsg::TradeConfirm).await;
    t.send(&ClientMsg::TradeConfirm).await;
    t.until(|m| matches!(m, ServerMsg::TradeEnd).then_some(())).await;
    // The taker leaves at once; the server dies before a checkpoint.
    drop(t);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    server.child.kill().unwrap();
    server.child.wait().unwrap();
    drop(g);

    let r = db(&data).await;
    let sheets = r.all_sheets().await.unwrap();
    let holders: Vec<_> = sheets
        .iter()
        .filter(|(_, _, s)| s.items.iter().any(|i| i.id == item))
        .map(|(id, _, _)| *id)
        .collect();
    assert_eq!(holders, vec![taker], "the piece moved once, to the taker");
    assert_eq!(r.item_owner(item).await.unwrap(), Some(taker));
    let coins = |id| sheets.iter().find(|(c, _, _)| *c == id).unwrap().2.coins;
    assert_eq!((coins(giver), coins(taker)), (300, 200));
    let _ = std::fs::remove_dir_all(data);
}
