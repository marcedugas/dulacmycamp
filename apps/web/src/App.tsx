import { Navigate, Route, Routes, useLocation } from 'react-router-dom';
import type { ReactNode } from 'react';
import Nav from './components/Nav';
import Footer from './components/Footer';
import { Spinner } from './components/ui';
import { useEffect } from 'react';
import { useAuth } from './lib/auth';
import { saveReturnTo } from './lib/returnTo';
import Landing from './routes/Landing';
import CalendarPage from './routes/CalendarPage';
import ForecastPage from './routes/ForecastPage';
import BookPage from './routes/BookPage';
import MyBookings from './routes/MyBookings';
import Profile from './routes/Profile';
import Inbox from './routes/Inbox';
import Login from './routes/Login';
import ForgotPassword from './routes/ForgotPassword';
import ChoosePassword from './routes/ChoosePassword';
import Privacy from './routes/Privacy';
import Checkout from './routes/Checkout';
import Journal from './routes/Journal';
import JournalNew from './routes/JournalNew';
import MyStay from './routes/MyStay';
import AdminPanel from './routes/admin/AdminPanel';

/** Remembers where a signed-out visitor was headed (router state for this
 *  tab, storage for the email-app round trip), then sends them to /login. */
function BounceToLogin({ from }: { from: string }) {
  useEffect(() => saveReturnTo(from), [from]);
  return <Navigate to="/login" replace state={{ from }} />;
}

/** Sends signed-out visitors to /login, remembering where they were headed. */
function RequireAuth({ children, adminOnly }: { children: ReactNode; adminOnly?: boolean }) {
  const { user, loading, isAdmin } = useAuth();
  const location = useLocation();

  if (loading) {
    return (
      <div className="flex min-h-[50vh] items-center justify-center">
        <Spinner />
      </div>
    );
  }
  if (!user) {
    return <BounceToLogin from={location.pathname + location.search} />;
  }
  if (adminOnly && !isAdmin) return <Navigate to="/" replace />;

  return <>{children}</>;
}

export default function App() {
  const { user } = useAuth();

  // A temporary password has to be replaced before anything else. The API
  // enforces this on every route; this is the screen that gets them through
  // it. It replaces the whole app (no nav, no routes), so on success they are
  // already at the page they were headed for.
  if (user?.must_change_password) return <ChoosePassword />;

  return (
    <div className="flex min-h-screen flex-col">
      <Nav />
      <main className="flex-1">
        <Routes>
          <Route path="/" element={<Landing />} />
          <Route path="/calendar" element={<CalendarPage />} />
          <Route path="/forecast" element={<ForecastPage />} />
          <Route path="/login" element={<Login />} />
          <Route path="/forgot-password" element={<ForgotPassword />} />
          <Route path="/privacy" element={<Privacy />} />
          <Route
            path="/book"
            element={
              <RequireAuth>
                <BookPage />
              </RequireAuth>
            }
          />
          <Route
            path="/my-bookings"
            element={
              <RequireAuth>
                <MyBookings />
              </RequireAuth>
            }
          />
          <Route
            path="/profile"
            element={
              <RequireAuth>
                <Profile />
              </RequireAuth>
            }
          />
          <Route
            path="/inbox"
            element={
              <RequireAuth>
                <Inbox />
              </RequireAuth>
            }
          />
          <Route
            path="/checkout"
            element={
              <RequireAuth>
                <Checkout />
              </RequireAuth>
            }
          />
          <Route
            path="/my-stay"
            element={
              <RequireAuth>
                <MyStay />
              </RequireAuth>
            }
          />
          {/* Was public. Journal entries are now scoped "family" vs "any
              registered account", so there is no anonymous tier left to
              serve — reading the journal takes a login like everything else
              behind it. */}
          <Route
            path="/journal"
            element={
              <RequireAuth>
                <Journal />
              </RequireAuth>
            }
          />
          <Route
            path="/journal/new"
            element={
              <RequireAuth>
                <JournalNew />
              </RequireAuth>
            }
          />
          <Route
            path="/admin"
            element={
              <RequireAuth adminOnly>
                <AdminPanel />
              </RequireAuth>
            }
          />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </main>
      <Footer />
    </div>
  );
}
