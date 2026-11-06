# Riftara (example game)

A complete game made with the kit, to show what it can do and to give the tests real data. It is **not part of the kit**: nothing here is a default, and a new game starts from [`templates/new-game/`](../../templates/new-game/README.md), not from this.

An isekai fantasy: the player is summoned into another world. Seven zones (three towns, hunting grounds, mines, a marsh), 20 monsters, 13 NPCs, 16 quests, 24 classes, 53 items, crafting and gathering, shops, groups and trade, day and night, ten languages.

## Run it

```sh
cd examples/riftara
server --dev --allow-missing-text
client --dev --server 127.0.0.1:7000 --name Ana --locale en
```

Build the client with `MMO_GAME_DIR=examples/riftara cargo build --release -p client` to see Riftara's title and interface texts.

## A first hour

1. **Arrival:** talk to the Summoner in the summoning circle ("What should I do now?", "I am ready.") to start the tutorial. The arrival title gives +5% XP up to level 10.
2. **Learn:** the Instructor, east of the plaza, teaches the Kick ("Teach me to fight."). Put it on the hotbar (`K`).
3. **Hunt:** the Guide waits at the south gate of Aurelia; past it are the Fields with rats, wolves and boars.
4. **Register:** the Guild, north of the plaza, gives rank F, a guild card, coins and potions.
5. **Trade and craft:** the market quarter west of the plaza has the merchant and the smith (right-click opens the shop; `G` opens crafting).
6. **Further:** the Briar Hills east of the Fields; Whisperwood to the west; Fenwick in the marsh south; Stonebrook and the mines east of the hills.

## What is here

| Folder | What |
|---|---|
| `content/` | all the game's rules and texts |
| `assets/` | provisional art, sounds, themes, fonts, interface texts |
| `translation.md` | fixed translations of this game's terms and names |
| `art-src/` | Python scripts that generate the provisional art and sounds (`python3 art-src/generate_art.py`, needs `pillow` and `numpy`) and the zone maps (`art-src/world_maps.py`) |
