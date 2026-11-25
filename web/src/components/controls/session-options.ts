import { GLOBAL } from '@/lib/constants';

// "Toutes", puis chaque session.
export const sessionOptions = (sessions: string[] = []): { value: string; label: string }[] => [
  { value: GLOBAL, label: 'Toutes' },
  ...sessions.map((session) => ({ value: session, label: session })),
];
