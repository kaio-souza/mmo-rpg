//! Sound events: the vocabulary the client fires and
//! `assets/sounds/sounds.ron` binds to files. Here, and not in `defs`, so
//! the client names them without depending on `defs` (like [`crate::Locale`]).

/// Something the client may play a sound for. A new event is a new line
/// here (and its English label in the editor); which file plays is data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SoundEvent {
    /// A blow of the player, or at the player's target, that hit.
    CombatHit,
    /// Same, a critical hit.
    CombatCrit,
    /// Same, a miss.
    CombatMiss,
    /// The player took damage.
    CombatHurt,
    /// A click on the interface that did something.
    UiClick,
    /// A refusal or an error message.
    UiError,
    /// A System message (level, technique, title, quest).
    SystemNotice,
    /// A technique used, when `sounds.ron` has no line for that technique
    TechniqueUse,
}

impl SoundEvent {
    pub const ALL: [SoundEvent; 8] = [
        SoundEvent::CombatHit,
        SoundEvent::CombatCrit,
        SoundEvent::CombatMiss,
        SoundEvent::CombatHurt,
        SoundEvent::UiClick,
        SoundEvent::UiError,
        SoundEvent::SystemNotice,
        SoundEvent::TechniqueUse,
    ];

    /// The name used in `sounds.ron`.
    pub fn id(self) -> &'static str {
        match self {
            SoundEvent::CombatHit => "combat_hit",
            SoundEvent::CombatCrit => "combat_crit",
            SoundEvent::CombatMiss => "combat_miss",
            SoundEvent::CombatHurt => "combat_hurt",
            SoundEvent::UiClick => "ui_click",
            SoundEvent::UiError => "ui_error",
            SoundEvent::SystemNotice => "system_notice",
            SoundEvent::TechniqueUse => "technique_use",
        }
    }

    pub fn from_id(id: &str) -> Option<SoundEvent> {
        SoundEvent::ALL.into_iter().find(|e| e.id() == id)
    }

    /// Interface and System sounds are not placed in the world
    /// the rest may be.
    pub fn interface(self) -> bool {
        matches!(
            self,
            SoundEvent::UiClick | SoundEvent::UiError | SoundEvent::SystemNotice
        )
    }
}
