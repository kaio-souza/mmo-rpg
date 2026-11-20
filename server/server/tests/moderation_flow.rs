//! Phase 2 moderation done criterion: a temporary password forces a change,
//! and an account sanction refuses the login with the reason. Runs the real
//! binary over TLS and changes the database while it is online.
#![allow(clippy::unwrap_used, clippy::expect_used)] // test helpers

use base::{AccountName, Arg, CharacterName, Locale};
use persistence::{NewSanction, Repository, SanctionKind};
use protocol::{ClientMsg, ErrorCode, MAX_SERVER_FRAME, PROTOCOL_VERSION, Password, ServerMsg};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
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

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

async fn db(data: &Path) -> Repository {
    Repository::open(&format!("sqlite:{}", data.join("game.db").display())).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn temporary_password_and_sanctions() {
    let data: PathBuf = std::env::temp_dir().join(format!("server-mod-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let cert = data.join("tls/cert.pem");
    let port = free_port();

    let server = Server::start(&data, port);
    wait_ready(port).await;

    // Register and create a character.
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
    c.send(&ClientMsg::CreateCharacter {
        look: base::Look::default(),
        name: CharacterName::try_from("Tester").unwrap(),
    })
    .await;
    let ServerMsg::Characters { characters } = c.recv().await else {
        panic!("no character list")
    };
    c.send(&ClientMsg::Enter {
        character: characters[0].id,
    })
    .await;
    assert!(matches!(c.recv().await, ServerMsg::Welcome { .. }));
    drop(c);
    tokio::time::sleep(Duration::from_millis(500)).await;

    // An admin resets the password; the temporary one must be changed.
    let repo = db(&data).await;
    let row = repo.account(&account()).await.unwrap().unwrap();
    repo.reset_password(
        row.id,
        persistence::hash_password("temp password").unwrap(),
        "cli:test",
        "tester",
        now(),
    )
    .await
    .unwrap();

    let mut c = Client::connect(port, &cert).await;
    let characters = c.login("temp password").await;
    c.send(&ClientMsg::Enter {
        character: characters[0].id,
    })
    .await;
    assert!(matches!(
        c.recv().await,
        ServerMsg::Error {
            code: ErrorCode::PasswordChangeRequired,
            ..
        }
    ));
    c.send(&ClientMsg::CreateCharacter {
        look: base::Look::default(),
        name: CharacterName::try_from("Other").unwrap(),
    })
    .await;
    assert!(matches!(
        c.recv().await,
        ServerMsg::Error {
            code: ErrorCode::PasswordChangeRequired,
            ..
        }
    ));
    c.send(&ClientMsg::ChangePassword {
        old: password_of("wrong password"),
        new: password_of("new password"),
    })
    .await;
    assert!(matches!(
        c.recv().await,
        ServerMsg::Error {
            code: ErrorCode::WrongCredentials,
            ..
        }
    ));
    c.send(&ClientMsg::ChangePassword {
        old: password_of("temp password"),
        new: password_of("new password"),
    })
    .await;
    assert!(matches!(c.recv().await, ServerMsg::Session { .. }));
    c.send(&ClientMsg::Enter {
        character: characters[0].id,
    })
    .await;
    assert!(matches!(c.recv().await, ServerMsg::Welcome { .. }));
    drop(c);
    tokio::time::sleep(Duration::from_millis(500)).await;

    // The old password no longer works; the new one does.
    let mut c = Client::connect(port, &cert).await;
    c.send(&ClientMsg::Login {
        account: account(),
        password: password_of("temp password"),
        device: None,
        proof: 0,
    })
    .await;
    assert!(matches!(
        c.recv().await,
        ServerMsg::Error {
            code: ErrorCode::WrongCredentials,
            ..
        }
    ));
    c.send(&ClientMsg::Login {
        account: account(),
        password: password_of("new password"),
        device: None,
        proof: 0,
    })
    .await;
    assert!(matches!(c.recv().await, ServerMsg::Session { .. }));
    drop(c);

    // A ban refuses the login and carries the translated reason key.
    let ban = repo
        .add_sanction(
            NewSanction {
                kind: SanctionKind::Ban,
                account: Some(row.id),
                ip: None,
                target: "tester".into(),
                reason: "moderation.reason.cheating".into(),
                note: String::new(),
                expires_at: None,
                created_by: "cli:test".into(),
            },
            now(),
        )
        .await
        .unwrap();
    let mut c = Client::connect(port, &cert).await;
    c.send(&ClientMsg::Login {
        account: account(),
        password: password_of("new password"),
        device: None,
        proof: 0,
    })
    .await;
    match c.recv().await {
        ServerMsg::Error { code, args } => {
            assert_eq!(code, ErrorCode::AccountBanned);
            assert!(matches!(args.first(), Some(Arg::Ref(_))));
        }
        other => panic!("expected a ban, got {other:?}"),
    }
    drop(c);

    // Revoking gives the account back.
    assert!(repo.revoke_sanction(ban, "cli:test", now()).await.unwrap());
    let mut c = Client::connect(port, &cert).await;
    c.login("new password").await;
    drop(c);

    // A suspension carries the seconds left and the reason.
    let suspend = repo
        .add_sanction(
            NewSanction {
                kind: SanctionKind::Suspend,
                account: Some(row.id),
                ip: None,
                target: "tester".into(),
                reason: "moderation.reason.botting".into(),
                note: String::new(),
                expires_at: Some(now() + 60),
                created_by: "cli:test".into(),
            },
            now(),
        )
        .await
        .unwrap();
    let mut c = Client::connect(port, &cert).await;
    c.send(&ClientMsg::Login {
        account: account(),
        password: password_of("new password"),
        device: None,
        proof: 0,
    })
    .await;
    match c.recv().await {
        ServerMsg::Error { code, args } => {
            assert_eq!(code, ErrorCode::AccountSuspended);
            assert!(matches!(args.first(), Some(Arg::Int(s)) if *s > 0));
            assert!(matches!(args.get(1), Some(Arg::Ref(_))));
        }
        other => panic!("expected a suspension, got {other:?}"),
    }
    drop(c);
    assert!(repo.revoke_sanction(suspend, "cli:test", now()).await.unwrap());

    // An IP ban refuses every login from that IP.
    let ip_ban = repo
        .add_sanction(
            NewSanction {
                kind: SanctionKind::IpBan,
                account: None,
                ip: Some("127.0.0.1".into()),
                target: "127.0.0.1".into(),
                reason: "moderation.reason.spam".into(),
                note: String::new(),
                expires_at: Some(now() + 60),
                created_by: "cli:test".into(),
            },
            now(),
        )
        .await
        .unwrap();
    let mut c = Client::connect(port, &cert).await;
    c.send(&ClientMsg::Login {
        account: account(),
        password: password_of("new password"),
        device: None,
        proof: 0,
    })
    .await;
    assert!(matches!(
        c.recv().await,
        ServerMsg::Error {
            code: ErrorCode::IpBlocked,
            ..
        }
    ));
    drop(c);
    assert!(repo.revoke_sanction(ip_ban, "cli:test", now()).await.unwrap());

    // A suspension made from the terminal reaches a player already in the
    // game at the next batch save (30 s).
    let mut c = Client::connect(port, &cert).await;
    let characters = c.login("new password").await;
    c.send(&ClientMsg::Enter {
        character: characters[0].id,
    })
    .await;
    assert!(matches!(c.recv().await, ServerMsg::Welcome { .. }));
    repo.add_sanction(
        NewSanction {
            kind: SanctionKind::Suspend,
            account: Some(row.id),
            ip: None,
            target: "tester".into(),
            reason: "moderation.reason.botting".into(),
            note: String::new(),
            expires_at: Some(now() + 600),
            created_by: "cli:test".into(),
        },
        now(),
    )
    .await
    .unwrap();
    assert_eq!(c.error_within(45).await, Some(ErrorCode::AccountSuspended));
    drop(c);

    drop(repo);
    drop(server);
    let _ = std::fs::remove_dir_all(&data);
}
