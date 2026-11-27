import { CloseIcon, MenuIcon } from '@/components/icons/icons';
import { pageUrl } from '@/lib/url';
import { cn } from '@/lib/utils';
import { useEffect, useRef, useState, type FC, type PropsWithChildren, type ReactNode } from 'react';
import { Logo } from './logo';
import { pages, type TPageKey } from './masthead.data';
import { VersionBadge } from './version-badge';

type Props = PropsWithChildren & {
  // Absent : une page hors du menu (Duel).
  current?: TPageKey;
  showVersion?: boolean;
  status?: ReactNode;
};

const MOBILE = '(max-width: 640px)';

const Brand: FC<{ heading?: boolean; showVersion: boolean }> = ({ heading = false, showVersion }) => {
  const Title = heading ? 'h1' : 'p';
  return (
    <div className="brand" aria-hidden={heading ? undefined : true}>
      <Logo />
      <Title className="title">
        Ladder Isolation{' '}
        <span className="title-meta">
          <span className="title-course">LOG200</span>
          {showVersion && <VersionBadge />}
        </span>
      </Title>
    </div>
  );
};

const Masthead: FC<Props> = ({ current, showVersion = true, status, children }) => {
  const [open, setOpen] = useState(false);
  const toggleRef = useRef<HTMLButtonElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const wasOpen = useRef(false);

  useEffect(() => {
    if (!open) {
      if (wasOpen.current) toggleRef.current?.focus();
      wasOpen.current = false;
      return;
    }
    wasOpen.current = true;
    closeRef.current?.focus();
    const main = document.querySelector('main');
    main?.setAttribute('inert', '');
    const mobile = matchMedia(MOBILE);
    const handleChange = () => !mobile.matches && setOpen(false);
    const handleKeyDown = (e: KeyboardEvent) => e.key === 'Escape' && setOpen(false);
    mobile.addEventListener('change', handleChange);
    document.addEventListener('keydown', handleKeyDown);
    return () => {
      main?.removeAttribute('inert');
      mobile.removeEventListener('change', handleChange);
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [open]);

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
        onClick={() => setOpen(true)}
      >
        <MenuIcon />
      </button>
      <div id="site-menu" className="menu">
        <div className="menu-head">
          <Brand showVersion={showVersion} />
          <button ref={closeRef} className="menu-toggle" type="button" aria-label="Fermer le menu" onClick={() => setOpen(false)}>
            <CloseIcon />
          </button>
        </div>
        <nav className="pages" aria-label="Pages">
          {pages.map(({ key, label, href }) => (
            <a
              key={key}
              href={pageUrl(href)}
              aria-current={key === current ? 'page' : undefined}
            >
              {label}
            </a>
          ))}
        </nav>
        {children && <div className="controls">{children}</div>}
      </div>
      {status}
    </header>
  );
};

export { Masthead };
