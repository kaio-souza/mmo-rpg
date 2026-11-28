# Mechanics

What the engine does today, configured by [content](content/README.md), and the mechanics a game could add later. A game uses as much of it as it wants: the template uses a few, the Riftara example uses almost all.

## In the engine today

**Combat**
- Hit, damage, defense and critical hits from attributes and formulas; line of sight; threat decides a monster's target.
- Weapon blows cost vigor (without vigor the blow waits twice as long); ranged monsters keep their distance.
- Techniques with cooldown, vigor or MP cost, damage, healing, revival and timed buffs on the caster.
- Rest: sitting (button in the status strip, key X) multiplies regeneration out of combat; any order stands up. A title bonus `LastStand(secs)` leaves 1 HP after a fatal blow, once every so many seconds.
- Counter-attack: an idle player hit by a monster fights back on their own.
- Falling and death: a character falls first and can be revived (by a technique or an item); death costs a level and locks the character for a while (both content numbers; none below `newbie_level`).
- PvP only inside `Pvp` regions; no combat in `Safe` regions.
- Statuses left by blows: poison, stun, slow; techniques that hit an area around the target.
- Bosses with phases: below a share of HP a monster hits harder, faster, in an area or with a status.

**Progression**
- Levels, XP and attribute points; six attributes; HP, vigor and MP.
- Proficiencies (0 to 100) that grow by use, with less gain for repetition.
- Techniques unlocked by proficiency points, levelled 1 to 10 by use; techniques only taught by an NPC, quest or class.
- Classes in tiers 1 to 4 with requirements (attributes, proficiencies or titles; rare classes earned by a deed and unseen until then), attribute points per level and faster proficiencies; production classes with a quality cap.
- Titles earned by level, counters, proficiencies or attributes; passive bonuses from every owned title; titles that teach techniques or replace others.
- Special attributes earned by titles or grown by a class (used to gate techniques and add to a proficiency).
- Titles and classes unique per server, passed to an heir when the holder stops playing; relics (items) that exist once.
- Titles announced to the whole server; server firsts kept for good (`/firsts`); public rankings (`/ranking`).

**Items and economy**
- Items with identity that can never be duplicated; stacks for materials and consumables; binding.
- Six rarities; hidden attribute rolls revealed by appraisal; powers (a borrowed technique or attribute bonus) from Unique up.
- Durability, breaking at zero, repair at shops (lowering maximum durability) or by a production class with the recipe's material.
- Consumables for HP, vigor, MP and reviving an ally; a hotbar the player arranges.
- NPC shops: buy, sell, repair, appraise, stock that restocks, a purse per player; monster and node loot, shared loot tables.
- Gathering from nodes that empty per player; recipes taught by NPCs; crafted pieces signed by their maker, with rolled rarity and maker mastery.
- Money sinks and coin telemetry per source; escrow that holds items and coins for a recipient (used by mail and market later).
- Off-hand items and items that light their wearer and burn while equipped (a torch).

**World**
- NPCs with dialogue graphs built from conditions and effects; affinity per NPC; karma, a hidden morality that dialogues move and that opens options and titles.
- Server goals: one shared quest the whole server works on, announced at each quarter and at the end, read with `/goals`; a reached goal opens dialogue options.
- Adventurer rank F to SSS: completing `quests_per_rank` quests of a rank raises it; a quest above the character's rank cannot start.
- Barbers: dialogue effects `SetHairStyle` and `SetHairColor`.
- Weather per zone (rain, snow, fog) with a chance per world hour, the same for every player (picture only).
- Quests: talk, deliver, kill, travel and craft objectives, item, coin and technique rewards, repeatable quests with a cooldown, quest log, markers and a pointer to the goal.
- Zones joined by seams into one open world, in one process or many; a starting zone; safe respawn.
- Portals and NPC teleports; instance zones (a copy per party) and dungeons with level and group checks, a boss that clears the run, lockouts and fastest-clear rankings.
- Day and night (picture only), light per tile, sprite and item; fog outside sight; tiles players toggle for everyone (braziers).
- Sound per event and technique, music per zone and screen.

**Social**
- Local chat and whispers, reports, mute; emotes with an image.
- Taming: monsters with a `tame` block (requirements the game chooses, weakened, a chance) become companions that follow, fight beside their owner and rest when they fall; window P calls, sends back and releases them.
- Companions get a name from their owner (window P). A bestiary (window J, scrolls) lists every monster defeated or tamed, with its HP, whether it attacks first, its drops and whether it can be tamed.
- Groups of up to 6 sharing XP by contribution; direct trade with confirmations on both sides, saved atomically.
- Ten languages; each player picks one.

**Accounts and moderation**
- Accounts with password, TLS, sessions, device tokens; brute-force defence ([security](security.md)).
- Character creation with appearance, drawn in layers (body tinted by skin, clothes, hair tinted by its colour); deleting and restoring characters.
- Roles, mute, suspend, ban, IP ban, audit log; announced restarts; hourly backups ([operations](operations.md)).

## Possible mechanics (not in the engine)

Designed for the example game but not built. Each needs engine work before content can use it.

| Area | Mechanic |
|---|---|
| Combat | damage types; a murderer mark for player killers |
| Progression | class change after level 30 within the tree; titles that give a penalty instead of a bonus; attributes earned by deeds; collections |
| Items | enhancement +1 to +15 with protection stones; dismantling equipment; carry weight limits; dimensional storage; exclusive items created by legendary classes |
| Economy | regional markets plus a global auction; buy orders; mail |
| World | ranked and hidden quests; karma moved by deeds outside dialogue (killing players, crimes); goals with rewards, timers and repeats; NPC routines; field bosses, invasions, rifts; travel and discovery; factions, server projects, story chapters |
| Social | group loot rules and shared quests; clans with ranks, storage and alliances; territory war with scheduled sieges; marriage with a shared vault; fame, bounties, mentoring, housing ([homes](player-homes.md): farming, breeding, carpentry); a human check for players who repeat one action for hours |
| Accounts | second factor (TOTP) at login |
| Presentation | a look built from parts (clothes chosen apart from the body) with palettes ([character parts](character-parts.md)); ambient sound layers |
