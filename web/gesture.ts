// Pointer drags on one element. A press asks `start` what it begins; the
// answer gets every move until the button is let go, and is told if the drag
// is called off instead — by Escape, or by the browser taking the pointer.
// The pointer is captured, so a drag keeps going outside the element.

export interface Gesture {
  move?(e: PointerEvent): void;
  /** The button was let go. */
  end?(e: PointerEvent): void;
  /** The drag was called off: nothing it showed should stay. */
  cancel?(): void;
}

/**
 * Routes presses on `el` to the gesture `start` returns for them; a press it
 * returns null for is a click, handled there and then. Moves with no button
 * down go to `hover`.
 */
export function gestures(el: HTMLElement, start: (e: PointerEvent) => Gesture | null, hover?: (e: PointerEvent) => void): void {
  let active: { gesture: Gesture; pointer: number } | null = null;

  const finish = () => {
    const a = active;
    active = null;
    if (a && el.hasPointerCapture(a.pointer)) el.releasePointerCapture(a.pointer);
    return a?.gesture;
  };

  el.addEventListener("pointerdown", (e) => {
    if (active) return;
    const gesture = start(e);
    if (!gesture) return;
    active = { gesture, pointer: e.pointerId };
    try {
      el.setPointerCapture(e.pointerId);
    } catch {
      // A pointer the browser no longer counts as down: the drag still runs while over `el`.
    }
  });
  el.addEventListener("pointermove", (e) => {
    if (active) active.gesture.move?.(e);
    else hover?.(e);
  });
  el.addEventListener("pointerup", (e) => {
    if (active?.pointer === e.pointerId) finish()?.end?.(e);
  });
  for (const type of ["pointercancel", "lostpointercapture"] as const) {
    el.addEventListener(type, () => finish()?.cancel?.());
  }
  // Escape calls off a drag before anything else sees the key.
  window.addEventListener(
    "keydown",
    (e) => {
      if (!active || e.key !== "Escape") return;
      e.preventDefault();
      e.stopImmediatePropagation();
      finish()?.cancel?.();
    },
    true,
  );
}
