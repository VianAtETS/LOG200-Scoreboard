import type { FC, PropsWithChildren } from 'react';

type Props = PropsWithChildren & {
  id: string;
  collapsed: boolean;
  controls: string;
  onToggle: () => void;
};

const SectionToggle: FC<Props> = ({ id, collapsed, controls, onToggle, children }) => (
  <h2 id={id} className="section-heading">
    <button
      type="button"
      className="section-toggle"
      aria-expanded={!collapsed}
      aria-controls={controls}
      onClick={onToggle}
    >
      {children}
      <span className="chevron" aria-hidden="true" />
    </button>
  </h2>
);

export { SectionToggle };
