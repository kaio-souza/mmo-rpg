//! Phase 2 done criterion: a character comes back where it stopped after the
//! server restarts. Runs the real binary over TLS with a fresh data folder.
#![allow(clippy::unwrap_used, clippy::expect_used)] // test helpers

use base::{AccountName, CharacterName, Locale, TilePos};
use protocol::{ClientMsg, ErrorCode, MAX_SERVER_FRAME, PROTOCOL_VERSION, Password, ServerMsg, SessionToken};
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

fn password(s: &str) -> Password {
    Password::try_from(s.to_owned()).unwrap()
}

/// Logs in and enters with the first character; returns the spawn position.
async fn enter(c: &mut Client) -> TilePos {
    c.send(&ClientMsg::Login {
        account: account(),
        password: password("right password"),
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
        match c.recv().await {
            ServerMsg::Welcome { pos, .. } => return pos,
            ServerMsg::QueuePosition { .. } | ServerMsg::Snapshot { .. } | ServerMsg::Diff(_) => {}
            other => panic!("unexpected {other:?}"),
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn character_returns_where_it_stopped_after_restart() {
    let data: PathBuf = std::env::temp_dir().join(format!("server-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let cert = data.join("tls/cert.pem");
    let port = free_port();

    let server = Server::start(&data, port);
    wait_ready(port).await;

    // Register, create a character, enter.
    let mut c = Client::connect(port, &cert).await;
    c.send(&ClientMsg::Register {
        account: account(),
        password: password("right password"),
        proof: 0,
    })
    .await;
    let ServerMsg::Session {
        token, characters, ..
    } = c.recv().await
    else {
        panic!("no session")
    };
    assert!(characters.is_empty());
    c.send(&ClientMsg::CreateCharacter {
        look: base::Look::default(),
        name: CharacterName::try_from("Tester").unwrap(),
    })
    .await;
    let ServerMsg::Characters { characters } = c.recv().await else {
        panic!("no character list")
    };
    assert_eq!(characters.len(), 1);
    c.send(&ClientMsg::Enter {
        character: characters[0].id,
    })
    .await;
    let start = loop {
        if let ServerMsg::Welcome { pos, your_id, .. } = c.recv().await {
            break (pos, your_id);
        }
    };

    // Walk three tiles and wait until the server says we are there.
    let target = TilePos::new(start.0.x + 3, start.0.y);
    c.send(&ClientMsg::Move { destination: target }).await;
    let mut at = start.0;
    while at != target {
        if let ServerMsg::Diff(d) = c.recv().await {
            for (id, pos, _) in d.moved {
                if id == start.1 {
                    at = pos;
                }
            }
        }
    }

    // A second login of the same account cannot enter at the same time.
    let mut twin = Client::connect(port, &cert).await;
    twin.send(&ClientMsg::Resume { token }).await;
    let ServerMsg::Session { characters, .. } = twin.recv().await else {
        panic!("resume failed")
    };
    twin.send(&ClientMsg::Enter {
        character: characters[0].id,
    })
    .await;
    assert!(matches!(
        twin.recv().await,
        ServerMsg::Error {
            code: ErrorCode::AlreadyOnline,
            ..
        }
    ));
    drop(twin);

    // Leaving saves; then the server dies without warning.
    drop(c);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    drop(server);

    let server = Server::start(&data, port);
    wait_ready(port).await;

    // Wrong password, then the used-up token, then the right password.
    let mut c = Client::connect(port, &cert).await;
    c.send(&ClientMsg::Login {
        account: account(),
        password: password("wrong password"),
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
    c.send(&ClientMsg::Resume {
        token: SessionToken([0; 32]),
    })
    .await;
    assert!(matches!(
        c.recv().await,
        ServerMsg::Error {
            code: ErrorCode::SessionExpired,
            ..
        }
    ));
    let back = enter(&mut c).await;
    assert_eq!(back, target, "character returns where it stopped");

    drop(c);
    drop(server);
    let _ = std::fs::remove_dir_all(&data);
}

/// A published server (no `--dev`) never takes the development login, even
/// from a client on the same machine: `DevLogin` is refused, whatever the
/// client binary allows on its command line.
#[tokio::test(flavor = "multi_thread")]
async fn dev_login_is_refused_without_dev() {
    let data: PathBuf = std::env::temp_dir().join(format!("server-devlogin-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let port = free_port();
    let server = Server::start(&data, port);
    wait_ready(port).await;

    let mut c = Client::connect(port, &data.join("tls/cert.pem")).await;
    c.send(&ClientMsg::DevLogin {
        name: CharacterName::try_from("Intruder").unwrap(),
    })
    .await;
    assert!(matches!(
        c.recv().await,
        ServerMsg::Error {
            code: ErrorCode::NotAllowed,
            ..
        }
    ));

    drop(c);
    drop(server);
    let _ = std::fs::remove_dir_all(&data);
}

/// `--dev` (plain TCP, no passwords) refuses to start on an address other
/// machines can reach.
#[test]
fn dev_refuses_a_public_address() {
    let data: PathBuf = std::env::temp_dir().join(format!("server-devpublic-{}", std::process::id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_server"))
        .arg("--dev")
        .args(["--address", &format!("0.0.0.0:{}", free_port())])
        .arg("--data-dir")
        .arg(&data)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut status = None;
    for _ in 0..100 {
        status = child.try_wait().unwrap();
        if status.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if status.is_none() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let _ = std::fs::remove_dir_all(&data);
    assert!(
        status.is_some_and(|s| !s.success()),
        "--dev must not start on 0.0.0.0"
    );
}
