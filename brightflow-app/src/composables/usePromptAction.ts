import type { PromptOptions } from '@/components/command/paletteActions';
import TextPromptModal from '@/components/command/TextPromptModal.vue';

/**
 * Per-component prompt helper: wraps `useOverlay` + `TextPromptModal` so a
 * context-menu / kebab item can request a single string value without
 * reaching for `window.prompt`. Returns the trimmed value, or `null` when
 * the user cancels (empty input counts as cancel — same contract as the
 * modal).
 *
 * One overlay controller is created per call to `usePromptAction()`; that is
 * fine because overlays mount lazily on `.open()`. Call this at the top of
 * `setup` — `useOverlay` must run during component setup.
 */
export function usePromptAction(): (title: string, opts?: PromptOptions) => Promise<string | null> {
  const overlay = useOverlay();
  const modal = overlay.create(TextPromptModal);

  return async (title, opts) => {
    const result: unknown = await modal.open({ title, ...opts }).result;
    return typeof result === 'string' ? result : null;
  };
}
