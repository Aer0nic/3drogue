# GRAZZARK

A first-person ASCII roguelike written in Rust. Descend ten randomly generated
dungeon levels, take the **Amulet of GRAZZARK** from its Warden on level 10,
and climb back to the surface alive.

## Build and run

```sh
# one-time: install Rust (https://rustup.rs)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# Linux audio needs the ALSA headers (Debian/antiX: sudo apt install libasound2-dev pkg-config)

cargo run --release
```

Options: `--no-audio`, `--256` (for terminals without 24-bit colour),
`--truecolor`. You need a terminal of at least 80x24; bigger looks better.

## Controls

| First-person | | Overhead map (Tab) | |
|---|---|---|---|
| W / Up | step forward | WASD / arrows | move N/W/S/E |
| S / Down | step back | Y U B N, Home PgUp End PgDn | diagonals |
| A D / Left Right | turn 45 degrees | Q / E | turn |
| Q / E | strafe | | |
| Z / C | diagonal step | | |

Space/F attack (or shoot) · 1-2 class skills · 3-6 Soul Altar skills ·
G pick up · I inventory · P drink a healing potion · `>` `<` Enter use stairs ·
`.` wait/search · **R toggles turn-based / real-time** · Tab/M toggles view ·
`-`/`+` volume · `?` help · Esc menu

## Features

- **Raycast ASCII 3D view**: textured walls with a different style per depth
  band, floor and ceiling casting, coloured flickering torch light, dynamic
  light from spells, and scaled ASCII monster sprites whose eyes glow in the dark.
- **Effects**: particles (blood, bones, sparks, embers, smoke), projectile
  trails, explosions, screen shake, damage flashes, low-health vignette,
  floating damage numbers, an animated held weapon and torch, and a melt
  transition between levels.
- **Turn-based or real-time**: press R at any time to switch.
- **6 classes**: Warrior, Rogue, Mage and Cleric are available from the start.
  Ranger (wolf companion) and Necromancer (raise the dead) are unlocked with soul points.
- **19 monsters in 8 factions that fight each other.** Undead attack the
  living, demons attack everything, greenskins and beasts are at war, and a
  stray bolt that hits an ally starts a brawl.
- **The dead rise again**: corpses come back to life when you are not looking,
  undead fastest of all.
- **Levels persist** for the whole run. Each level has a stairway up and a
  stairway down. Carrying the Amulet sends hunters after you on every level on
  the way back up.
- **Procedural audio** (no sample files): each monster species has its own
  synthesised voice for alerts, attacks, pain and death. There are random
  dungeon ambient sounds and generative music that changes key and mode with
  depth and adds drums when combat starts.
- **Sound propagation**: sounds travel along corridors. A sound around a corner
  is quieter and muffled, seems to come from the corner, and arrives with a
  delayed echo. Reverb depends on the size of the room you are in.
- **Death screens by cause of death**: devoured (blood rain), crushed
  (cracking screen), restless dead (ghost mist), claimed by the abyss (hellfire),
  poisoned (toxic bubbles), impaled (spikes), arcane (rune storm), or burned by
  your own fireball (fire burning down to ash).
- **Persistent profile** (`~/.local/share/grazzark/profile.json`): every run
  earns soul points. Spend them at the Soul Altar on passive upgrades (HP,
  damage, armour, mana, sight/crit, XP, regeneration, starting potions, loot,
  life-on-kill), active skills (Blink, Soul Burst, Phase Ward, Second Wind) and
  class unlocks. The Hall of the Dead records your past runs.
