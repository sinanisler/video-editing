import { useEffect, useRef } from "react";
import { ui } from "../store";
import type { MenuItem } from "../store";
import { clamp } from "../util";

/** Opens the right-click menu at a screen point. Closes on a click elsewhere, Escape, or scroll. */
export function openMenu(x: number, y: number, items: MenuItem[]) {
  ui.set({ menu: { x, y, items } });
}

export function ContextMenu() {
  const menu = ui.use((s) => s.menu);
  const box = useRef<HTMLDivElement>(null);
  const open = !!menu;

  useEffect(() => {
    if (!open) return;
    const close = (e: Event) => {
      if (!box.current?.contains(e.target as Node)) ui.set({ menu: null });
    };
    const escape = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      // Only the menu closes; the Escape that would clear the selection stops here.
      e.stopPropagation();
      ui.set({ menu: null });
    };
    window.addEventListener("pointerdown", close, true);
    window.addEventListener("wheel", close, true);
    window.addEventListener("keydown", escape, true);
    return () => {
      window.removeEventListener("pointerdown", close, true);
      window.removeEventListener("wheel", close, true);
      window.removeEventListener("keydown", escape, true);
    };
  }, [open]);

  if (!menu) return null;
  const left = clamp(menu.x, 4, window.innerWidth - 230);
  const top = clamp(menu.y, 4, window.innerHeight - (menu.items.length * 30 + 20));
  // Each choice closes the menu first, so a choice that opens a dialog or starts a rename is not hidden behind it.
  const pick = (run: () => void) => {
    ui.set({ menu: null });
    run();
  };
  return (
    <div ref={box} className="menu" style={{ left, top }} role="menu">
      {menu.items.map((item, i) => {
        if (item === "sep") return <div key={i} className="msep" />;
        if ("swatches" in item) {
          return (
            <div key={i} className="mrow">
              {/* The swatches name themselves, so the row carries no text label. */}
              <span className="swatches" role="group" aria-label={item.label}>
                {item.swatches.map((c, j) => (
                  <i
                    key={j}
                    className={`sw ${c === (item.current ?? null) ? "on" : ""} ${c ? "" : "none"}`}
                    style={c ? { background: c } : undefined}
                    data-tip={c ? `Colour ${c}` : "No colour"}
                    onClick={() => pick(() => item.onPick(c))}
                  />
                ))}
              </span>
            </div>
          );
        }
        return (
          <div
            key={i}
            role="menuitem"
            aria-disabled={item.disabled}
            className={`mitem ${item.disabled ? "off" : ""} ${item.danger ? "danger" : ""}`}
            onClick={() => !item.disabled && pick(item.action)}
          >
            <span className="mcheck">{item.checked ? "✓" : ""}</span>
            <span className="mlabel">{item.label}</span>
            {item.hint && <kbd>{item.hint}</kbd>}
          </div>
        );
      })}
    </div>
  );
}
