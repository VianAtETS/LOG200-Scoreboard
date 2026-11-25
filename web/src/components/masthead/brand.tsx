import type { FC } from 'react';
import { Logo } from './logo';
import { VersionBadge } from './version-badge';

const Brand: FC<{ heading?: boolean; showVersion: boolean }> = ({
  heading = false,
  showVersion,
}) => {
  const title = (
    <>
      Ladder Isolation{' '}
      <span className="title-meta">
        <span className="title-course">LOG200</span>
        {showVersion && <VersionBadge />}
      </span>
    </>
  );
  return (
    <div className="brand" aria-hidden={heading ? undefined : true}>
      <Logo />
      {heading ? <h1 className="title">{title}</h1> : <p className="title">{title}</p>}
    </div>
  );
};

export { Brand };
