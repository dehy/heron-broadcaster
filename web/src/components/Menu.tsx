// SPDX-License-Identifier: AGPL-3.0-or-later

// A "…" menu of actions: Enter or Space opens it, the arrows move between
// its items, Escape or a click outside closes it.

import { type KeyboardEvent, type ReactNode, useEffect, useId, useRef, useState } from "react";
import { t } from "../i18n";

export interface MenuItem {
  label: ReactNode;
  onSelect: () => void;
  disabled?: boolean;
  /** Why it is disabled, or what it does. */
  title?: string | undefined;
  danger?: boolean;
}

export function Menu({ items, label = t("ui.moreActions") }: { items: MenuItem[]; label?: string }) {
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const id = useId();

  const entries = () => [...(list.current?.querySelectorAll<HTMLButtonElement>("[role=menuitem]:not(:disabled)") ?? [])];
  const close = (focus = true) => {
    setOpen(false);
    if (focus) {
      button.current?.focus();
    }
  };

  useEffect(() => {
    if (!open) {
      return;
    }
    entries()[0]?.focus();
    const outside = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!list.current?.contains(target) && !button.current?.contains(target)) {
        setOpen(false);
      }
    };
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, [open]);

  const keys = (event: KeyboardEvent<HTMLDivElement>) => {
    const all = entries();
    const at = all.indexOf(document.activeElement as HTMLButtonElement);
    if (event.key === "Escape") {
      event.preventDefault();
      close();
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      all[(at + step + all.length) % all.length]?.focus();
    } else if (event.key === "Home" || event.key === "End") {
      event.preventDefault();
      (event.key === "Home" ? all[0] : all[all.length - 1])?.focus();
    } else if (event.key === "Tab") {
      close(false);
    }
  };

  return (
    <span className="menu">
      <button
        ref={button}
        type="button"
        className="btn small"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        aria-label={label}
        title={label}
        onClick={() => setOpen((current) => !current)}
        onKeyDown={(event) => {
          if (event.key === "ArrowDown" && !open) {
            event.preventDefault();
            setOpen(true);
          }
        }}
      >
        …
      </button>
      {open && (
        <div ref={list} id={id} className="menu-list" role="menu" aria-label={label} onKeyDown={keys}>
          {items.map((item, index) => (
            <button
              // Items are fixed for a given menu.
              // biome-ignore lint/suspicious/noArrayIndexKey: the items keep their order
              key={index}
              type="button"
              role="menuitem"
              className={item.danger ? "danger" : undefined}
              disabled={item.disabled}
              title={item.title}
              onClick={() => {
                close();
                item.onSelect();
              }}
            >
              {item.label}
            </button>
          ))}
        </div>
      )}
    </span>
  );
}
