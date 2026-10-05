//! Phase 5: chat (local, whisper), mute, report and in-game admin commands,
//! through the real binary over TLS.

use base::{AccountName, CharacterName, ChatText, Locale, Look, TextId};
use persistence::{Repository, Role};
use protocol::{
    AdminCommand, ChatChannel, ClientMsg, ErrorCode, MAX_SERVER_FRAME, PROTOCOL_VERSION, Password, ReasonId,
    ServerMsg,
};
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

    /// Logs in; returns the characters.
    async fn login(&mut self, name: &str) -> Vec<protocol::CharacterSummary> {
        self.send(&ClientMsg::Login {
            account: AccountName::try_from(name).unwrap(),
            password: password_of("right password"),
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

fn password_of(s: &str) -> Password {
    Password::try_from(s.to_owned()).unwrap()
}

async fn db(data: &Path) -> Repository {
    Repository::open(&format!("sqlite:{}", data.join("game.db").display())).unwrap()
}

fn name(s: &str) -> CharacterName {
    CharacterName::try_from(s).unwrap()
}

fn key(k: &str) -> TextId {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara/content");
    let mut raw = defs::load(&content).unwrap();
    // The test server runs with `--allow-missing-text`: do the same here.
    let _ = raw.fill_missing_text();
    raw.validate().unwrap().keys().id(k).unwrap()
}

impl Client {
    /// Registers `account`, creates `character` with `look` and checks the list.
    async fn register(port: u16, cert: &Path, account: &str, character: &str, look: Look) {
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
        c.send(&ClientMsg::CreateCharacter {
            name: name(character),
            look,
        })
        .await;
        let ServerMsg::Characters { characters } = c.recv().await else {
            panic!("no list")
        };
        assert_eq!((characters[0].look, characters[0].level), (look, 1));
    }

    /// Logs in and enters with the first character.
    async fn play(port: u16, cert: &Path, account: &str) -> Client {
        let mut c = Client::connect(port, cert).await;
        let list = c.login(account).await;
        c.send(&ClientMsg::Enter {
            character: list[0].id,
        })
        .await;
        loop {
            if let ServerMsg::Welcome { .. } = c.recv().await {
                return c;
            }
        }
    }

    /// Skips game messages until one `pick` accepts.
    async fn until<T>(&mut self, mut pick: impl FnMut(ServerMsg) -> Option<T>) -> T {
        loop {
            if let Some(t) = pick(self.recv().await) {
                return t;
            }
        }
    }

    async fn chat(&mut self, channel: ChatChannel, text: &str) {
        self.send(&ClientMsg::Chat {
            channel,
            text: ChatText::try_from(text).unwrap(),
        })
        .await;
    }

    async fn system(&mut self) -> TextId {
        self.until(|m| match m {
            ServerMsg::System { text, .. } => Some(text.id),
            _ => None,
        })
        .await
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn chat_mute_report_and_admin_commands() {
    let data: PathBuf = std::env::temp_dir().join(format!("server-chat-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let cert = data.join("tls/cert.pem");
    let port = free_port();
    let _server = Server::start(&data, port);
    wait_ready(port).await;

    let look = Look {
        body: 1,
        skin: 4,
        hair_style: 7,
        hair_color: 2,
    };
    Client::register(port, &cert, "boss", "Boss", look).await;
    Client::register(port, &cert, "ana", "Ana", Look::default()).await;
    // A look outside the lists is refused.
    let mut odd = Client::connect(port, &cert).await;
    odd.login("ana").await;
    odd.send(&ClientMsg::CreateCharacter {
        name: name("Odd"),
        look: Look {
            body: 200,
            ..Look::default()
        },
    })
    .await;
    assert_eq!(odd.error_within(10).await, Some(ErrorCode::InvalidMessage));
    drop(odd);
    let repo = db(&data).await;
    let (_, boss_account, _) = repo.character_by_name(&name("Boss")).await.unwrap().unwrap();
    repo.set_role(boss_account, Role::Admin, "cli:test", "boss", 1)
        .await
        .unwrap();

    let mut boss = Client::play(port, &cert, "boss").await;
    let mut ana = Client::play(port, &cert, "ana").await;

    // Ana sees Boss with the look chosen at creation.
    let seen = ana
        .until(|m| {
            let list = match m {
                ServerMsg::Snapshot { entities } => entities,
                ServerMsg::Diff(d) => d.entered,
                _ => return None,
            };
            list.into_iter()
                .find(|e| e.name == protocol::EntityName::Player(name("Boss")))
        })
        .await;
    assert_eq!(seen.look, look);

    // Local chat reaches both; a whisper reaches the receiver and the sender.
    boss.chat(ChatChannel::Local, "olá a todos").await;
    let heard = |m| match m {
        ServerMsg::ChatMessage { channel, from, text } => Some((channel, from, text.to_string())),
        _ => None,
    };
    assert_eq!(
        ana.until(heard).await,
        (ChatChannel::Local, name("Boss"), "olá a todos".into())
    );
    ana.chat(ChatChannel::Whisper(name("boss")), "你好").await;
    assert_eq!(boss.until(heard).await.0, ChatChannel::Local);
    assert_eq!(boss.until(heard).await.2, "你好");
    assert_eq!(ana.until(heard).await.1, name("Ana"));
    ana.chat(ChatChannel::Whisper(name("Nobody")), "hi").await;
    assert_eq!(ana.error_within(10).await, Some(ErrorCode::PlayerOffline));

    // A player cannot use admin commands.
    ana.send(&ClientMsg::Admin(AdminCommand::Kick { name: name("Boss") }))
        .await;
    assert_eq!(ana.error_within(10).await, Some(ErrorCode::NotAllowed));

    // The admin: /who, a bad reason, then /mute.
    boss.send(&ClientMsg::Admin(AdminCommand::Who { name: name("ana") }))
        .await;
    assert_eq!(boss.system().await, key("admin.who"));
    let mute = |reason: &str| {
        ClientMsg::Admin(AdminCommand::Mute {
            name: name("Ana"),
            secs: 600,
            reason: ReasonId::try_from(reason).unwrap(),
        })
    };
    boss.send(&mute("nonsense")).await;
    assert_eq!(boss.system().await, key("admin.bad_reason"));
    boss.send(&mute("spam")).await;
    assert_eq!(boss.system().await, key("admin.muted"));
    // The mute applies at once, in the same connection.
    let mut muted = false;
    for _ in 0..50 {
        ana.chat(ChatChannel::Local, "still here?").await;
        match ana.error_within(5).await {
            Some(ErrorCode::Muted) => {
                muted = true;
                break;
            }
            Some(ErrorCode::TooFast) | None => tokio::time::sleep(Duration::from_millis(600)).await,
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(muted, "mute not applied");

    // A report keeps what Ana could see (after the per-second chat limit).
    tokio::time::sleep(Duration::from_millis(1100)).await;
    ana.send(&ClientMsg::ReportChat { name: name("Boss") }).await;
    assert_eq!(ana.system().await, key("chat.reported"));
    let reports = repo.chat_reports(0).await.unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(
        (reports[0].reporter.as_str(), reports[0].target.as_str()),
        ("Ana", "Boss")
    );
    assert!(reports[0].lines.iter().any(|l| l.text == "olá a todos"));
    assert!(
        reports[0]
            .lines
            .iter()
            .any(|l| l.channel == "whisper:boss" && l.text == "你好")
    );
    let log = repo.admin_log(None, None, 50).await.unwrap();
    assert!(log.iter().any(|l| l.action == "who" && l.actor == "boss"));
    assert!(log.iter().any(|l| l.action == "mute" && l.actor == "boss"));

    // /kick drops Ana's connection.
    boss.send(&ClientMsg::Admin(AdminCommand::Kick { name: name("Ana") }))
        .await;
    assert_eq!(boss.system().await, key("admin.kicked"));
    assert_eq!(
        ana.error_within(10).await,
        None,
        "connection closed without an error"
    );
    let _ = std::fs::remove_dir_all(&data);
}
