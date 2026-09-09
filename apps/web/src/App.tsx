import { Navigate, Route, Routes, useLocation } from 'react-router-dom';
import type { ReactNode } from 'react';
import Nav from './components/Nav';
import Footer from './components/Footer';
import { Spinner } from './components/ui';
import { useAuth } from './lib/auth';
import Landing from './routes/Landing';
import CalendarPage from './routes/CalendarPage';
import ForecastPage from './routes/ForecastPage';
import BookPage from './routes/BookPage';
import MyBookings from './routes/MyBookings';
import Profile from './routes/Profile';
import Inbox from './routes/Inbox';
import Login from './routes/Login';
import Checkout from './routes/Checkout';
import Journal from './routes/Journal';
import JournalNew from './routes/JournalNew';
import MyStay from './routes/MyStay';
import AdminPanel from './routes/admin/AdminPanel';

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
    return <Navigate to="/login" replace state={{ from: location.pathname + location.search }} />;
  }
  if (adminOnly && !isAdmin) return <Navigate to="/" replace />;

  return <>{children}</>;
}

export default function App() {
  return (
    <div className="flex min-h-screen flex-col">
      <Nav />
      <main className="flex-1">
        <Routes>
          <Route path="/" element={<Landing />} />
          <Route path="/calendar" element={<CalendarPage />} />
          <Route path="/forecast" element={<ForecastPage />} />
          <Route path="/login" element={<Login />} />
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
          <Route
            path="/journal"
            element={<Journal />}
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
