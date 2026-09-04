import { useEffect, useRef, useState } from 'react';
import { Link, NavLink, useLocation, useNavigate } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { Bell, ChevronDown, Fish, Menu, X } from 'lucide-react';
import { api } from '../lib/api';
import { useAuth } from '../lib/auth';
import type { Message } from '../lib/types';
import { cx } from './ui';

/** Unread messages addressed to me — the bell badge. */
function useUnreadCount(userId: string | undefined) {
  const { data } = useQuery({
    queryKey: ['messages'],
    queryFn: () => api<Message[]>('/messages'),
    enabled: Boolean(userId),
    refetchInterval: 120_000,
  });
  return data?.filter((m) => !m.is_read && m.recipient_id === userId).length ?? 0;
}

function Wordmark() {
  return (
    <Link to="/" className="flex items-center gap-2">
      <Fish className="text-forest-300" size={22} aria-hidden />
      <span className="font-display text-lg font-bold tracking-tight">
        <span className="text-forest-300">Dulac</span>
        <span className="text-cream"> My Camp</span>
      </span>
    </Link>
  );
}

const linkClass = ({ isActive }: { isActive: boolean }) =>
  cx(
    'rounded-lg px-3 py-2 text-sm font-semibold transition',
    isActive ? 'bg-white/10 text-cream' : 'text-cream/75 hover:bg-white/5 hover:text-cream',
  );

export default function Nav() {
  const { user, isAdmin, signOut } = useAuth();
  const navigate = useNavigate();
  const location = useLocation();
  const unread = useUnreadCount(user?.id);

  const [menuOpen, setMenuOpen] = useState(false);
  const [mobileOpen, setMobileOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  // Any navigation closes both menus.
  useEffect(() => {
    setMenuOpen(false);
    setMobileOpen(false);
  }, [location.pathname]);

  useEffect(() => {
    if (!menuOpen) return;
    const onClick = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) setMenuOpen(false);
    };
    document.addEventListener('mousedown', onClick);
    return () => document.removeEventListener('mousedown', onClick);
  }, [menuOpen]);

  const handleSignOut = () => {
    signOut();
    navigate('/');
  };

  const links = user
    ? [
        { to: '/calendar', label: 'Calendar' },
        { to: '/book', label: 'Book' },
        { to: '/my-bookings', label: 'My Bookings' },
        { to: '/checkout', label: 'Checkout' },
        { to: '/journal', label: 'Journal' },
      ]
    : [
        { to: '/calendar', label: 'Calendar' },
        { to: '/book', label: 'Book Now' },
        { to: '/journal', label: 'Journal' },
      ];

  return (
    <header className="wood-grain sticky top-0 z-40 bg-charcoal shadow-md">
      <nav className="mx-auto flex max-w-6xl items-center justify-between gap-4 px-4 py-3">
        <Wordmark />

        <div className="hidden items-center gap-1 md:flex">
          {links.map((l) => (
            <NavLink key={l.to} to={l.to} className={linkClass}>
              {l.label}
            </NavLink>
          ))}

          {user ? (
            <>
              <Link
                to="/inbox"
                aria-label={unread > 0 ? `Inbox, ${unread} unread` : 'Inbox'}
                className="relative ml-1 rounded-lg p-2 text-cream/75 transition hover:bg-white/5 hover:text-cream"
              >
                <Bell size={19} />
                {unread > 0 && (
                  <span className="absolute -right-0.5 -top-0.5 min-w-[18px] rounded-full bg-clay px-1 text-[11px] font-bold leading-[18px] text-white">
                    {unread > 9 ? '9+' : unread}
                  </span>
                )}
              </Link>

              <div className="relative ml-1" ref={menuRef}>
                <button
                  onClick={() => setMenuOpen((v) => !v)}
                  aria-expanded={menuOpen}
                  className="flex items-center gap-1.5 rounded-lg px-2.5 py-2 text-sm font-semibold text-cream/85 transition hover:bg-white/5"
                >
                  <span className="grid h-7 w-7 place-items-center rounded-full bg-forest-600 text-xs font-bold text-cream">
                    {(user.full_name || user.email)[0]?.toUpperCase()}
                  </span>
                  <span className="max-w-[10ch] truncate">{user.full_name || 'Account'}</span>
                  <ChevronDown size={15} />
                </button>

                {menuOpen && (
                  <div className="absolute right-0 mt-2 w-44 overflow-hidden rounded-xl border border-sand bg-cream py-1 shadow-lg">
                    <Link to="/profile" className="block px-4 py-2 text-sm hover:bg-cream-dark">
                      Profile
                    </Link>
                    <Link to="/inbox" className="block px-4 py-2 text-sm hover:bg-cream-dark">
                      Inbox {unread > 0 && <span className="text-clay">({unread})</span>}
                    </Link>
                    {isAdmin && (
                      <Link to="/admin" className="block px-4 py-2 text-sm hover:bg-cream-dark">
                        Admin
                      </Link>
                    )}
                    <button
                      onClick={handleSignOut}
                      className="block w-full border-t border-sand px-4 py-2 text-left text-sm text-clay hover:bg-cream-dark"
                    >
                      Log out
                    </button>
                  </div>
                )}
              </div>
            </>
          ) : (
            <NavLink to="/login" className={linkClass}>
              Login
            </NavLink>
          )}
        </div>

        <button
          className="rounded-lg p-2 text-cream md:hidden"
          onClick={() => setMobileOpen((v) => !v)}
          aria-label="Menu"
          aria-expanded={mobileOpen}
        >
          {mobileOpen ? <X size={20} /> : <Menu size={20} />}
        </button>
      </nav>

      {mobileOpen && (
        <div className="border-t border-white/10 px-4 pb-4 md:hidden">
          {links.map((l) => (
            <NavLink key={l.to} to={l.to} className={({ isActive }) => cx(linkClass({ isActive }), 'block')}>
              {l.label}
            </NavLink>
          ))}
          {user ? (
            <>
              <NavLink to="/inbox" className={({ isActive }) => cx(linkClass({ isActive }), 'block')}>
                Inbox {unread > 0 && <span className="text-clay">({unread})</span>}
              </NavLink>
              <NavLink to="/profile" className={({ isActive }) => cx(linkClass({ isActive }), 'block')}>
                Profile
              </NavLink>
              {isAdmin && (
                <NavLink to="/admin" className={({ isActive }) => cx(linkClass({ isActive }), 'block')}>
                  Admin
                </NavLink>
              )}
              <button
                onClick={handleSignOut}
                className="block w-full rounded-lg px-3 py-2 text-left text-sm font-semibold text-clay hover:bg-white/5"
              >
                Log out
              </button>
            </>
          ) : (
            <NavLink to="/login" className={({ isActive }) => cx(linkClass({ isActive }), 'block')}>
              Login
            </NavLink>
          )}
        </div>
      )}
    </header>
  );
}
