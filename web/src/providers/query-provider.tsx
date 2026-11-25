import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { FC, PropsWithChildren } from 'react';

// Comme l'ancienne page : relecture à intervalle fixe, même en arrière-plan, sans réessai entre deux.
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: false,
      refetchOnWindowFocus: false,
      refetchIntervalInBackground: true,
    },
  },
});

type Props = PropsWithChildren;

const QueryProvider: FC<Props> = ({ children }) => (
  <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
);

export { QueryProvider };
