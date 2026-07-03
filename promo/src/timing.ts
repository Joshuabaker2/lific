/**
 * Beat sheet in frames (30 fps). TransitionSeries overlaps scenes by the
 * transition duration, so total = sum(scenes) - sum(transitions).
 *
 * Research-derived structure (see Lific page LIF-DOC-17):
 *  cold open pain -> agitate (Jira / Linear / FOSS group) -> reveal ->
 *  terminal demo -> UI demo -> agent/MCP demo -> proof -> single CTA.
 */
export const TRANSITION = 12;

/*
 * Grid-locked to music.wav: 130 BPM, 32 bars, 59.077s (beat = 13.846
 * frames). Every transition midpoint sits on a beat; the track's drops
 * land on the reveal (bar 9, frame 443) and the kanban card grab
 * (bar 17, frame 886). Total = 1772 frames.
 */
export const SCENES = {
  hook: 96,
  jira: 74,
  linear: 81,
  foss: 234,
  reveal: 123,
  terminal: 275,
  ui: 234,
  agent: 219,
  teams: 261,
  cta: 283,
} as const;

const durations = Object.values(SCENES);
export const TOTAL_FRAMES =
  durations.reduce((a, b) => a + b, 0) - (durations.length - 1) * TRANSITION;
