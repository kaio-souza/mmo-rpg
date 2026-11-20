# Assets

Everything the client shows or plays is in the game's `assets/` folder and is called **by name**, never by number or colour in code. Replacing art is replacing a file with the same name; nothing is compiled. Anything without art falls back to a generic shape, so missing art never hides the game.

## Layout

```text
assets/
├── tiles.ron                  floor number → sprite (and light)
├── light.ron                  colour of the day by hour, and the light each player carries
├── theme.ron                  which theme players see
├── sprites/
│   ├── sprites.ron            anchors and animation strips, per folder or per sprite
│   ├── tiles/<name>.png       floors, 64×32
│   ├── walls/<name>.png       walls and tall objects, 64×72
│   ├── creatures/<name>.png   player, NPCs, monsters (64×64 frames)
│   └── equipment/<name>.png   layers drawn over the body: weapon, armour, off-hand
├── icons/<name>.png           items, techniques, classes, titles, monsters, emotes, interface (32×32)
├── themes/<id>.ron            colours, fonts and widget style of the interface
├── fonts/                     font files and their licences
├── text/<code>/ui.ron         interface texts (read when the client starts)
├── sounds/, music/            .ogg files; sounds/sounds.ron binds them to events
├── ui/login_background.png    optional login screen background (16:9, 640×360 or more)
└── CREDITS.md                 author and licence of every file that needs one
```

A name is the file name without `.png`, in `[a-z0-9_]`, unique in its folder. Content refers to these names: `icon` (items, techniques, classes, titles, monsters, special attributes, emotes), `sprite` (monsters, NPCs, nodes), `visual` (equipment layer).

## Floors: `tiles.ron`

```ron
{
    1: (sprite: "grass"),
    3: (sprite: "stone_wall", wall: true),      // stands up: sorted with creatures
    6: (sprite: "brazier", wall: true,
        light: Some((radius: 6.0, color: (1.0, 0.62, 0.28), strength: 0.75)),
        day_sprite: Some("brazier_unlit")),     // drawn instead by day
}
```

The zone map stores the number ([world](content/world.md#mapron)); this file is only the look.

## Sprites and animation: `sprites.ron`

```ron
(
    folders: {
        "creatures": (anchor: (32, 58), frames: 4, fps: 8, directions: 4, play: Moving),
        "tiles": (anchor: (32, 16)),
        "walls": (anchor: (32, 56)),
    },
    sprites: { "brazier": (frames: 4, fps: 6) },   // a sprite's fields win over its folder's
    lights: { "iron_vein": (radius: 2.0, color: (0.55, 0.75, 1.0), strength: 0.5) },
)
```

- **Anchor:** the pixel of one frame that sits on the centre of its tile.
- **Strips:** an animated image is one PNG with frames side by side and, with 4 directions, one row per direction in the order south, west, north, east. `frames` 1 to 64, `fps` (default 8), `directions` 1 or 4, `play`: `Always` (scenery) or `Moving` (only while walking; standing shows frame 1).
- Equipment layers follow the body's frame and direction when they are strips too.
- Character layers (body, clothes, hair; [appearance](content/characters.md#appearance)) are strips of the same size and anchor as `player`. A layer tinted by a colour (body, hair) is painted in light greys: the game multiplies it by the skin or hair colour, so white becomes that colour and the shading stays.
- A strip that does not divide into its frames and rows is drawn whole, with a warning in the client log and the editor.

## Light

`light.ron` holds the ambient colour across one world day and the light around every player. Tiles (`light` in `tiles.ron`), sprites (`lights` in `sprites.ron`) and equipped items (`light` on the item) add their own. Light is only picture: no rule reads it.

## Themes and fonts

`theme.ron` names the theme everyone sees (`GameTheme(theme: "isekai")`); `themes/<id>.ron` holds colours, fonts and widget style. The editor's **Theme** screen edits it, and the running client follows. Languages with their own scripts (Cyrillic, Chinese, Japanese, Korean) have their own font set.

## Sounds and music

`sounds/sounds.ron` lists every sound file with its licence (`CC0`, `CC-BY-3.0`, `CC-BY-4.0` or `commissioned`) and credit, then binds them to events (hits, clicks, errors, System notices), techniques, zone music (fading in from the zone's edge) and screen music (`login`). Files are Ogg Vorbis at 44 100 Hz; an event picks among up to 8 variations. Edit it on the editor's **Sounds** screen.

## Where art comes from

Draw it in any tool (Aseprite, LibreSprite), generate it, or both; the kit only reads PNG files. The editor's **Graphics** screen lists sprites and icons with previews and accepts uploads; copying a file into `assets/` works too. In development the client re-reads `assets/` within a second of a change. The template's art was made with PixelLab and adjusted by hand ([credits](../templates/new-game/assets/CREDITS.md)).
