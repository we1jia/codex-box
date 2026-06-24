import type { MouseEvent as ReactMouseEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

const INTERACTIVE_SELECTOR =
  'button, a, input, textarea, select, [role="button"], [data-no-window-drag]';

export function startWindowDragFromMouseEvent(
  event: ReactMouseEvent<HTMLElement>
) {
  if (event.button !== 0) return;

  const target = event.target;
  if (
    target instanceof HTMLElement &&
    target.closest(INTERACTIVE_SELECTOR)
  ) {
    return;
  }

  event.preventDefault();
  void getCurrentWindow().startDragging().catch(() => {
    // Browser preview has no native window to drag.
  });
}
