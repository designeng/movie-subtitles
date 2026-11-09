/** Platform conventions: modifier keys and file manager naming. */

export const isMac = /Mac/i.test(navigator.platform || navigator.userAgent);
export const isWindows = /Win/i.test(navigator.platform || navigator.userAgent);

/** The primary shortcut modifier: ⌘ on macOS, Ctrl elsewhere. */
export const modKey = isMac ? "⌘" : "Ctrl";

/** True when the platform's primary modifier is held. */
export function hasMod(e: KeyboardEvent | MouseEvent): boolean {
  return isMac ? e.metaKey : e.ctrlKey;
}

/** "Show in Finder" / "Show in Explorer" / "Show in Folder". */
export const revealLabel = isMac ? "Show in Finder" : isWindows ? "Show in Explorer" : "Show in Folder";

/** Keys of a shortcut as shown to the user, e.g. ["⌘", ","] or ["Ctrl", ","]. */
export function shortcut(...keys: string[]): string[] {
  return keys.map((k) => (k === "Mod" ? modKey : k));
}
