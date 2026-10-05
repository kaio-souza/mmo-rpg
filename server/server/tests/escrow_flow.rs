//! Rule 4 through the real binary: a parcel waiting in custody
//! reaches the backpack when its character enters the game, with the
//! crafter's signature on the piece, and the sheet holding it is on disk
//! before the next checkpoint.

use base::{CharacterName, Locale};
use persistence::{EscrowParcel, Repository, SavedItem, SavedSheet, SavedStack};
use protocol::{ClientMsg, MAX_SERVER_FRAME, PROTOCOL_VERSION, ServerMsg};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

const PIECE: u64 = 900_001;

fn signed_knife() -> SavedItem {
    SavedItem {
        id: PIECE,
        item: "hunting_knife".into(),
        equipped: None,
        slot: 0,
        hidden: vec![(0, 2)],
        appraised: true,
        durability: (17, 30),
        crafter: Some("Forgemaster".into()),
        rarity: Some(2),
        mastery: 125,
        power: None,
    }
}

async fn send(s: &mut TcpStream, m: &ClientMsg) {
    let mut b = Vec::new();
    protocol::encode(m, &mut b).unwrap();
    s.write_all(&b).await.unwrap();
}

/// Reads until `keep` says yes, or fails after 20 s.
async fn until<T>(s: &mut TcpStream, buf: &mut Vec<u8>, mut keep: impl FnMut(&ServerMsg) -> Option<T>) -> T {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        while let Some((m, used)) = protocol::decode_frame::<ServerMsg>(buf, MAX_SERVER_FRAME).unwrap() {
            buf.drain(..used);
            if let Some(v) = keep(&m) {
                return v;
            }
        }
        let mut chunk = vec![0u8; 256 * 1024];
        let n = tokio::time::timeout_at(deadline, s.read(&mut chunk))
            .await
            .expect("no answer in time")
            .unwrap();
        assert!(n > 0, "server closed the connection");
        buf.extend_from_slice(&chunk[..n]);
    }
}

#[tokio::test]
async fn a_parcel_in_custody_arrives_on_entering() {
    let root: PathBuf = std::env::temp_dir().join(format!("server-escrow-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let data = root.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let db = format!("sqlite:{}", data.join("game.db").display());
    let name = CharacterName::try_from("Postie").unwrap();
    let postie = {
        let repo = Repository::open(&db).unwrap();
        let ch = repo
            .dev_character(&name, &SavedSheet::default(), 1)
            .await
            .unwrap();
        let parcel = EscrowParcel {
            coins: 77,
            stacks: vec![SavedStack {
                item: "copper_ore".into(),
                qty: 3,
            }],
            items: vec![signed_knife()],
        };
        repo.restore_escrow(None, ch.id, parcel, 1).await.unwrap();
        assert_eq!(repo.escrows_for(ch.id).await.unwrap().len(), 1);
        ch.id
    };

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
            .arg(&content)
            .args(["--zone", "aurelia", "--threads", "2"])
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
    let mut s = s.expect("server did not start");
    let mut buf = Vec::new();
    send(
        &mut s,
        &ClientMsg::Hello {
            version: PROTOCOL_VERSION,
            locale: Locale::EN,
        },
    )
    .await;
    send(&mut s, &ClientMsg::DevLogin { name: name.clone() }).await;

    // Into the backpack: the coins, the ore and the signed knife.
    let crafter = until(&mut s, &mut buf, |m| match m {
        ServerMsg::Inventory { coins, instances, .. } if *coins == 77 => instances
            .iter()
            .find(|i| i.id == PIECE)
            .map(|i| (i.crafter.clone(), i.durability)),
        _ => None,
    })
    .await;
    assert_eq!(crafter, (CharacterName::try_from("Forgemaster").ok(), (17, 30)));

    // Saved at once: custody is empty and the sheet holds the piece.
    let repo = Repository::open(&db).unwrap();
    let mut saved = None;
    for _ in 0..100 {
        let sheet = repo.load_character(postie).await.unwrap().unwrap().sheet;
        if sheet.coins == 77 {
            saved = Some(sheet);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let sheet = saved.expect("the delivery was saved before the checkpoint");
    assert!(repo.escrows_for(postie).await.unwrap().is_empty());
    let knife = sheet
        .items
        .iter()
        .find(|i| i.id == PIECE)
        .expect("the knife is saved");
    assert_eq!(knife.crafter.as_deref(), Some("Forgemaster"));
    assert_eq!(
        (knife.rarity, knife.mastery, knife.hidden.clone()),
        (Some(2), 125, vec![(0, 2)])
    );
    assert_eq!(repo.item_owner(PIECE).await.unwrap(), Some(postie));
    let _ = std::fs::remove_dir_all(&root);
}
