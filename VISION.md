# Ring World: Vision

_The living direction doc. FEATURES.md tracks what shipped; this tracks where we're going and why._

## The pitch

**Quiet megastructure survival.** You are one small person living on the inner
surface of a ring the size of a planetary orbit. The horizon curves up. The
far side of the world hangs overhead as an arch in the sky. The sun never
moves; night is an engineered thing, a shadow panel sliding silently across
the land. Survival mechanics give you something to do; the ring gives you a
reason to feel something while doing it.

The reference feelings: the first screenshot-pause in Outer Wilds, standing
still in Shadow of the Colossus, the arch of Halo's original title screen.
Minecraft is the mechanical skeleton, not the soul.

## Design pillars

1. **The ring is the main character.** Every system should remind you where
   you are. Light, sky, fog, navigation, weather, landmarks: if a feature
   would play identically on a flat infinite world, ask how the ring can own
   it instead.
2. **Awe through scale and silence.** Sparse, lonely, deliberate. Prefer one
   striking thing on the horizon over ten busy things nearby. Sound and music
   stay minimal; night feels engineered, not spooky-forest.
3. **Survival with weight, not grind.** Danger is legible (night pressure,
   depth pressure), progress is tangible (tools, light, shelter), nothing is
   a timer for its own sake.
4. **Simulation honesty.** The world is geometrically real: true curved
   meshing, a physically consistent day/night mechanism, seamless
   circumnavigation. If you walk far enough, you come home. That promise is
   sacred.

## Where we are (2026-08-11)

Playable core loop: generate, explore, mine, craft, build, fight/avoid mobs
at night, persist between sessions. The ring identity is real: curved chunks,
eternal-noon sun with orbiting shadow squares, per-fragment terminator, stars
at night, seamless world seam, arch overhead.

## Roadmap

### Near (polish the feel)
- [x] Mob models: composite box bodies (head/body/legs), facing, walk swing
- [x] Mob movement: feet-level collision + real step-up hops (no more
      teleport-snap glitching on slopes)
- [x] Terminator warmth: dusk/dawn amber band as a shadow-square edge sweeps
- [ ] Block-break/place particles + sounds
- [ ] Footstep audio tied to block type; ambient wind bed
- [ ] Held-item rendering (see what you're holding)
- [ ] Damage feedback: hit flash on mobs, screen nudge on player hurt

### Mid (make the ring legible)
- [ ] Rim walls: mountains at the axial world edges instead of a cliff into
      space (Niven's rim walls); make the world feel bounded by design
- [ ] Arch landmarks: biome colors visible on the overhead arch matching real
      terrain at that theta, so the sky is a map
- [ ] Shadow-square anticipation: watch the next panel approaching along the
      ring; plan your day around it
- [ ] Simple compass/position HUD in ring coordinates (theta as clock time)

### Far (reasons to travel)
- [ ] Scattered structures worth walking to (ruins, observation towers)
- [ ] Biome-specific resources so circumnavigation has economic pull
- [ ] Weather bands at fixed latitudes (rain belt, dust belt)
- [ ] A "spine" transit system along the ring: build rails, shrink the world

## Art direction notes

- Palette: warm terrain under white sun, cold blue-black space, amber
  terminator. Night is navy + starfield, never pure black.
- Silhouettes over detail: mobs and structures should read at 100 blocks.
- The arch overhead is the wallpaper of every screenshot; protect its
  visibility (fog must never fully eat it).
