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

Space/F attack (or shoot) · 1-2 class skills · 3-4 class skills bought at the Soul Altar ·
G pick up · I inventory · P drink a healing potion · `>` `<` Enter use stairs ·
`.` wait/search · **Shift+R toggles turn-based / real-time** · Tab toggles view · M full map ·
`-`/`+` volume · `?` help · Esc menu

## Features

- **Raycast ASCII 3D view**: textured walls with a different style per depth
  band, floor and ceiling casting, coloured flickering torch light, dynamic
  light from spells, and scaled ASCII monster sprites whose eyes glow in the dark.
- **Effects**: particles (blood, bones, sparks, embers, smoke), projectile
  trails, explosions, screen shake, damage flashes, low-health vignette,
  floating damage numbers, an animated held weapon and torch, and a melt
  transition between levels.
- **Turn-based or real-time**: press Shift+R at any time to switch.
- **Save and continue**: quitting mid-run saves it; choose Continue Descent on the title screen to resume.
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
  earns soul points. Spend them at the Soul Altar on stat upgrades, boons that
  change how a run plays (Bloodlust, Soul Anchor, Blood Magic, Cartographer,
  Deep Pockets and more), class unlocks, and two extra skills for each class.
  The Hall of the Dead records your past runs.
- **Difficulties**: Normal, Hard, Difficult and Chaotic. Escaping with the
  Amulet unlocks the next one. Higher difficulties give monsters more health and
  damage, and multiply the soul points you earn. Normal pays 20% fewer.
- **Bestiary**: every monster you slay is recorded across runs. 1 kill reveals
  its name, 10 its basic stats, 100 its lore, haunts and full stats. At 200 kills
  you master it: +1.5% crit chance and +2% crit damage against it in every run.
  Uniques and the Warden need far fewer kills: 1 name, 3 stats, 5 lore,
  7 full stats, 10 mastered.
- **Monster closets**: some floors hide a big room packed with sleeping
  monsters, sometimes with champions or even an overlord among them.
- **Necromancer thralls grow**: each kill gives a thrall a 5% chance to become
  a champion (+2% per 5 levels). A champion thrall that slays an enemy champion
  may become an overlord thrall (1.5%, +0.5% per 5 levels), which roars foes into
  flight and raises short-lived skeleton minions, each on a 25-50 turn cooldown.
  Every thrall binds 3 of your max HP (2 from level 10, 1 from level 20) until it
  dies, and they heal a little slower than you.
- **Shuffled floor themes**: each run deals three of five wall styles (brick
  cellars, mossy halls, obsidian vaults, frozen catacombs, crystal caverns) in
  random order, in bands of 2-4 floors, before the bone-walled Sanctum.
- **The living dungeon**: faces surface in the Sanctum's walls in silent
  screams. Take the Amulet and they stare in terror, or hang dead. On the
  climb back up, flesh, veins and watching eyes spread over the dungeon, thicker
  on every floor nearer the surface. It also torments you with curses that
  last a few turns: Forgetting (your map is wiped until it passes), Paranoia
  (phantom monsters that can't hurt you and vanish when touched), Darkness,
  Vertigo (steps lurch the wrong way), Leaden limbs (40% slower) and Hush (no
  skills or scrolls).
