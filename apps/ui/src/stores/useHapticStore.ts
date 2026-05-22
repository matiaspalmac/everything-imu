import { create } from "zustand";
import type { HapticConfigDto } from "../api/client";

/**
 * Haptic bridge UI state. Holds two things that must outlive the Haptics
 * page component so navigating away and back does not lose work:
 *
 * - `discovered`: OSC addresses the bridge has reported seeing from VRChat,
 *   so the user can bind a contact parameter by tapping it in-game.
 * - `config`: the draft haptic config. The page edits this in place; it is
 *   only persisted to the backend DB on Save. Keeping it here (instead of
 *   component `useState`) means an unsaved edit survives a tab switch.
 */
type State = {
  discovered: string[];
  add(address: string): void;
  clear(): void;
  /** Draft config, or null until the first backend fetch completes. */
  config: HapticConfigDto | null;
  /** True once the initial fetch has run, so it does not refetch and clobber
   * unsaved edits when the page remounts. */
  configLoaded: boolean;
  setConfig(config: HapticConfigDto): void;
};

const MAX_DISCOVERED = 200;

export const useHapticStore = create<State>((set) => ({
  discovered: [],
  add: (address) =>
    set((s) =>
      s.discovered.includes(address)
        ? s
        : { discovered: [address, ...s.discovered].slice(0, MAX_DISCOVERED) },
    ),
  clear: () => set({ discovered: [] }),
  config: null,
  configLoaded: false,
  setConfig: (config) => set({ config, configLoaded: true }),
}));
