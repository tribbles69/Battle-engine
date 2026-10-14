# Roadmap

Two parts: **what Fylking should eventually do**, grouped by area, and **the
order to build it in**. The order matters more than the list. Each stage
should leave the game playable and fun to test, and should build on the stages
before it rather than forcing rewrites.

Legend: ✅ done · 🟡 partly there · ⬜ not started

---

## Part 1: Feature list

### Core simulation
- ✅ Deterministic fixed-step sim, separate from graphics
- ✅ Soldiers in formation slots, re-slotting on turns
- ✅ Individual melee: attack/defence/armour/damage, reach, swing timers
- ✅ Charges: bonus window, impact shove, cavalry knockdown, braced spears
- ✅ Morale: casualties, shock, flank/rear, charges, nearby routs, fatigue
- ✅ Routing, rallying, shattering
- ✅ Stamina
- ⬜ Order recording and replays (falls out of determinism)
- ⬜ Performance target: 10,000 soldiers at 60 fps on a mid-range PC
- ⬜ Proper wheeling: formations pivot on a flank instead of swinging
- ⬜ Unit stances: guard (hold formation in melee), skirmish (keep distance), fire at will

### Terrain
- ⬜ Heightmap ground with slopes
- ⬜ Height advantage: uphill fighters get a bonus, downhill charges hit harder
- ⬜ Slopes and rough ground slow movement and tire men
- ⬜ Forests: slow, break formation, hide units, block arrows and charges
- ⬜ Rivers and fords: crossing penalties, bridge chokepoints
- ⬜ Line of sight and hidden units

### Ranged combat
- ⬜ Projectiles with ballistic arcs and travel time
- ⬜ Archers (arcing fire), guns/teppo (flat, short range, morale shock), javelins
- ⬜ Ammunition
- ⬜ Accuracy vs range, shooting into melee hits friends
- ⬜ Shields and facing reduce missile damage
- ⬜ Missile cover from terrain

### Command
- ⬜ General's unit: morale aura, death causes army-wide shock
- ⬜ General abilities: rally, inspire
- ⬜ Unit groups (lock several units together, move as one)
- ⬜ Battle time limit and reinforcements
- 🟡 Orders: move, attack, run, halt, formation drag. Missing: queued waypoints, retreat order

### Battle AI
- 🟡 Infantry advance, cavalry waits then flanks
- ⬜ Army-level plan: hold a line, pick a weak flank, keep a reserve
- ⬜ Uses terrain (holds hills, avoids forests with cavalry)
- ⬜ Ranged AI: keep distance, focus fire, pull back
- ⬜ Defends its own flanks
- ⬜ Difficulty levels (better decisions, not cheating stats)

### Battle flow
- 🟡 Scenarios from files
- ⬜ Deployment phase with deployment zones
- ⬜ Custom battle screen: pick armies, map, funds
- ⬜ Post-battle results: kills, losses, heroic victory/defeat

### Presentation
- ✅ Primitive-shape soldiers, banners showing morale, unit cards, event feed
- ⬜ Real models (glTF) and skeletal animation driven by the sim's swing/hit/death timers
- ⬜ Level of detail and instancing for huge battles
- ⬜ Sound: clash, shouts, horns, footsteps, ambient
- ⬜ Unit info panel and tooltips (stats, modifiers on morale)
- ⬜ Minimap
- ⬜ Better ground: textures, grass, scattered rocks
- ⬜ Settings menu: graphics, key binding, volume

### Content
- 🟡 Three unit types (yari ashigaru, katana samurai, yari cavalry)
- ⬜ A full Sengoku roster: bows, guns, naginata, heavy cavalry, warrior monks
- ⬜ Factions with unique units and bonuses
- ⬜ More maps and scenarios, a few historical battles

### Modding
- ✅ Units and scenarios as TOML, reload with F5
- ⬜ Mod folders that layer over the base game (load order, overrides)
- ⬜ Models, textures and sounds loadable from mods
- ⬜ Scripted scenarios (triggers, objectives) via a small scripting language
- ⬜ Mod browser / easy sharing

### Sieges
- ⬜ Walls, gates, towers that shoot
- ⬜ Ladders, rams, breaking gates
- ⬜ Fighting on walls and in streets; capture points

### Multiplayer
- ⬜ Lockstep networking (send orders only, thanks to determinism)
- ⬜ Desync detection
- ⬜ Lobby and matchmaking

### Campaign
- ⬜ Map with provinces, settlements, roads
- ⬜ Turns, army movement, recruitment, upkeep
- ⬜ Economy and buildings
- ⬜ Diplomacy and factions
- ⬜ Campaign armies fight in the battle engine, results return to the map

---

## Part 2: Build order

Each stage lists what goes in and why it comes at that point.

### Stage 0: Prototype ✅
Flat field, three unit types, formations, melee, charges, morale, simple AI,
3D view and controls. Proves the core loop is fun and the architecture holds.

### Stage 1: Solid foundations
- Order recording and replay
- Performance pass to 10,000 soldiers (profile, then tune grid and plans)
- Unit stances (guard, skirmish)
- Wheeling formations
- Unit info panel with morale breakdown

*Why first:* everything later gets tested against bigger battles, and replays
let you reproduce bugs and check balance changes. Cheap now, painful later.

### Stage 2: Terrain
- Heightmap, slopes, height advantage
- Forests and rough ground
- A couple of maps with hills and woods

*Why before ranged:* missiles need height (range bonus), cover (forests) and
line of sight. Building terrain first means archers are designed once.

### Stage 3: Ranged combat
- Projectiles, archers, guns, javelins, ammo
- Fire at will, friendly fire, shields
- AI that uses ranged units

*Why here:* the single biggest change to how battles play. With terrain in
place, it's done properly the first time.

### Stage 4: Command and AI v2
- General's unit, aura, death shock, rally
- Unit groups and queued orders
- Army-level AI: battle line, reserves, flank defence, uses hills

*Why here:* once terrain and missiles exist, the AI has enough tools to be
worth making smarter. Doing AI earlier means rewriting it.

### Stage 5: A complete battle game
- Deployment phase
- Custom battle screen
- Post-battle results
- Full Sengoku roster and two or three factions
- Sound

*Milestone:* at this point Fylking is a standalone custom-battle game people
can download and enjoy. A good time for a first public release.

### Stage 6: Modding proper
- Mod folders with load order
- Custom models, textures and sounds from mods
- Scenario scripting

*Why here:* the data formats will have settled through stages 1-5, so mods
won't break every update. Releasing to modders here grows the content.

### Stage 7: Presentation
- glTF models and animation
- LOD for huge battles
- Ground textures, minimap, settings menu

*Why this late:* art is the most expensive thing to redo, so it waits until
the gameplay it shows is stable. Modders may also have started contributing.

### Stage 8: Multiplayer
- Lockstep netcode, desync checks, lobby

*Why here:* needs a stable, deterministic sim and finished battle flow. The
sim is deterministic already, so this is mostly plumbing.

### Stage 9: Sieges
Walls, gates, towers, ladders, street fighting. Big and self-contained;
needs terrain, ranged and good AI first.

### Stage 10: Campaign
The second game. Map, turns, economy, diplomacy, with battles fought in the
engine. Starts only when battles are finished enough to carry it.

---

## Next up

Stage 1, starting with **order recording and replay**: it's small, and it
makes every later stage easier to test and debug.
