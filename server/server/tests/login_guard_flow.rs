//! Against the real binary: parallel wrong passwords cannot slip
//! past the wait, the account's own wait stops guesses from many IPs but
//! not the owner's device, and the shield asks for a proof of work.
//! Different source addresses come from the loopback range (127.0.0.x).

use base::{AccountName, Locale};
use protocol::{ClientMsg, DeviceToken, ErrorCode, MAX_SERVER_FRAME, PROTOCOL_VERSION, Password, ServerMsg};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpSocket, TcpStream};
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

struct Server {
    child: Child,
}

impl Server {
    fn start(data: &Path, port: u16, shield_per_min: u32) -> Server {
        let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara/content");
        let child = Command::new(env!("CARGO_BIN_EXE_server"))
            .args(["--address", &format!("127.0.0.1:{port}")])
            .arg("--data-dir")
            .arg(data)
            .arg("--content")
            .arg(content)
            .args(["--zone", "dev_field", "--threads", "2"])
            .args(["--shield-per-min", &shield_per_min.to_string()])
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
    challenge: ([u8; 16], u8),
}

impl Client {
    /// Connects from `127.0.0.<from>` and reads the first `Challenge`.
    async fn connect(port: u16, cert: &Path, from: u8) -> Client {
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
        let socket = TcpSocket::new_v4().unwrap();
        socket
            .bind(SocketAddr::new(IpAddr::from([127, 0, 0, from]), 0))
            .unwrap();
        let tcp = socket
            .connect(SocketAddr::new(IpAddr::from([127, 0, 0, 1]), port))
            .await
            .unwrap();
        let s = TlsConnector::from(Arc::new(cfg))
            .connect(ServerName::try_from("localhost").unwrap(), tcp)
            .await
            .unwrap();
        let mut c = Client {
            s,
            buf: Vec::new(),
            challenge: ([0; 16], 0),
        };
        c.send(&ClientMsg::Hello {
            version: PROTOCOL_VERSION,
            locale: Locale::from_static("en"),
        })
        .await;
        match c.next().await {
            Some(ServerMsg::Challenge { seed, bits }) => c.challenge = (seed, bits),
            other => panic!("expected a challenge, got {other:?}"),
        }
        c
    }

    async fn send(&mut self, m: &ClientMsg) {
        let mut b = Vec::new();
        protocol::encode(m, &mut b).unwrap();
        self.s.write_all(&b).await.unwrap();
        self.s.flush().await.unwrap();
    }

    /// Next message that is not a chunk or a pong; `None` once closed.
    async fn next(&mut self) -> Option<ServerMsg> {
        loop {
            if let Some((m, used)) = protocol::decode_frame::<ServerMsg>(&self.buf, MAX_SERVER_FRAME).unwrap()
            {
                self.buf.drain(..used);
                match m {
                    ServerMsg::Chunk { .. } | ServerMsg::Pong { .. } | ServerMsg::Appearances(_) => continue,
                    m => return Some(m),
                }
            }
            let mut chunk = vec![0u8; 64 * 1024];
            let n = tokio::time::timeout(Duration::from_secs(30), self.s.read(&mut chunk))
                .await
                .expect("server answer")
                .unwrap_or(0);
            if n == 0 {
                return None;
            }
            self.buf.extend_from_slice(&chunk[..n]);
        }
    }

    /// The answer to a `Login` or `Register`, keeping the new challenge.
    async fn answer(&mut self) -> ServerMsg {
        loop {
            match self.next().await.expect("server closed the connection") {
                ServerMsg::Challenge { seed, bits } => self.challenge = (seed, bits),
                m => return m,
            }
        }
    }

    fn proof(&self, account: &str) -> u64 {
        protocol::solve_proof(&self.challenge.0, account, self.challenge.1)
    }

    async fn login(&mut self, account: &str, password: &str, device: Option<DeviceToken>) -> ServerMsg {
        let proof = self.proof(account);
        self.send(&ClientMsg::Login {
            account: AccountName::try_from(account).unwrap(),
            password: Password::try_from(password.to_owned()).unwrap(),
            device,
            proof,
        })
        .await;
        self.answer().await
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

fn data_dir(name: &str) -> PathBuf {
    let data = std::env::temp_dir().join(format!("server-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    data
}

fn code(m: &ServerMsg) -> Option<ErrorCode> {
    match m {
        ServerMsg::Error { code, .. } => Some(*code),
        _ => None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn guesses_wait_and_the_owner_device_still_enters() {
    let data = data_dir("guard");
    let cert = data.join("tls/cert.pem");
    let port = free_port();
    let server = Server::start(&data, port, 1000);
    wait_ready(port).await;

    // A password with the account name is refused; then a good one.
    let mut c = Client::connect(port, &cert, 1).await;
    let mut device = None;
    for (password, want) in [("victim2026!", false), ("right password", true)] {
        let proof = c.proof("victim");
        c.send(&ClientMsg::Register {
            account: AccountName::try_from("victim").unwrap(),
            password: Password::try_from(password.to_owned()).unwrap(),
            proof,
        })
        .await;
        match c.answer().await {
            ServerMsg::Session {
                device: Some(d),
                failed_logins: 0,
                ..
            } if want => device = Some(d),
            m if !want => assert_eq!(code(&m), Some(ErrorCode::PasswordHasName)),
            m => panic!("{m:?}"),
        }
    }
    let device = device.unwrap();
    drop(c);

    // Eight wrong passwords at once from one IP: only the free four and
    // the one that starts the wait reach the password check.
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let cert = cert.clone();
        tasks.push(tokio::spawn(async move {
            let mut c = Client::connect(port, &cert, 2).await;
            code(&c.login("victim", "wrong password", None).await)
        }));
    }
    let mut wrong = 0;
    for t in tasks {
        match t.await.unwrap() {
            Some(ErrorCode::WrongCredentials) => wrong += 1,
            Some(ErrorCode::TooManyAttempts) => {}
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(wrong, 5);

    // More guesses from other IPs fill the account's own ten free tries.
    let mut c = Client::connect(port, &cert, 3).await;
    for _ in 0..4 {
        let m = c.login("victim", "wrong password", None).await;
        assert_eq!(code(&m), Some(ErrorCode::WrongCredentials));
    }
    let mut c = Client::connect(port, &cert, 4).await;
    for _ in 0..2 {
        let m = c.login("victim", "wrong password", None).await;
        assert_eq!(code(&m), Some(ErrorCode::WrongCredentials));
    }
    let m = c.login("victim", "wrong password", None).await;
    assert_eq!(code(&m), Some(ErrorCode::TooManyAttempts), "account waits");

    // Each guess after a wait doubles it: 2 s, 4 s, then 8 s.
    for (from, wait) in [(6, 2500), (7, 4500)] {
        tokio::time::sleep(Duration::from_millis(wait)).await;
        let mut c = Client::connect(port, &cert, from).await;
        let m = c.login("victim", "wrong password", None).await;
        assert_eq!(code(&m), Some(ErrorCode::WrongCredentials));
    }
    // The waits are saved within a second and survive a restart.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    drop(server);
    let server = Server::start(&data, port, 1000);
    wait_ready(port).await;

    // Even the right password waits without a device...
    let mut owner = Client::connect(port, &cert, 5).await;
    let m = owner.login("victim", "right password", None).await;
    assert_eq!(code(&m), Some(ErrorCode::TooManyAttempts));
    // ...but the account's device enters, hears of the guesses and gets a
    // new token.
    match owner
        .login("victim", "right password", Some(device.clone()))
        .await
    {
        ServerMsg::Session {
            device: Some(new),
            failed_logins: 13,
            ..
        } => assert_ne!(new, device),
        m => panic!("{m:?}"),
    }
    drop(owner);
    drop(server);
    let _ = std::fs::remove_dir_all(&data);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_shield_asks_for_a_proof_of_work() {
    let data = data_dir("shield");
    let cert = data.join("tls/cert.pem");
    let port = free_port();
    let server = Server::start(&data, port, 0);
    wait_ready(port).await;

    let mut c = Client::connect(port, &cert, 1).await;
    let (seed, bits) = c.challenge;
    assert_eq!(bits, login_bits());
    // A nonce that does not pass closes the connection.
    let bad = (0..)
        .find(|n| !protocol::proof_ok(&seed, "worker", *n, bits))
        .unwrap();
    c.send(&ClientMsg::Register {
        account: AccountName::try_from("worker").unwrap(),
        password: Password::try_from("right password".to_owned()).unwrap(),
        proof: bad,
    })
    .await;
    loop {
        match c.next().await {
            None => break,
            Some(ServerMsg::Error { code, .. }) => assert_eq!(code, ErrorCode::InvalidMessage),
            Some(m) => panic!("{m:?}"),
        }
    }
    // The solved one opens the account.
    let mut c = Client::connect(port, &cert, 1).await;
    let proof = c.proof("worker");
    c.send(&ClientMsg::Register {
        account: AccountName::try_from("worker").unwrap(),
        password: Password::try_from("right password".to_owned()).unwrap(),
        proof,
    })
    .await;
    assert!(matches!(c.answer().await, ServerMsg::Session { .. }));
    drop(server);
    let _ = std::fs::remove_dir_all(&data);
}

/// The server's `SHIELD_BITS`.
fn login_bits() -> u8 {
    18
}
