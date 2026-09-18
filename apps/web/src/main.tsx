import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { RouterProvider, createBrowserRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { Toaster } from 'sonner';
import App from './App';
import { AuthProvider } from './lib/auth';
import './index.css';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // The camp calendar is not a trading screen; refetching on every window
      // focus just burns NOAA's rate limit.
      refetchOnWindowFocus: false,
      staleTime: 60_000,
      retry: 1,
    },
  },
});

// A data router rather than <BrowserRouter> for one reason: `useBlocker`,
// which the journal form uses to warn before someone navigates away from an
// unposted story, only works under one. The routes themselves stay as plain
// <Routes> inside App — a single splat route hands everything to it.
const router = createBrowserRouter([
  {
    path: '*',
    element: (
      <AuthProvider>
        <App />
        <Toaster richColors position="top-center" />
      </AuthProvider>
    ),
  },
]);

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  </StrictMode>,
);
