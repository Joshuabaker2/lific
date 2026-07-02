// LIF-245 — shared "is the command palette open" flag. CommandPalette.svelte
// owns the real UI state (query, mode, results, catalog); this tiny mirror
// exists solely so lib/shortcuts.ts's `shortcutsSuppressed()` can check
// whether the palette is open without importing the component itself (and
// without CommandPalette needing to know anything about the shortcuts
// registry). Same "module singleton mirrored into a dumb flag" shape as
// peek.svelte.ts.

class CommandPaletteOpenState {
  open = $state(false);
}

export const commandPaletteState = new CommandPaletteOpenState();
