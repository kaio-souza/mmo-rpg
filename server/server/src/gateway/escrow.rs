//! Custody (`escrow`, rule 4) between the zones and the database.
//!
//! - A deposit (`FromZone::Escrow`) goes to the saver with the sender's
//!   sheet: one transaction; the zone hears `EscrowDone`.
//! - On entering the game, parcels waiting for the character are taken out
//!   of custody one by one and handed to its zone (`Command::Deliver`); the
//!   zone saves the sheet at once, or hands the parcel back (no room), and
//!   it goes back into custody.

use super::{Done, Gateway};
use crate::saving::{EscrowJob, SaveJob};
use crate::translation;
use crate::zone_link::ToZone;
use base::{CharacterId, PlayerId, ZoneId};
use persistence::CharacterSave;
use tracing::{error, warn};

impl Gateway {
    pub(super) fn on_escrow(&mut self, zone: ZoneId, e: sim::EscrowOut) {
        let Some(reply) = self.zones.get(&zone).map(|z| z.tx.clone()) else {
            return;
        };
        let save = CharacterSave {
            id: e.from,
            zone: self.place(zone, e.pos).0,
            pos: self.place(zone, e.pos).1,
            sheet: translation::sheet_to_saved(&e.sheet),
        };
        self.save(SaveJob {
            saves: vec![save],
            release: Vec::new(),
            escrow: Some(EscrowJob::Deposit {
                reply,
                ticket: e.ticket,
                to: e.to,
                parcel: translation::parcel_to_saved(&e.parcel),
            }),
        });
    }

    /// A refused deposit whose sender is gone: custody keeps it for them.
    pub(super) fn on_escrow_lost(&mut self, from: CharacterId, parcel: &sim::EscrowParcel) {
        warn!(
            sender = from.get(),
            "refused parcel could not go back: kept in custody"
        );
        self.restore(Some(from), from, parcel);
    }

    pub(super) fn on_delivered(&mut self, zone: ZoneId, d: sim::DeliveredOut) {
        let Some(character) = self.delivering.remove(&d.escrow) else {
            error!(escrow = d.escrow, "delivery nobody asked for");
            return;
        };
        match d.result {
            // Saved now, not at the next checkpoint: the row is already gone.
            Ok((sheet, pos)) => self.save(SaveJob {
                saves: vec![CharacterSave {
                    id: character,
                    zone: self.place(zone, pos).0,
                    pos: self.place(zone, pos).1,
                    sheet: translation::sheet_to_saved(&sheet),
                }],
                release: Vec::new(),
                escrow: None,
            }),
            Err(parcel) => self.restore(None, character, &parcel),
        }
    }

    fn restore(&self, sender: Option<CharacterId>, recipient: CharacterId, parcel: &sim::EscrowParcel) {
        self.save(SaveJob {
            saves: Vec::new(),
            release: Vec::new(),
            escrow: Some(EscrowJob::Restore {
                sender,
                recipient,
                parcel: translation::parcel_to_saved(parcel),
            }),
        });
    }

    /// The character just entered the game: take what waits for it.
    pub(super) fn deliver_waiting(&self, player: PlayerId, character: CharacterId) {
        let (repo, done) = (self.repo.clone(), self.done_tx.clone());
        tokio::spawn(async move {
            let rows = match repo.escrows_for(character).await {
                Ok(rows) => rows,
                Err(e) => return warn!(error = %e, "custody not read"),
            };
            for row in rows {
                match repo.take_escrow(row.id, character).await {
                    Ok(Some(parcel)) => {
                        let _ = done.send(Done::EscrowTaken {
                            player,
                            character,
                            escrow: row.id,
                            parcel,
                        });
                    }
                    Ok(None) => {}
                    Err(e) => warn!(error = %e, escrow = row.id, "parcel not taken"),
                }
            }
        });
    }

    /// A parcel out of the database: to the zone that has the character
    /// now, or straight back into custody.
    pub(super) fn on_escrow_taken(
        &mut self,
        player: PlayerId,
        character: CharacterId,
        escrow: u64,
        parcel: persistence::EscrowParcel,
    ) {
        let parcel = translation::parcel_to_sim(&parcel);
        match self.in_game.get(&player).filter(|w| w.character == character) {
            Some(who) => {
                let zone = who.zone;
                self.delivering.insert(escrow, character);
                self.to_zone(
                    zone,
                    ToZone::Command(sim::Command::Deliver {
                        player,
                        escrow,
                        parcel: Box::new(parcel),
                    }),
                );
            }
            None => self.restore(None, character, &parcel),
        }
    }
}
