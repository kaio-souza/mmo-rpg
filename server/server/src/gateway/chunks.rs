//! Terrain the client needs: the encoded `Chunk` frame of every chunk of
//! the world (all zones' maps, phase 10), and which of them each player
//! already has.

use super::*;

impl Gateway {
    /// The zone's tiles after a reload (phase 6c): changed chunks go again
    /// to whoever has them; same size only (a resized map waits for a restart).
    pub(super) fn refresh_chunks(&mut self) {
        let content = crate::content::slot_get(&self.cfg.slot);
        let words = world_words(&content.catalog);
        let fresh = encode_chunks(&words);
        let changed: BTreeSet<(i32, i32)> = fresh
            .iter()
            .filter(|(k, v)| self.chunks.get(k).is_some_and(|old| old != *v))
            .map(|(k, _)| *k)
            .collect();
        if changed.is_empty() || fresh.keys().ne(self.chunks.keys()) {
            return;
        }
        self.chunks = fresh;
        self.chunk_words = words;
        self.resend(&changed);
        info!(chunks = changed.len(), "zone map repainted");
    }

    /// Tiles a player switched in a zone: the chunk's word is
    /// patched, encoded again and sent again to whoever has it.
    pub(super) fn switch_tiles(&mut self, tiles: &[(TilePos, u16)]) {
        let mut changed = BTreeSet::new();
        for (pos, word) in tiles {
            let key = protocol::chunk_of(*pos);
            let Some(words) = self.chunk_words.get_mut(&key) else {
                continue;
            };
            let i = (pos.y - key.1 * protocol::CHUNK_SIZE) * protocol::CHUNK_SIZE + pos.x
                - key.0 * protocol::CHUNK_SIZE;
            if let Some(w) = usize::try_from(i).ok().and_then(|i| words.get_mut(i)) {
                *w = *word;
                changed.insert(key);
            }
        }
        for key in &changed {
            if let Some(frame) = self
                .chunk_words
                .get(key)
                .and_then(|w| encode_chunk(*key, w.clone()))
            {
                self.chunks.insert(*key, frame);
            }
        }
        self.resend(&changed);
    }

    /// The chunks go again to every connection in the game that has them.
    fn resend(&mut self, changed: &BTreeSet<(i32, i32)>) {
        let ids: Vec<ConnectionId> = self.conns.keys().copied().collect();
        for c in ids {
            let Some(conn) = self.conns.get_mut(&c) else {
                continue;
            };
            conn.sent_chunks.retain(|k| !changed.contains(k));
            if matches!(conn.phase, Phase::InZone(_)) {
                self.stream_chunks(c);
            }
        }
    }

    /// Sends the chunks in range that this connection does not have yet.
    pub(super) fn stream_chunks(&mut self, c: ConnectionId) {
        let view_radius = self.cfg.view_radius;
        let Some(conn) = self.conns.get_mut(&c) else {
            return;
        };
        let keys = to_send(&mut conn.sent_chunks, conn.pos, view_radius, |k| {
            self.chunks.contains_key(k)
        });
        for key in keys {
            if let Some(frame) = self.chunks.get(&key) {
                self.net.send(c, frame.clone());
            }
        }
    }
}

/// Chunks in range of `pos` that are not in `sent`, now marked sent. The
/// client forgets far chunks (`KEEP_CHUNKS`); `sent` forgets them a chunk
/// sooner, so coming back sends them again.
fn to_send(
    sent: &mut BTreeSet<(i32, i32)>,
    pos: TilePos,
    view_radius: u16,
    exists: impl Fn(&(i32, i32)) -> bool,
) -> Vec<(i32, i32)> {
    sent.retain(|k| protocol::chunk_kept(pos, *k, protocol::KEEP_CHUNKS - 1));
    let r = i32::from(view_radius) + CHUNK_MARGIN;
    let (x0, y0) = protocol::chunk_of(TilePos::new(pos.x.saturating_sub(r), pos.y.saturating_sub(r)));
    let (x1, y1) = protocol::chunk_of(TilePos::new(pos.x.saturating_add(r), pos.y.saturating_add(r)));
    let mut new = Vec::new();
    for cy in y0..=y1 {
        for cx in x0..=x1 {
            if exists(&(cx, cy)) && sent.insert((cx, cy)) {
                new.push((cx, cy));
            }
        }
    }
    new
}

pub(super) fn in_range(view_radius: u16, pos: TilePos, cx: i32, cy: i32) -> bool {
    let r = i32::from(view_radius) + CHUNK_MARGIN;
    let (x0, y0) = protocol::chunk_of(TilePos::new(pos.x.saturating_sub(r), pos.y.saturating_sub(r)));
    let (x1, y1) = protocol::chunk_of(TilePos::new(pos.x.saturating_add(r), pos.y.saturating_add(r)));
    (x0..=x1).contains(&cx) && (y0..=y1).contains(&cy)
}

/// Tile words of every chunk of the world: each zone's map, joined where a
/// chunk holds tiles of two zones (zones never overlap).
pub(super) fn world_words(catalog: &defs::ValidatedCatalog) -> BTreeMap<(i32, i32), Vec<u16>> {
    let mut words: BTreeMap<(i32, i32), Vec<u16>> = BTreeMap::new();
    for (_, def) in catalog.zones() {
        let Ok(map) = sim::zone_map(def) else { continue };
        for (cx, cy) in map.chunks() {
            let new = map.chunk_words(cx, cy);
            match words.get_mut(&(cx, cy)) {
                Some(old) => {
                    for (o, n) in old.iter_mut().zip(new) {
                        if n != 0 {
                            *o = n;
                        }
                    }
                }
                None => {
                    words.insert((cx, cy), new);
                }
            }
        }
    }
    words
}

/// The encoded `Chunk` frame of each chunk.
pub(super) fn encode_chunks(words: &BTreeMap<(i32, i32), Vec<u16>>) -> BTreeMap<(i32, i32), Bytes> {
    words
        .iter()
        .filter_map(|(k, w)| Some((*k, encode_chunk(*k, w.clone())?)))
        .collect()
}

fn encode_chunk((cx, cy): (i32, i32), words: Vec<u16>) -> Option<Bytes> {
    let tiles = ChunkTiles::try_from(words).ok()?;
    let mut buf = Vec::new();
    protocol::encode(&ServerMsg::Chunk { cx, cy, tiles }, &mut buf).ok()?;
    Some(Bytes::from(buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chunk_the_client_forgot_is_sent_again() {
        let mut sent = BTreeSet::new();
        let home = TilePos::new(10, 10);
        let all = |_: &(i32, i32)| true;
        assert!(to_send(&mut sent, home, 20, all).contains(&(0, 0)));
        assert!(to_send(&mut sent, home, 20, all).is_empty());
        // Far enough that the client dropped the home chunk.
        let far = TilePos::new(10 + 32 * (protocol::KEEP_CHUNKS + 1), 10);
        to_send(&mut sent, far, 20, all);
        assert!(!sent.contains(&(0, 0)));
        assert!(to_send(&mut sent, home, 20, all).contains(&(0, 0)));
    }

    #[test]
    fn the_server_forgets_before_the_client() {
        let mut sent = BTreeSet::new();
        let center = TilePos::new(0, 0);
        to_send(&mut sent, center, 20, |_| true);
        for k in &sent {
            assert!(protocol::chunk_kept(center, *k, protocol::KEEP_CHUNKS - 1));
        }
    }
}
