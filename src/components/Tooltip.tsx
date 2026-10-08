import { useEffect, useLayoutEffect, useRef, useState } from "react";

/** How long the pointer rests on a control before its tip appears. */
const DELAY_MS = 400;

interface Tip {
  text: string;
  x: number;
  y: number;
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
        // Above the control, unless that would run off the top of the window.
        const below = r.top < 90;
        setTip({ text, x: r.left + r.width / 2, y: below ? r.bottom + 8 : r.top - 8, below });
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

  // Keep the box inside the window horizontally, whatever the control's position.
  useLayoutEffect(() => {
    if (!tip || !box.current) return;
    const half = box.current.offsetWidth / 2;
    setLeft(Math.min(Math.max(tip.x, half + 8), window.innerWidth - half - 8));
  }, [tip]);

  if (!tip) return null;
  return (
    <div
      ref={box}
      className={`tip ${tip.below ? "below" : "above"}`}
      role="tooltip"
      style={{ left, top: tip.y }}
    >
      {tip.text}
    </div>
  );
}
