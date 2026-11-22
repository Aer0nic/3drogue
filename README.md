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

**Graphics detail** (`--detail=auto|high|low|minimal`, or *Detail* in the Esc
menu; saved per profile). The 3D view is ray cast on all your CPU cores. In
*Auto* the game times every frame and drops a level if the machine (or the
terminal) can't keep up, then climbs back when there's headroom. *Minimal* is
bare bones for very slow computers: half-resolution casting, plain floor and
ceiling lighting, no particles, rounded colours so the terminal redraws less,
and 20 frames a second. The game draws text, so the graphics card is used by
your terminal emulator, not the game: a GPU-accelerated terminal (kitty,
Alacritty, WezTerm, Ghostty) helps most at large sizes.

## Controls

| First-person | | Overhead view (Shift+M) | |
|---|---|---|---|
| W / Up | step forward | WASD / arrows | move N/W/S/E |
| S / Down | step back | Y U B N, Home PgUp End PgDn | diagonals |
| A D / Left Right | turn 45 degrees | Q / E | turn |
| Q / E | strafe | | |
| Z / C | diagonal step | | |

Space/F attack (or shoot) · 1-2 class skills · 3-4 class skills bought at the Soul Altar ·
G pick up · I inventory (backtick `` ` `` sorts it by type) · P drink a healing potion · `>` `<` Enter use stairs ·
`.` wait/search · **Shift+R toggles turn-based / real-time** · Shift+M switches first-person / overhead view · Shift+C toggles Last Rites · Shift+Q listens intently (Keen Hearing) · Tab (or M) full map · L message log (last 100 turns) · T relight your torch ·
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
- **32 kinds of monster in 8 factions that fight each other.** Undead attack the
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
- **Profiles**: several players (or several attempts) can each keep their own
  profile, with its own soul points, upgrades, bestiary, history and saved run.
  The first time you play you name one; with more than one, the game asks
  which to load when it starts. "Change Profile" and "New Profile" are on the
  title menu.
- **Persistent progress**: every run earns soul points. Spend them at the Soul Altar on stat upgrades, boons that
  change how a run plays (Bloodlust, Soul Anchor, Blood Magic, Cartographer, Glinting Eye (items on the minimap),
  Last Rites (burn or shatter your kills so they never rise), Appraiser's Eye
  (better gear marked in your pack), Eager Gulp (drink Might and Vigor on the spot),
  Potion Belt (stack potions of a kind in one pack slot), Keen Hearing (hear further;
  then listen with Shift+Q to see where hidden sounds come from),
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
- **Necromancer thralls**: when several corpses are in reach, Raise Dead opens
  a list showing each body's max HP upkeep and whatever the bestiary has taught
  you about it. Stronger monsters hold more of your max HP while they live (less
  every 10 levels). A thrall that falls can never be raised again, unless you
  learn Undying Bond at the Soul Altar, which allows one more raising at about
  50% more upkeep. Each kill gives a thrall a 5% chance to become a champion
  (+2% per 5 levels), and a champion thrall that slays an enemy champion may
  become an overlord thrall (1.5%, +0.5% per 5 levels). Overlord thralls roar
  foes into flight and raise short-lived skeleton minions, each on a 25-50 turn
  cooldown. Thralls heal a little slower than you.
- **Every monster has a special power on a timer**: minotaurs and cave bears
  charge and hurl you back, wraiths sap your life, myconids burst into
  paralysing spore clouds, spiders web you in place, zombies and wights grab
  you, slimes split, shamans heal their kin, ogres and the Warden shake the
  ground, trolls throw boulders, basilisks petrify with a gaze, banshees shriek,
  goblins throw grit in your eyes, rats and wolves call their pack, and more.
  Monsters use these powers on each other when they fight, too. Paralysis
  always leaves a short grace period, so it can never chain.
- **15 floor themes** dealt out per run. Near the surface (depths 1-3): cellars,
  crypts, dank gaols and sewers. The middle depths (4-6): mossy halls, frozen
  catacombs, fungal grottos, a drowned temple and old mines. The deep (7-9):
  obsidian vaults, crystal caverns, an ossuary, the starless abyss, a blood
  temple and an infernal foundry. Then the bone-walled Sanctum.
- **The Warden and its wards**: Vorth guards the Amulet and nothing else. It
  hunts only you, and no monster but a necromancer's thralls can harm it. Four
  ward prisms feed its power, one somewhere on floors 1-3, 4-5, 6-8 and 9-10:
  floating prisms pouring shrieking souls up into the ceiling. Strike one and
  a Prism Guardian (a crystal golem that throws shards) tears itself out of it
  and seals the room behind walls of crystal. The ward is shielded until the
  Guardian falls; then it crumbles, and something far below screams. Each
  broken ward weakens the Warden, and with all four gone it is a hollow shell.
  Taking the Amulet shatters any wards still standing.
- **Themed doors**: every theme has its own doors: moss-eaten planks, slabs of
  obsidian split by fire, bone doors, frozen doors, crystal, woven roots,
  bronze doors with a porthole, braced timber, lashed bones, swirling void,
  gilded crimson. Sewers, gaols, crypts and foundries have gates of bars you
  can see (and spot monsters) through.
- **Special rooms**: pillared halls, rubble-strewn halls, walled inner chambers
  with treasure behind the door, ossuaries whose bones rise, armories,
  libraries, candlelit shrines, trapped galleries with a prize, beasts' dens and
  spore gardens.
- **The legend** (title menu, and before your first descent): why the Amulet,
  the heart of GRAZZARK, must be carried into the sunlight.
- **The living dungeon**: now and then a few of the bone faces carved into the
  Sanctum's bricks come alive in silent screams. Take the Amulet and they shriek
  in torment, more often, and some die for good, weeping blood from closed eyes and lips. On the
  climb back up, flesh, veins and watching eyes spread over the dungeon, thicker
  on every floor nearer the surface. It also torments you with curses that
  last a few turns: Forgetting (your map is wiped until it passes), Paranoia
  (phantom monsters that can't hurt you and vanish when touched), Darkness,
  Vertigo (steps lurch the wrong way), Leaden limbs (40% slower), Hush (no
  skills or scrolls) and Gale (an unnatural wind snuffs your torch: total
  darkness and no map until the wind dies and you relight it with T).
- **Torch smoke**: the torch in your left hand trails smoke that curls, drifts
  as you turn, streams back as you walk, and is ripped sideways by a gale.
- **New monsters**, including centipedes, slimes, myconids, goblin shamans,
  scorpions, hobgoblins, cave bears, gargoyles, wights, banshees, fire
  elementals, basilisks and pit fiends. Each floor favours the creatures at
  home there, so weak ones crowd the upper floors and strong ones the depths.

## Standalone builds

`scripts/build-release.sh` builds ready-to-share packages into `dist/`:

- `grazzark-linux-x86_64.tar.gz`: the game, a menu entry (`install-linux.sh`)
  and instructions. Needs glibc 2.34 or newer (Ubuntu 22.04, Debian 12,
  Fedora 35 and later) and ALSA (`libasound2`), which every desktop has.
- `grazzark-windows-x86_64.zip`: `grazzark.exe` and instructions. Windows 10
  or newer. Built from Linux with `rustup target add x86_64-pc-windows-gnu`
  and MinGW-w64 (`apt install mingw-w64`); on Windows itself just run
  `cargo build --release`.

How it starts:

- **Run from a terminal / Command Prompt**: it plays right there.
- **Double-clicked on Windows**: it is a console program, so Windows opens a
  console window for it. If it ever crashes, the window waits for Enter so
  you can read the message.
- **Double-clicked on Linux**: with no terminal attached, it reopens itself
  in the first terminal emulator it finds (x-terminal-emulator,
  gnome-terminal, konsole, xfce4-terminal, mate-terminal, tilix, kitty,
  alacritty, wezterm, foot or xterm).

Profiles live in `%APPDATA%\grazzark\profiles` on Windows and
`~/.local/share/grazzark/profiles` on Linux, one folder each. A profile from
an older version is moved there automatically as "Default".
