import { CloseIcon, MenuIcon } from '@/components/icons/icons';
import { pageUrl } from '@/lib/url';
import { cn } from '@/lib/utils';
import { useCallback, useEffect, useRef, useState } from 'react';
import type { FC, PropsWithChildren, ReactNode } from 'react';
import { Brand } from './brand';
import { pages } from './masthead.data';
import type { TPageKey } from './masthead.data';

type Props = PropsWithChildren & {
  // Absent : une page hors du menu (Duel).
  current?: TPageKey;
  showVersion?: boolean;
  status?: ReactNode;
};

const MOBILE = '(max-width: 640px)';

const Masthead: FC<Props> = ({ current, showVersion = true, status, children }) => {
  const [open, setOpen] = useState(false);
  const toggleRef = useRef<HTMLButtonElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const wasOpen = useRef(false);
  const hasChildren = Boolean(children);
  const openMenu = useCallback((): void => setOpen(true), []);
  const closeMenu = useCallback((): void => setOpen(false), []);

  useEffect(() => {
    if (!open) {
      if (wasOpen.current) {
        toggleRef.current?.focus();
      }
      wasOpen.current = false;
      return () => {
        wasOpen.current = false;
      };
    }
    wasOpen.current = true;
    closeRef.current?.focus();
    const main = document.querySelector('main');
    main?.setAttribute('inert', '');
    const mobile = matchMedia(MOBILE);
    const handleChange = (): void => {
      if (!mobile.matches) {
        closeMenu();
      }
    };
    const handleKeyDown = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        closeMenu();
      }
    };
    mobile.addEventListener('change', handleChange);
    document.addEventListener('keydown', handleKeyDown);
    return () => {
      main?.removeAttribute('inert');
      mobile.removeEventListener('change', handleChange);
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [open, closeMenu]);

  return (
    <header className={cn('masthead', open && 'menu-open')}>
      <Brand heading showVersion={showVersion} />
      <button
        ref={toggleRef}
        className="menu-toggle"
        type="button"
        aria-label="Menu"
        aria-expanded={open}
        aria-controls="site-menu"
        inert={open}
        onClick={openMenu}
      >
        <MenuIcon />
      </button>
      <div id="site-menu" className="menu">
        <div className="menu-head">
          <Brand showVersion={showVersion} />
          <button
            ref={closeRef}
            className="menu-toggle"
            type="button"
            aria-label="Fermer le menu"
            onClick={closeMenu}
          >
            <CloseIcon />
          </button>
        </div>
        <nav className="pages" aria-label="Pages">
          {pages.map(({ key, label, href }) => (
            <a key={key} href={pageUrl(href)} aria-current={key === current ? 'page' : undefined}>
              {label}
            </a>
          ))}
        </nav>
        {hasChildren ? <div className="controls">{children}</div> : children}
      </div>
      {status}
    </header>
  );
};

export { Masthead };
