//! Actions and the key map. Keys are
//! physical positions named as `winit` names them (`KeyW`, `Digit1`,
//! `Escape`), so the map holds for any keyboard layout. The user file
//! `keybinds.ron` keeps only what differs from the default:
//!
//! ```ron
//! (wasd: true, keys: {"open_inventory": "KeyB", "hotbar_1": "Shift+Digit1"})
//! ```

use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Action {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    Stop,
    /// 0..=9: slots 1 to 10.
    Hotbar(u8),
    TargetNext,
    TargetClear,
    Interact,
    ChatOpen,
    OpenInventory,
    OpenSheet,
    OpenTechniques,
    OpenQuests,
    OpenMap,
    OpenSocial,
    /// Phase 8: the crafting window.
    OpenCrafting,
    /// The help window: keys and tips.
    OpenHelp,
    /// The Options window (sound).
    OpenOptions,
    /// The companions window: tamed monsters.
    OpenCompanions,
    /// The bestiary: monsters defeated and tamed.
    OpenBestiary,
    /// Sit down to rest or stand up.
    Rest,
    /// All sound off or back on.
    ToggleMute,
    ShowNames,
    Menu,
    Screenshot,
}

impl Action {
    /// Every action, in the order of the docs.
    pub fn all() -> Vec<Action> {
        let mut v = vec![
            Action::MoveUp,
            Action::MoveDown,
            Action::MoveLeft,
            Action::MoveRight,
            Action::Stop,
        ];
        v.extend((0..10).map(Action::Hotbar));
        v.extend([
            Action::TargetNext,
            Action::TargetClear,
            Action::Interact,
            Action::ChatOpen,
            Action::OpenInventory,
            Action::OpenSheet,
            Action::OpenTechniques,
            Action::OpenQuests,
            Action::OpenMap,
            Action::OpenSocial,
            Action::OpenCrafting,
            Action::OpenCompanions,
            Action::OpenBestiary,
            Action::Rest,
            Action::OpenHelp,
            Action::OpenOptions,
            Action::ToggleMute,
            Action::ShowNames,
            Action::Menu,
            Action::Screenshot,
        ]);
        v
    }

    /// The stable id used in `keybinds.ron`.
    pub fn id(self) -> String {
        match self {
            Action::MoveUp => "move_up".into(),
            Action::MoveDown => "move_down".into(),
            Action::MoveLeft => "move_left".into(),
            Action::MoveRight => "move_right".into(),
            Action::Stop => "stop".into(),
            Action::Hotbar(i) => format!("hotbar_{}", i + 1),
            Action::TargetNext => "target_next".into(),
            Action::TargetClear => "target_clear".into(),
            Action::Interact => "interact".into(),
            Action::ChatOpen => "chat_open".into(),
            Action::OpenInventory => "open_inventory".into(),
            Action::OpenSheet => "open_sheet".into(),
            Action::OpenTechniques => "open_techniques".into(),
            Action::OpenQuests => "open_quests".into(),
            Action::OpenMap => "open_map".into(),
            Action::OpenSocial => "open_social".into(),
            Action::OpenCrafting => "open_crafting".into(),
            Action::OpenHelp => "open_help".into(),
            Action::OpenOptions => "open_options".into(),
            Action::OpenCompanions => "open_companions".into(),
            Action::OpenBestiary => "open_bestiary".into(),
            Action::Rest => "rest".into(),
            Action::ToggleMute => "toggle_mute".into(),
            Action::ShowNames => "show_names".into(),
            Action::Menu => "menu".into(),
            Action::Screenshot => "screenshot".into(),
        }
    }

    fn default_key(self) -> String {
        match self {
            Action::MoveUp => "KeyW".into(),
            Action::MoveDown => "KeyS".into(),
            Action::MoveLeft => "KeyA".into(),
            Action::MoveRight => "KeyD".into(),
            Action::Stop => "Space".into(),
            Action::Hotbar(i) => format!("Digit{}", (i + 1) % 10),
            Action::TargetNext => "Tab".into(),
            Action::TargetClear | Action::Menu => "Escape".into(),
            Action::Interact => "KeyF".into(),
            Action::ChatOpen => "Enter".into(),
            Action::OpenInventory => "KeyI".into(),
            Action::OpenSheet => "KeyC".into(),
            Action::OpenTechniques => "KeyK".into(),
            Action::OpenQuests => "KeyL".into(),
            Action::OpenMap => "KeyM".into(),
            Action::OpenSocial => "KeyO".into(),
            Action::OpenCrafting => "KeyG".into(),
            Action::OpenHelp => "F1".into(),
            Action::OpenOptions => "F10".into(),
            Action::OpenCompanions => "KeyP".into(),
            Action::OpenBestiary => "KeyJ".into(),
            Action::Rest => "KeyX".into(),
            Action::ToggleMute => "F9".into(),
            Action::ShowNames => "AltLeft".into(),
            Action::Screenshot => "F12".into(),
        }
    }

    fn is_movement(self) -> bool {
        matches!(
            self,
            Action::MoveUp | Action::MoveDown | Action::MoveLeft | Action::MoveRight
        )
    }
}

/// A key with modifiers, as `Shift+Digit1`.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Chord {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: String,
}

impl Chord {
    pub fn parse(text: &str) -> Option<Chord> {
        let mut c = Chord::default();
        let mut parts: Vec<&str> = text.split('+').collect();
        c.key = parts.pop()?.trim().to_owned();
        if c.key.is_empty() {
            return None;
        }
        for m in parts {
            match m.trim() {
                "Ctrl" => c.ctrl = true,
                "Alt" => c.alt = true,
                "Shift" => c.shift = true,
                _ => return None,
            }
        }
        Some(c)
    }
}

#[derive(Deserialize)]
#[serde(default)]
struct KeyFile {
    wasd: bool,
    keys: BTreeMap<String, String>,
}

impl Default for KeyFile {
    fn default() -> Self {
        KeyFile {
            wasd: true,
            keys: BTreeMap::new(),
        }
    }
}

pub struct KeyMap {
    pub wasd: bool,
    by_chord: BTreeMap<Chord, Vec<Action>>,
}

impl Default for KeyMap {
    fn default() -> Self {
        KeyMap::from_overrides(true, &BTreeMap::new())
    }
}

impl KeyMap {
    /// Reads `keybinds.ron`; unknown actions or keys are reported and
    /// skipped, the rest keeps working.
    pub fn load(text: &str) -> (KeyMap, Vec<String>) {
        match ron::from_str::<KeyFile>(text) {
            Ok(f) => {
                let mut problems = Vec::new();
                let known: Vec<String> = Action::all().into_iter().map(Action::id).collect();
                let mut keys = BTreeMap::new();
                for (action, key) in f.keys {
                    if !known.contains(&action) {
                        problems.push(format!("keybinds: unknown action `{action}`"));
                    } else if Chord::parse(&key).is_none() {
                        problems.push(format!("keybinds: `{action}`: bad key `{key}`"));
                    } else {
                        keys.insert(action, key);
                    }
                }
                (KeyMap::from_overrides(f.wasd, &keys), problems)
            }
            Err(e) => (KeyMap::default(), vec![format!("keybinds: {e}")]),
        }
    }

    fn from_overrides(wasd: bool, keys: &BTreeMap<String, String>) -> KeyMap {
        let mut by_chord: BTreeMap<Chord, Vec<Action>> = BTreeMap::new();
        for a in Action::all() {
            if a.is_movement() && !wasd && !keys.contains_key(&a.id()) {
                continue;
            }
            let key = keys.get(&a.id()).cloned().unwrap_or_else(|| a.default_key());
            if let Some(c) = Chord::parse(&key) {
                by_chord.entry(c).or_default().push(a);
            }
        }
        KeyMap { wasd, by_chord }
    }

    /// Actions bound to this chord (Esc is both `TargetClear` and `Menu`).
    pub fn actions(&self, chord: &Chord) -> &[Action] {
        self.by_chord.get(chord).map_or(&[], |v| v.as_slice())
    }

    /// The chord of an action, for labels (`hotbar_1` → `Digit1`).
    pub fn chord_of(&self, action: Action) -> Option<&Chord> {
        self.by_chord
            .iter()
            .find(|(_, v)| v.contains(&action))
            .map(|(c, _)| c)
    }
}
