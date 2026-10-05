//! One task writes every save in arrival order, so a later save of a
//! character never lands before an earlier one. A character leaving is
//! released (may enter again) only after its save is on disk.
//!
//! Custody (rule 4) rides the same queue: a deposit is written in
//! order with the saves around it, so a later batch never lands before the
//! sheet that gave the parcel away.

use crate::gateway::Done;
use crate::zone_link::ToZone;
use base::{AccountId, CharacterId, CharacterName};
use persistence::{CharacterSave, EscrowParcel, RepoError, Repository};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info};

#[derive(Default)]
pub struct SaveJob {
    pub saves: Vec<CharacterSave>,
    /// Characters to release after the write (they left the game).
    pub release: Vec<(Option<AccountId>, CharacterId)>,
    /// A custody write instead of a batch save.
    pub escrow: Option<EscrowJob>,
}

pub enum EscrowJob {
    /// `saves` holds the sender's sheet without the parcel: both go in one
    /// transaction; the zone hears `EscrowDone` on `reply`.
    Deposit {
        reply: mpsc::UnboundedSender<ToZone>,
        ticket: u64,
        to: CharacterName,
        parcel: EscrowParcel,
    },
    /// A parcel back into custody (no room, or its sender left).
    Restore {
        sender: Option<CharacterId>,
        recipient: CharacterId,
        parcel: EscrowParcel,
    },
}

/// The task ends when every sender is dropped and the queue is empty.
pub fn start(
    repo: Arc<Repository>,
    done: mpsc::UnboundedSender<Done>,
) -> (mpsc::UnboundedSender<SaveJob>, JoinHandle<()>) {
    let (tx, mut rx) = mpsc::unbounded_channel::<SaveJob>();
    let task = tokio::spawn(async move {
        while let Some(job) = rx.recv().await {
            let now = crate::login::now_unix();
            match job.escrow {
                Some(op) => escrow(&repo, job.saves, op, now).await,
                None => {
                    let count = job.saves.len();
                    match repo.save_zone_batch(job.saves, now).await {
                        Ok(rejected) if rejected.is_empty() => debug!(count, "saved"),
                        // Never two owners: that character keeps its previous save.
                        Ok(rejected) => {
                            error!(?rejected, count, "saves refused: item owned by another character")
                        }
                        // The previous save stays; the player loses at most this interval.
                        Err(e) => error!(error = %e, count, "save failed"),
                    }
                }
            }
            for (account, character) in job.release {
                let _ = done.send(Done::Released { account, character });
            }
        }
    });
    (tx, task)
}

async fn escrow(repo: &Repository, saves: Vec<CharacterSave>, op: EscrowJob, now: u64) {
    match op {
        EscrowJob::Deposit {
            reply,
            ticket,
            to,
            parcel,
        } => {
            let Some(save) = saves.into_iter().next() else {
                return;
            };
            let sender = save.id;
            let ok = match repo.commit_escrow(save, to.clone(), parcel, now).await {
                Ok(id) => {
                    info!(escrow = id, sender = sender.get(), to = %to, "parcel in custody");
                    true
                }
                // An unknown name is the player's mistake, not the server's.
                Err(RepoError::NotFound) => {
                    debug!(sender = sender.get(), to = %to, "custody refused: no such character");
                    false
                }
                Err(e) => {
                    error!(error = %e, sender = sender.get(), "custody refused");
                    false
                }
            };
            let _ = reply.send(ToZone::Command(sim::Command::EscrowDone { ticket, ok }));
        }
        EscrowJob::Restore {
            sender,
            recipient,
            parcel,
        } => match repo.restore_escrow(sender, recipient, parcel, now).await {
            Ok(id) => info!(escrow = id, recipient = recipient.get(), "parcel back in custody"),
            // Its pieces belong to someone now: never two owners.
            Err(e) => error!(error = %e, recipient = recipient.get(), "parcel not restored"),
        },
    }
}
