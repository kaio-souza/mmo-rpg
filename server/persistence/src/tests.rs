use super::*;

fn repo() -> Repository {
    Repository::open("sqlite::memory:").unwrap()
}

fn account(s: &str) -> AccountName {
    AccountName::try_from(s).unwrap()
}

fn name(s: &str) -> CharacterName {
    CharacterName::try_from(s).unwrap()
}

fn sheet(coins: u64) -> SavedSheet {
    SavedSheet {
        coins,
        ..SavedSheet::default()
    }
}

#[tokio::test]
async fn accounts_are_unique_and_case_insensitive() {
    let r = repo();
    let id = r
        .create_account(&account("Kai"), Some("h".into()), 1)
        .await
        .unwrap();
    assert_eq!(
        r.create_account(&account("kai"), Some("h".into()), 1).await,
        Err(RepoError::AccountTaken)
    );
    let row = r.account(&account("KAI")).await.unwrap().unwrap();
    assert_eq!(row.id, id);
    assert_eq!(row.password_hash.as_deref(), Some("h"));
    assert_eq!(row.locale, Locale::EN);
    r.set_locale(id, Locale::from_static("pt-BR")).await.unwrap();
    assert_eq!(
        r.account(&account("kai")).await.unwrap().unwrap().locale,
        Locale::from_static("pt-BR")
    );
    assert_eq!(r.account(&account("nobody")).await.unwrap(), None);
}

#[tokio::test]
async fn session_works_once_and_expires() {
    let r = repo();
    let id = r.create_account(&account("kai"), None, 1).await.unwrap();
    let t = TokenHash([1; 32]);
    r.create_session(id, t, 100).await.unwrap();
    assert_eq!(r.take_session(t, 50).await.unwrap(), Some(id));
    assert_eq!(r.take_session(t, 50).await.unwrap(), None, "used up");
    let t2 = TokenHash([2; 32]);
    r.create_session(id, t2, 100).await.unwrap();
    assert_eq!(r.take_session(t2, 100).await.unwrap(), None, "expired");
}

#[tokio::test]
async fn devices_failed_logins_and_blocks() {
    let r = repo();
    let id = r.create_account(&account("kai"), None, 1).await.unwrap();
    let other = r.create_account(&account("ana"), None, 1).await.unwrap();
    let t = TokenHash([1; 32]);
    r.create_device(id, t, 10, 100).await.unwrap();
    assert!(r.device_ok(id, t, 50).await.unwrap());
    assert!(!r.device_ok(other, t, 50).await.unwrap(), "another account");
    assert!(!r.device_ok(id, t, 100).await.unwrap(), "expired");
    for i in 0..crate::DEVICES_PER_ACCOUNT as u8 {
        r.create_device(id, TokenHash([10 + i; 32]), 20 + u64::from(i), 1000)
            .await
            .unwrap();
    }
    assert!(!r.device_ok(id, t, 50).await.unwrap(), "oldest dropped");
    assert!(r.device_ok(id, TokenHash([10; 32]), 50).await.unwrap());
    r.delete_devices(id).await.unwrap();
    assert!(!r.device_ok(id, TokenHash([10; 32]), 50).await.unwrap());

    assert_eq!(r.take_failed_logins(id).await.unwrap(), 0);
    r.add_failed_login(id).await.unwrap();
    r.add_failed_login(id).await.unwrap();
    r.add_failed_login(other).await.unwrap();
    let rows = r.failed_logins(1).await.unwrap();
    assert_eq!(
        rows.iter()
            .map(|f| (f.name.as_str(), f.failed_logins))
            .collect::<Vec<_>>(),
        [("kai", 2), ("ana", 1)]
    );
    assert_eq!(r.take_failed_logins(id).await.unwrap(), 2);
    assert_eq!(r.take_failed_logins(id).await.unwrap(), 0);

    let b = crate::LoginBlock {
        key: "ip:10.0.0.1".into(),
        count: 7,
        last_at: 5,
        blocked_until: 99,
    };
    r.save_login_blocks(vec![b.clone()]).await.unwrap();
    assert_eq!(r.login_blocks().await.unwrap(), [b]);
    r.save_login_blocks(Vec::new()).await.unwrap();
    assert!(r.login_blocks().await.unwrap().is_empty());
}

#[tokio::test]
async fn characters_limit_and_unique_names() {
    let r = repo();
    let a = r.create_account(&account("aaa"), None, 1).await.unwrap();
    let b = r.create_account(&account("bbb"), None, 1).await.unwrap();
    let c1 = r
        .create_character(a, &name("Kai"), 2, &sheet(1), 1)
        .await
        .unwrap();
    assert_eq!(
        r.create_character(b, &name("KAI"), 2, &sheet(1), 1).await,
        Err(RepoError::NameTaken)
    );
    r.create_character(a, &name("Mio"), 2, &sheet(2), 1)
        .await
        .unwrap();
    assert_eq!(
        r.create_character(a, &name("Ren"), 2, &sheet(3), 1).await,
        Err(RepoError::TooManyCharacters)
    );
    let list = r.characters(a).await.unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].id, c1);
    assert!(r.characters(b).await.unwrap().is_empty());
}

#[tokio::test]
async fn save_and_load_position_and_sheet() {
    let r = repo();
    let a = r.create_account(&account("kai"), None, 1).await.unwrap();
    let c = r
        .create_character(a, &name("Kai"), 5, &sheet(4), 1)
        .await
        .unwrap();
    let loaded = r.load_character(c).await.unwrap().unwrap();
    assert_eq!(loaded.place, None);
    assert_eq!(loaded.account, a);
    r.save_zone_batch(
        vec![CharacterSave {
            id: c,
            zone: "dev_field".into(),
            pos: TilePos::new(-3, 7),
            sheet: sheet(5),
        }],
        2,
    )
    .await
    .unwrap();
    let loaded = r.load_character(c).await.unwrap().unwrap();
    assert_eq!(loaded.place, Some(("dev_field".into(), TilePos::new(-3, 7))));
    assert_eq!(loaded.sheet, sheet(5));
    assert_eq!(r.load_character(CharacterId::new(999)).await.unwrap(), None);
}

#[tokio::test]
async fn dev_characters_live_in_the_dev_account() {
    let r = repo();
    let first = r.dev_character(&name("Bot1"), &sheet(1), 1).await.unwrap();
    let again = r.dev_character(&name("bot1"), &sheet(9), 1).await.unwrap();
    assert_eq!(first.id, again.id);
    assert_eq!(again.sheet, sheet(1), "existing character keeps its sheet");
    let dev = r.account(&account(DEV_ACCOUNT)).await.unwrap().unwrap();
    assert_eq!(dev.password_hash, None);
    let real = r
        .create_account(&account("kai"), Some("h".into()), 1)
        .await
        .unwrap();
    r.create_character(real, &name("Kai"), 5, &sheet(1), 1)
        .await
        .unwrap();
    assert_eq!(
        r.dev_character(&name("Kai"), &sheet(1), 1).await,
        Err(RepoError::NameTaken)
    );
}

#[tokio::test]
async fn reopening_a_file_keeps_data_and_schema() {
    let dir = std::env::temp_dir().join(format!("persistence-test-{}", std::process::id()));
    let path = dir.join("game.db");
    let url = format!("sqlite:{}", path.display());
    {
        let r = Repository::open(&url).unwrap();
        r.create_account(&account("kai"), None, 1).await.unwrap();
    }
    let r = Repository::open(&url).unwrap();
    assert!(r.account(&account("kai")).await.unwrap().is_some());
    drop(r);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn unknown_sheet_version_is_an_error() {
    assert!(matches!(sheet::decode(99, &[]), Err(RepoError::Corrupt(_))));
    let (v, bytes) = sheet::encode(&sheet(7)).unwrap();
    assert_eq!(sheet::decode(v, &bytes).unwrap(), sheet(7));
}

#[test]
fn password_hash_roundtrip() {
    let h = hash_password("correct horse").unwrap();
    assert!(h.starts_with("$argon2id$"));
    assert!(verify_password("correct horse", &h));
    assert!(!verify_password("wrong horse", &h));
    assert!(!verify_password("x", "not a hash"));
}

#[test]
fn bad_url() {
    assert!(matches!(
        Repository::open("postgres://x"),
        Err(RepoError::BadUrl(_))
    ));
}

#[tokio::test]
async fn roles_and_temporary_password() {
    let r = repo();
    let id = r
        .create_account(&account("kai"), Some("h".into()), 1)
        .await
        .unwrap();
    let row = r.account(&account("kai")).await.unwrap().unwrap();
    assert_eq!(row.role, Role::Player);
    assert!(!row.must_change_password);

    r.set_role(id, Role::Admin, "cli:test", "kai", 2).await.unwrap();
    let row = r.account(&account("kai")).await.unwrap().unwrap();
    assert_eq!(row.role, Role::Admin);

    r.create_session(id, TokenHash([1; 32]), 10_000).await.unwrap();
    r.reset_password(id, "temp".into(), "cli:test", "kai", 3)
        .await
        .unwrap();
    let row = r.account(&account("kai")).await.unwrap().unwrap();
    assert_eq!(row.password_hash.as_deref(), Some("temp"));
    assert!(row.must_change_password);
    assert_eq!(
        r.take_session(TokenHash([1; 32]), 4).await.unwrap(),
        None,
        "reset drops the sessions"
    );

    r.set_password(id, Some("new".into()), false).await.unwrap();
    let row = r.account_by_id(id).await.unwrap().unwrap();
    assert_eq!(row.password_hash.as_deref(), Some("new"));
    assert!(!row.must_change_password);
}

#[tokio::test]
async fn sanctions_are_strongest_first_and_never_deleted() {
    let r = repo();
    let id = r
        .create_account(&account("kai"), Some("h".into()), 1)
        .await
        .unwrap();
    let suspend = r
        .add_sanction(
            NewSanction {
                kind: SanctionKind::Suspend,
                account: Some(id),
                ip: None,
                target: "kai".into(),
                reason: "botting".into(),
                note: String::new(),
                expires_at: Some(100),
                created_by: "cli:test".into(),
            },
            10,
        )
        .await
        .unwrap();
    let ban = r
        .add_sanction(
            NewSanction {
                kind: SanctionKind::Ban,
                account: Some(id),
                ip: None,
                target: "kai".into(),
                reason: "cheating".into(),
                note: "second".into(),
                expires_at: None,
                created_by: "cli:test".into(),
            },
            11,
        )
        .await
        .unwrap();
    assert_eq!(
        r.account_sanction(id, 12).await.unwrap().map(|s| s.kind),
        Some(SanctionKind::Ban)
    );
    assert_eq!(r.sanctions_of(id).await.unwrap().len(), 2);

    assert!(r.revoke_sanction(ban, "cli:test", 13).await.unwrap());
    assert!(!r.revoke_sanction(ban, "cli:test", 14).await.unwrap());
    assert_eq!(
        r.account_sanction(id, 15).await.unwrap().map(|s| s.kind),
        Some(SanctionKind::Suspend)
    );
    // Expired: nothing active, but the row stays.
    assert_eq!(r.account_sanction(id, 101).await.unwrap(), None);
    assert_eq!(r.sanctions_of(id).await.unwrap().len(), 2);
    assert_eq!(suspend.0, 1);
    assert_eq!(
        r.sanctions_changed_since(11).await.unwrap().len(),
        1,
        "only the ban created and revoked since then"
    );
}

#[tokio::test]
async fn ip_sanctions_use_the_normalized_key() {
    let r = repo();
    let ip: std::net::IpAddr = "2001:db8::1".parse().unwrap();
    let key = ip_key(ip);
    assert_eq!(key, "2001:db8::/64");
    r.add_sanction(
        NewSanction {
            kind: SanctionKind::IpBan,
            account: None,
            ip: Some(key.clone()),
            target: key.clone(),
            reason: "spam".into(),
            note: String::new(),
            expires_at: Some(100),
            created_by: "cli:test".into(),
        },
        10,
    )
    .await
    .unwrap();
    assert_eq!(
        r.ip_sanction(&key, 20).await.unwrap().map(|s| s.kind),
        Some(SanctionKind::IpBan)
    );
    assert_eq!(r.ip_sanction(&key, 101).await.unwrap(), None);
}

#[tokio::test]
async fn logins_are_kept_for_thirty_days() {
    let r = repo();
    let id = r
        .create_account(&account("kai"), Some("h".into()), 1)
        .await
        .unwrap();
    let old = 1;
    let now = 31 * 24 * 60 * 60;
    r.record_login(id, "10.0.0.1", old).await.unwrap();
    r.record_login(id, "10.0.0.2", now).await.unwrap();
    let rows = r.recent_logins(id, 10).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].ip, "10.0.0.2");
}

#[tokio::test]
async fn admin_log_records_every_action() {
    let r = repo();
    let id = r
        .create_account(&account("kai"), Some("h".into()), 1)
        .await
        .unwrap();
    r.create_character(id, &name("Kai"), 5, &sheet(1), 1)
        .await
        .unwrap();
    r.set_role(id, Role::Admin, "cli:test", "kai", 2).await.unwrap();
    r.reset_password(id, "temp".into(), "cli:test", "kai", 3)
        .await
        .unwrap();
    r.rename_character(&name("Kai"), &name("KaiTwo"), "cli:test", 4)
        .await
        .unwrap();
    r.add_sanction(
        NewSanction {
            kind: SanctionKind::Suspend,
            account: Some(id),
            ip: None,
            target: "kai".into(),
            reason: "spam".into(),
            note: String::new(),
            expires_at: Some(100),
            created_by: "cli:test".into(),
        },
        5,
    )
    .await
    .unwrap();
    r.revoke_sanction(SanctionId(1), "cli:test", 6).await.unwrap();

    let all = r.admin_log(None, None, 50).await.unwrap();
    let actions: Vec<&str> = all.iter().map(|l| l.action.as_str()).collect();
    assert_eq!(
        actions,
        ["revoke", "suspend", "rename", "reset_password", "promote"]
    );
    assert_eq!(r.admin_log(Some("kai"), None, 50).await.unwrap().len(), 3);
    assert_eq!(r.admin_log(None, Some(4), 50).await.unwrap().len(), 3);
    assert_eq!(r.admin_log_since(4).await.unwrap().len(), 3);
}

#[tokio::test]
async fn rename_frees_the_old_name() {
    let r = repo();
    let id = r
        .create_account(&account("kai"), Some("h".into()), 1)
        .await
        .unwrap();
    let first = r
        .create_character(id, &name("Kai"), 5, &sheet(1), 1)
        .await
        .unwrap();
    let other = r
        .create_account(&account("other"), Some("h".into()), 1)
        .await
        .unwrap();
    r.create_character(other, &name("KaiTwo"), 5, &sheet(1), 1)
        .await
        .unwrap();
    assert_eq!(
        r.rename_character(&name("Kai"), &name("KaiTwo"), "cli:test", 2)
            .await,
        Err(RepoError::NameTaken)
    );
    r.rename_character(&name("Kai"), &name("KaiRenamed"), "cli:test", 3)
        .await
        .unwrap();
    assert_eq!(
        r.rename_character(&name("Nope"), &name("Xyz"), "cli:test", 4)
            .await,
        Err(RepoError::NotFound)
    );
    // The old name is free again.
    r.create_character(other, &name("Kai"), 5, &sheet(2), 5)
        .await
        .unwrap();
    assert_eq!(first.get(), 1);
}

fn suspension(id: AccountId, expires_at: u64) -> NewSanction {
    NewSanction {
        kind: SanctionKind::Suspend,
        account: Some(id),
        ip: None,
        target: "kai".into(),
        reason: "botting".into(),
        note: String::new(),
        expires_at: Some(expires_at),
        created_by: "cli:test".into(),
    }
}

#[tokio::test]
async fn same_kind_sanctions_pick_the_one_that_ends_last() {
    let r = repo();
    let id = r
        .create_account(&account("kai"), Some("h".into()), 1)
        .await
        .unwrap();
    r.add_sanction(suspension(id, 500), 10).await.unwrap();
    r.add_sanction(suspension(id, 200), 11).await.unwrap();
    let active = r.account_sanction(id, 20).await.unwrap().unwrap();
    assert_eq!(active.expires_at, Some(500));
}

#[tokio::test]
async fn backup_check_counts_rows_and_rejects_garbage() {
    let dir = std::env::temp_dir().join(format!("persistence-backup-{}", std::process::id()));
    let path = dir.join("game.db");
    {
        let r = Repository::open(&format!("sqlite:{}", path.display())).unwrap();
        let id = r.create_account(&account("kai"), None, 1).await.unwrap();
        r.create_character(id, &name("Kai"), 5, &sheet(1), 1)
            .await
            .unwrap();
    }
    let file = path.display().to_string();
    assert_eq!(Repository::verify_backup(&file).unwrap(), (1, 1));
    let junk = dir.join("junk.db");
    std::fs::write(&junk, b"not a database").unwrap();
    assert!(Repository::verify_backup(&junk.display().to_string()).is_err());
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn coin_flow_is_kept_by_source_and_pruned() {
    let r = repo();
    let row = |at: u64, flow: &str, created: bool, amount: u64| crate::CoinFlowRow {
        at,
        flow: flow.into(),
        created,
        amount,
    };
    let old = crate::TELEMETRY_KEEP_SECS + 100;
    r.save_coin_flow(vec![
        row(10, "monster_loot", true, 50),
        row(10, "shop_purchase", false, 30),
    ])
    .await
    .unwrap();
    let rows = r.coin_flow(0).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].flow, "monster_loot");
    assert!(rows[0].created && !rows[1].created);
    // A later save prunes what is older than the window.
    r.save_coin_flow(vec![row(old, "repair", false, 7)])
        .await
        .unwrap();
    let rows = r.coin_flow(0).await.unwrap();
    assert_eq!(rows.len(), 1, "the old samples went away");
    assert_eq!(rows[0].flow, "repair");
    assert_eq!(r.save_coin_flow(Vec::new()).await, Ok(()));
}

fn with_item(id: u64) -> SavedSheet {
    SavedSheet {
        items: vec![SavedItem {
            id,
            item: "sword".into(),
            equipped: None,
            slot: 0,
            hidden: vec![],
            appraised: false,
            durability: (120, 120),
            crafter: None,
            rarity: None,
            mastery: 0,
            power: None,
        }],
        ..SavedSheet::default()
    }
}

fn save(id: CharacterId, sheet: SavedSheet) -> CharacterSave {
    CharacterSave {
        id,
        zone: "z".into(),
        pos: base::TilePos::new(0, 0),
        sheet,
    }
}

#[tokio::test]
async fn an_item_never_has_two_owners() {
    let r = repo();
    let acc = r.create_account(&account("Own"), None, 1).await.unwrap();
    let a = r
        .create_character(acc, &name("Alfa"), 3, &sheet(1), 1)
        .await
        .unwrap();
    let b = r
        .create_character(acc, &name("Bravo"), 3, &sheet(1), 1)
        .await
        .unwrap();
    let c = r
        .create_character(acc, &name("Charlie"), 3, &sheet(1), 1)
        .await
        .unwrap();
    let (start, end) = r.reserve_item_ids(10).await.unwrap();
    assert_eq!(end - start, 10);
    assert_eq!(
        r.reserve_item_ids(5).await.unwrap().0,
        end,
        "blocks never overlap"
    );
    // A holds the item.
    assert!(
        r.save_zone_batch(vec![save(a, with_item(start))], 2)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(r.item_owner(start).await.unwrap(), Some(a));
    // B claims it while A still has it: B's save is refused, A keeps it.
    let rejected = r
        .save_zone_batch(vec![save(b, with_item(start))], 3)
        .await
        .unwrap();
    assert_eq!(rejected, vec![(b, start)]);
    assert_eq!(r.item_owner(start).await.unwrap(), Some(a));
    assert!(r.load_character(b).await.unwrap().unwrap().sheet.items.is_empty());
    // A hands it to C in the same batch (a trade): allowed.
    let rejected = r
        .save_zone_batch(vec![save(a, SavedSheet::default()), save(c, with_item(start))], 4)
        .await
        .unwrap();
    assert!(rejected.is_empty());
    assert_eq!(r.item_owner(start).await.unwrap(), Some(c));
    let holders: usize = r
        .all_sheets()
        .await
        .unwrap()
        .iter()
        .filter(|(_, _, s)| s.items.iter().any(|i| i.id == start))
        .count();
    assert_eq!(holders, 1);
    // Every accepted save left a telemetry sample; the refused one did not.
    assert_eq!(r.telemetry(0).await.unwrap().len(), 3);
}

/// Phase 4 criterion: a crash (kill -9) in the middle of trades never leaves
/// an item with two holders. The test runs itself again as a child process
/// that trades one item between two characters in a loop; the parent kills
/// it at several points, reopens the file and checks the single holder.
#[tokio::test]
async fn a_crash_mid_trade_never_duplicates_an_item() {
    const CHILD: &str = "PERSISTENCE_CRASH_CHILD";
    if let Ok(url) = std::env::var(CHILD) {
        let r = Repository::open(&url).unwrap();
        let a = CharacterId::new(1);
        let c = CharacterId::new(2);
        let mut turn = 0u64;
        loop {
            let (from, to) = if turn.is_multiple_of(2) { (a, c) } else { (c, a) };
            let rejected = r
                .save_zone_batch(
                    vec![save(from, SavedSheet::default()), save(to, with_item(1))],
                    10 + turn,
                )
                .await
                .unwrap();
            assert!(rejected.is_empty());
            turn += 1;
        }
    }
    let dir = std::env::temp_dir().join(format!("persistence-crash-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let url = format!("sqlite:{}", dir.join("game.db").display());
    {
        let r = Repository::open(&url).unwrap();
        let acc = r.create_account(&account("Crash"), None, 1).await.unwrap();
        let a = r
            .create_character(acc, &name("Alfa"), 3, &sheet(1), 1)
            .await
            .unwrap();
        let c = r
            .create_character(acc, &name("Charlie"), 3, &sheet(1), 1)
            .await
            .unwrap();
        assert_eq!((a, c), (CharacterId::new(1), CharacterId::new(2)));
        let (start, _) = r.reserve_item_ids(1).await.unwrap();
        assert_eq!(start, 1);
        r.save_zone_batch(vec![save(a, with_item(1))], 2).await.unwrap();
    }
    for wait_ms in [30u64, 75, 140, 220] {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tests::a_crash_mid_trade_never_duplicates_an_item"])
            .env(CHILD, &url)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(wait_ms));
        assert!(
            child.try_wait().unwrap().is_none(),
            "the child must still be trading"
        );
        // SIGKILL: no destructor, no rollback by the process itself.
        child.kill().unwrap();
        child.wait().unwrap();
        let r = Repository::open(&url).unwrap();
        let holders: Vec<CharacterId> = r
            .all_sheets()
            .await
            .unwrap()
            .into_iter()
            .filter(|(_, _, s)| s.items.iter().any(|i| i.id == 1))
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(holders.len(), 1, "after {wait_ms} ms: {holders:?}");
        assert_eq!(r.item_owner(1).await.unwrap(), Some(holders[0]));
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn phase5_lookups_mutes_and_chat_reports() {
    let r = repo();
    let acc = r
        .create_account(&account("kai"), Some("h".into()), 1)
        .await
        .unwrap();
    let mut s = sheet(0);
    s.look = ["body_b".into(), "skin_2".into(), "bald".into(), "red".into()];
    let id = r.create_character(acc, &name("Kai"), 5, &s, 1).await.unwrap();
    let list = r.characters(acc).await.unwrap();
    assert_eq!((list[0].level, list[0].look[2].as_str()), (1, "bald"));
    assert_eq!(
        r.character_by_name(&name("KAI")).await.unwrap(),
        Some((id, acc, "kai".to_owned()))
    );
    assert_eq!(r.account_by_id(acc).await.unwrap().unwrap().name, "kai");
    assert_eq!(r.mute_until(acc, 10).await.unwrap(), None);
    let mute = |expires_at| NewSanction {
        kind: SanctionKind::Mute,
        account: Some(acc),
        ip: None,
        target: "kai".into(),
        reason: "moderation.reason.spam".into(),
        note: String::new(),
        expires_at,
        created_by: "admin".into(),
    };
    r.add_sanction(mute(Some(100)), 10).await.unwrap();
    r.add_sanction(mute(Some(50)), 10).await.unwrap();
    assert_eq!(r.mute_until(acc, 10).await.unwrap(), Some(100));
    assert_eq!(r.mute_until(acc, 100).await.unwrap(), None);
    // A mute never blocks the login and keeps the sessions.
    let report = |at| ChatReport {
        id: 0,
        reporter: "Ana".into(),
        target: "Kai".into(),
        lines: vec![ChatLine {
            at,
            channel: "local".into(),
            from: "Kai".into(),
            text: "olá\u{3000}mundo".into(),
        }],
        created_at: at,
    };
    r.add_chat_report(report(1)).await.unwrap();
    r.add_chat_report(report(1 + CHAT_REPORT_KEEP_SECS + 1))
        .await
        .unwrap();
    let all = r.chat_reports(0).await.unwrap();
    assert_eq!(all.len(), 1, "the old report is gone");
    assert_eq!(all[0].lines[0].text, "olá\u{3000}mundo");
    r.log_admin(AdminLogRow {
        id: 0,
        actor: "kai".into(),
        action: "kick".into(),
        target: "Ana".into(),
        args: String::new(),
        note: String::new(),
        created_at: 5,
    })
    .await
    .unwrap();
    assert!(
        r.admin_log_since(5)
            .await
            .unwrap()
            .iter()
            .any(|l| l.action == "kick")
    );
}

#[tokio::test]
async fn a_deleted_character_frees_its_name_slot_and_items() {
    let r = repo();
    let acc = r.create_account(&account("kai"), None, 1).await.unwrap();
    let other = r.create_account(&account("mio"), None, 1).await.unwrap();
    let kai = r
        .create_character(acc, &name("Kai"), 1, &sheet(1), 1)
        .await
        .unwrap();
    let (item, _) = r.reserve_item_ids(1).await.unwrap();
    r.save_zone_batch(vec![save(kai, with_item(item))], 2)
        .await
        .unwrap();
    // Another account cannot delete it; a wrong id is not found.
    assert_eq!(
        r.delete_character(other, kai, "mio", 3).await,
        Err(RepoError::NotFound)
    );
    assert_eq!(r.delete_character(acc, kai, "kai", 3).await, Ok(name("Kai")));
    assert_eq!(
        r.delete_character(acc, kai, "kai", 4).await,
        Err(RepoError::NotFound),
        "deleted once"
    );
    // Gone from the list, the slot, the lookups and the reports.
    assert!(r.characters(acc).await.unwrap().is_empty());
    assert_eq!(r.load_character(kai).await.unwrap(), None);
    assert_eq!(r.character_by_name(&name("kai")).await.unwrap(), None);
    assert!(r.all_sheets().await.unwrap().is_empty());
    assert_eq!(r.item_owner(item).await.unwrap(), None, "the item died with it");
    // The name is free again, and the limit counts live characters only.
    r.create_character(acc, &name("KAI"), 1, &sheet(1), 5)
        .await
        .unwrap();
    let log = r.admin_log(None, None, 10).await.unwrap();
    assert_eq!(log[0].action, "delete_character");
    assert_eq!(log[0].target, "Kai");
}

#[tokio::test]
async fn a_deleted_character_comes_back_with_its_items() {
    let r = repo();
    let acc = r.create_account(&account("kai"), None, 1).await.unwrap();
    let kai = r
        .create_character(acc, &name("Kai"), 2, &sheet(1), 1)
        .await
        .unwrap();
    let (item, _) = r.reserve_item_ids(1).await.unwrap();
    r.save_zone_batch(vec![save(kai, with_item(item))], 2)
        .await
        .unwrap();
    r.delete_character(acc, kai, "kai", 3).await.unwrap();
    let gone = r.deleted_characters(acc).await.unwrap();
    assert_eq!((gone[0].id, gone[0].deleted_at), (kai, 3));
    assert_eq!(
        r.restore_character(CharacterId::new(999), 2, "cli:x", 4).await,
        Err(RepoError::NotFound)
    );
    // Full account, then name reused: refused, nothing changes.
    let ana = r
        .create_character(acc, &name("Ana"), 2, &sheet(1), 4)
        .await
        .unwrap();
    let _bo = r
        .create_character(acc, &name("Bobo"), 2, &sheet(1), 4)
        .await
        .unwrap();
    assert_eq!(
        r.restore_character(kai, 2, "cli:x", 5).await,
        Err(RepoError::TooManyCharacters)
    );
    r.delete_character(acc, ana, "kai", 5).await.unwrap();
    let other = r.create_account(&account("mio"), None, 1).await.unwrap();
    let thief = r
        .create_character(other, &name("KAI"), 2, &sheet(1), 5)
        .await
        .unwrap();
    assert_eq!(
        r.restore_character(kai, 2, "cli:x", 6).await,
        Err(RepoError::NameTaken)
    );
    r.delete_character(other, thief, "mio", 6).await.unwrap();
    // Back: in the list, loadable, owning its piece again, audited.
    assert_eq!(r.restore_character(kai, 2, "cli:x", 7).await, Ok(name("Kai")));
    assert!(r.characters(acc).await.unwrap().iter().any(|c| c.id == kai));
    assert!(r.load_character(kai).await.unwrap().is_some());
    assert_eq!(r.item_owner(item).await.unwrap(), Some(kai));
    assert_eq!(
        r.admin_log(None, None, 1).await.unwrap()[0].action,
        "restore_character"
    );
    assert_eq!(
        r.restore_character(kai, 2, "cli:x", 8).await,
        Err(RepoError::NotFound),
        "already back"
    );
}

#[test]
fn backup_retention_keeps_hours_then_one_per_day() {
    const H: u64 = 3600;
    const D: u64 = 24 * H;
    let now = 100 * D;
    // One copy an hour for the last 40 days.
    let stamps: Vec<u64> = (0..40 * 24).map(|i| now - i * H).collect();
    let removed = backups_to_remove(&stamps, now);
    let kept: Vec<u64> = stamps.iter().copied().filter(|s| !removed.contains(s)).collect();
    // 48 hourly; days older than those keep their first copy, for 30 days.
    for i in 0..BACKUP_KEEP_HOURLY as u64 {
        assert!(kept.contains(&(now - i * H)), "hour {i}");
    }
    let old_days: Vec<u64> = kept
        .iter()
        .filter(|s| **s < now - 48 * H)
        .map(|s| s / D)
        .collect();
    let mut unique = old_days.clone();
    unique.dedup();
    assert_eq!(old_days, unique, "one copy per old day");
    assert!(kept.iter().all(|s| *s / D >= (now - BACKUP_KEEP_DAYS * D) / D));
    assert!(removed.windows(2).all(|w| w[0] < w[1]), "oldest first");
    // Few copies: nothing goes.
    assert!(backups_to_remove(&[now - H, now], now).is_empty());
    assert_eq!(backup_stamp("game-1763042400.db"), Some(1_763_042_400));
    assert_eq!(backup_stamp("game.db"), None);
    assert_eq!(backup_stamp("game-x.db"), None);
}

#[tokio::test]
async fn a_backup_is_a_checked_copy_and_old_ones_go() {
    let dir = std::env::temp_dir().join(format!("persistence-hourly-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let r = Repository::open(&format!("sqlite:{}", dir.join("game.db").display())).unwrap();
    let id = r.create_account(&account("kai"), None, 1).await.unwrap();
    r.create_character(id, &name("Kai"), 5, &sheet(1), 1)
        .await
        .unwrap();
    let backups = dir.join("backup");
    std::fs::create_dir_all(&backups).unwrap();
    // 48 hourly copies before this one, and one from 60 days ago: the
    // retention drops the stale one (past 30 days, beyond the 48 newest).
    let now = 100 * 24 * 3600;
    for h in 1..=48 {
        std::fs::write(backups.join(format!("game-{}.db", now - h * 3600)), b"x").unwrap();
    }
    let stale = backups.join(format!("game-{}.db", now - 60 * 24 * 3600));
    std::fs::write(&stale, b"old").unwrap();
    let done = r.backup(backups.clone(), now).await.unwrap();
    assert_eq!(done.file, backups.join(format!("game-{now}.db")));
    assert!(done.removed.contains(&stale), "{:?}", done.removed);
    assert!(!stale.exists());
    assert!(done.file.exists());
    assert_eq!(
        Repository::verify_backup(&done.file.display().to_string()).unwrap(),
        (1, 1)
    );
    // The game keeps writing while and after the copy is taken.
    r.create_character(id, &name("Mio"), 5, &sheet(1), 2)
        .await
        .unwrap();
    assert_eq!(r.characters(id).await.unwrap().len(), 2);
    // In memory (tests) the copy also works.
    let mem = repo();
    let done = mem.backup(backups.clone(), now + 3600).await.unwrap();
    assert!(Repository::verify_backup(&done.file.display().to_string()).is_ok());
    let _ = std::fs::remove_dir_all(dir);
}

fn signed(id: u64) -> SavedItem {
    SavedItem {
        crafter: Some("Forgemaster".into()),
        rarity: Some(3),
        mastery: 125,
        durability: (31, 40),
        ..with_item(id).items.remove(0)
    }
}

fn parcel(coins: u64, items: Vec<SavedItem>) -> EscrowParcel {
    EscrowParcel {
        coins,
        stacks: vec![SavedStack {
            item: "ingot".into(),
            qty: 3,
        }],
        items,
    }
}

/// Rule 4: the sheet and the parcel are written together, the
/// piece keeps its crafter, and nobody holds it while it waits.
#[tokio::test]
async fn escrow_moves_a_signed_piece_in_one_transaction() {
    let r = repo();
    let acc = r.create_account(&account("Mail"), None, 1).await.unwrap();
    let a = r
        .create_character(acc, &name("Alfa"), 3, &sheet(100), 1)
        .await
        .unwrap();
    let b = r
        .create_character(acc, &name("Bravo"), 3, &sheet(0), 1)
        .await
        .unwrap();
    let mut holding = with_item(7);
    holding.items = vec![signed(7)];
    assert!(
        r.save_zone_batch(vec![save(a, holding)], 2)
            .await
            .unwrap()
            .is_empty()
    );

    // Unknown recipient: nothing written, Alfa still owns the piece.
    let after = save(a, sheet(60));
    assert_eq!(
        r.commit_escrow(after.clone(), name("Nobody"), parcel(40, vec![signed(7)]), 3)
            .await,
        Err(RepoError::NotFound)
    );
    assert_eq!(r.item_owner(7).await.unwrap(), Some(a));
    assert_eq!(
        r.load_character(a).await.unwrap().unwrap().sheet.items,
        vec![signed(7)]
    );

    let id = r
        .commit_escrow(after, name("bravo"), parcel(40, vec![signed(7)]), 3)
        .await
        .unwrap();
    let alfa = r.load_character(a).await.unwrap().unwrap().sheet;
    assert_eq!((alfa.coins, alfa.items.len()), (60, 0));
    assert_eq!(r.item_owner(7).await.unwrap(), None, "custody holds it");
    // A stale save of Alfa with the piece is refused: never two owners.
    let mut stale = with_item(7);
    stale.items = vec![signed(7)];
    assert_eq!(
        r.save_zone_batch(vec![save(a, stale)], 4).await.unwrap(),
        vec![(a, 7)]
    );

    let waiting = r.escrows_for(b).await.unwrap();
    assert_eq!(waiting.len(), 1);
    let row = waiting.first().unwrap();
    assert_eq!((row.id, row.sender, row.recipient), (id, Some(a), b));
    assert_eq!(row.parcel, parcel(40, vec![signed(7)]));

    // Only the recipient takes it, once.
    assert_eq!(r.take_escrow(id, a).await.unwrap(), None);
    let got = r.take_escrow(id, b).await.unwrap().unwrap();
    assert_eq!(
        got.items.first().and_then(|i| i.crafter.as_deref()),
        Some("Forgemaster")
    );
    assert_eq!(r.take_escrow(id, b).await.unwrap(), None);
    assert!(r.escrows_for(b).await.unwrap().is_empty());
    // Bravo's next save claims it.
    let mut bravo = with_item(7);
    bravo.items = got.items.clone();
    assert!(
        r.save_zone_batch(vec![save(b, bravo)], 5)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(r.item_owner(7).await.unwrap(), Some(b));

    // No room at Bravo: back to custody, still Bravo's to take.
    let back = r.restore_escrow(None, b, got, 6).await.unwrap();
    assert_eq!(r.item_owner(7).await.unwrap(), None);
    assert_eq!(r.escrows_for(b).await.unwrap().first().map(|e| e.id), Some(back));
}

#[tokio::test]
async fn escrow_refuses_a_piece_the_sender_does_not_own() {
    let r = repo();
    let acc = r.create_account(&account("Thief"), None, 1).await.unwrap();
    let a = r
        .create_character(acc, &name("Alfa"), 3, &sheet(0), 1)
        .await
        .unwrap();
    let b = r
        .create_character(acc, &name("Bravo"), 3, &sheet(0), 1)
        .await
        .unwrap();
    assert!(
        r.save_zone_batch(vec![save(b, with_item(9))], 2)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        r.commit_escrow(save(a, sheet(0)), name("Bravo"), parcel(0, vec![signed(9)]), 3)
            .await,
        Err(RepoError::ItemOwned(9))
    );
    assert!(r.escrows_for(b).await.unwrap().is_empty());
    assert_eq!(r.item_owner(9).await.unwrap(), Some(b));
}

#[tokio::test]
async fn server_firsts_and_unique_holders() {
    let r = repo();
    let a = r.create_account(&account("kai"), None, 1).await.unwrap();
    let kai = r
        .create_character(a, &name("Kai"), 2, &sheet(0), 1)
        .await
        .unwrap();
    let mio = r
        .create_character(a, &name("Mio"), 2, &sheet(0), 1)
        .await
        .unwrap();
    // Only the first is recorded, for good.
    assert!(r.record_first("title:legend", kai, 10).await.unwrap());
    assert!(!r.record_first("title:legend", mio, 11).await.unwrap());
    assert_eq!(
        r.server_firsts().await.unwrap(),
        vec![("title:legend".to_owned(), name("Kai"), 10)]
    );
    // A unique key has one holder until it is released.
    assert_eq!(r.claim_unique("class:smith", kai, 5).await.unwrap(), kai);
    assert_eq!(r.claim_unique("class:smith", mio, 6).await.unwrap(), kai);
    assert!(
        !r.release_unique("class:smith", mio).await.unwrap(),
        "not the holder"
    );
    let held = r.unique_holders().await.unwrap();
    assert_eq!((held.len(), held[0].character, held[0].last_seen), (1, kai, 1));
    assert!(r.release_unique("class:smith", kai).await.unwrap());
    assert_eq!(
        r.claim_unique("class:smith", mio, 7).await.unwrap(),
        mio,
        "the heir"
    );
}

#[tokio::test]
async fn dungeon_clears_and_lockouts() {
    let r = repo();
    let a = r.create_account(&account("kai"), None, 1).await.unwrap();
    let kai = r
        .create_character(a, &name("Kai"), 2, &sheet(0), 1)
        .await
        .unwrap();
    r.add_clear("crypt", "Kai, Mio", 300, 10).await.unwrap();
    r.add_clear("crypt", "Ana", 200, 11).await.unwrap();
    r.add_clear("crypt", "Bea", 300, 9).await.unwrap();
    r.add_clear("tower", "Zed", 1, 9).await.unwrap();
    assert_eq!(
        r.fastest_clears("crypt", 2).await.unwrap(),
        vec![("Ana".to_owned(), 200), ("Bea".to_owned(), 300)]
    );
    r.set_lockout(kai, "crypt", 100).await.unwrap();
    r.set_lockout(kai, "crypt", 150).await.unwrap();
    assert_eq!(
        r.lockouts(50).await.unwrap(),
        vec![(kai, "crypt".to_owned(), 150)]
    );
    assert!(r.lockouts(150).await.unwrap().is_empty(), "over");
}

#[tokio::test]
async fn server_goals_are_saved_and_updated() {
    let r = repo();
    assert!(r.server_goals().await.unwrap().is_empty());
    r.save_server_goals(vec![("cull".into(), 40, None), ("feast".into(), 3, None)])
        .await
        .unwrap();
    r.save_server_goals(vec![("cull".into(), 100, Some(77))])
        .await
        .unwrap();
    assert_eq!(
        r.server_goals().await.unwrap(),
        vec![("cull".to_owned(), 100, Some(77)), ("feast".to_owned(), 3, None)]
    );
}
