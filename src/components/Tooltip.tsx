import { useEffect, useLayoutEffect, useRef, useState } from "react";

/** How long the pointer rests on a control before its tip appears. */
const DELAY_MS = 400;

interface Tip {
  text: string;
  x: number;
  /** The control's top and bottom edges. The tip sits above the top, or below the bottom. */
  top: number;
  bottom: number;
  below: boolean;
}

/**
 * One tooltip for the whole window. Any element with a `data-tip` attribute shows its text after a short rest.
 * Newlines in the text become line breaks. It hides as soon as the pointer is pressed, a key is pressed, or the page scrolls.
 */
export function Tooltip() {
  const [tip, setTip] = useState<Tip | null>(null);
  const box = useRef<HTMLDivElement>(null);
  const [left, setLeft] = useState(0);

  useEffect(() => {
    let timer = 0;
    let current: HTMLElement | null = null;
    const hide = () => {
      window.clearTimeout(timer);
      current = null;
      setTip(null);
    };
    const onOver = (e: PointerEvent) => {
      const el = (e.target as HTMLElement | null)?.closest<HTMLElement>("[data-tip]") ?? null;
      if (el === current) return;
      hide();
      if (!el) return;
      current = el;
      timer = window.setTimeout(() => {
        const text = el.dataset.tip;
        if (!text || !el.isConnected) return;
        const r = el.getBoundingClientRect();
        setTip({ text, x: r.left + r.width / 2, top: r.top, bottom: r.bottom, below: false });
      }, DELAY_MS);
    };
    document.addEventListener("pointerover", onOver);
    document.addEventListener("pointerdown", hide, true);
    document.addEventListener("keydown", hide, true);
    document.addEventListener("wheel", hide, true);
    document.addEventListener("scroll", hide, true);
    return () => {
      hide();
      document.removeEventListener("pointerover", onOver);
      document.removeEventListener("pointerdown", hide, true);
      document.removeEventListener("keydown", hide, true);
      document.removeEventListener("wheel", hide, true);
      document.removeEventListener("scroll", hide, true);
    };
  }, []);

  // Keep the box inside the window horizontally, whatever the control's position. And above the control when it
  // fits there; a long tip next to the top of the window goes below instead of being cut off.
  useLayoutEffect(() => {
    if (!tip || !box.current) return;
    const half = box.current.offsetWidth / 2;
    setLeft(Math.min(Math.max(tip.x, half + 8), window.innerWidth - half - 8));
    if (!tip.below && tip.top - 8 - box.current.offsetHeight < 0) setTip({ ...tip, below: true });
  }, [tip]);

  if (!tip) return null;
  return (
    <div
      ref={box}
      className={`tip ${tip.below ? "below" : "above"}`}
      role="tooltip"
      style={{ left, top: tip.below ? tip.bottom + 8 : tip.top - 8 }}
    >
      {tip.text}
    </div>
  );
}
