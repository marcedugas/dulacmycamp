import { Link } from 'react-router-dom';
import { Fish } from 'lucide-react';

export default function Footer() {
  return (
    <footer className="wood-grain mt-16 bg-charcoal">
      <div className="mx-auto flex max-w-6xl flex-col items-center justify-between gap-3 px-4 py-8 sm:flex-row">
        <div className="flex items-center gap-2">
          <Fish className="text-forest-300" size={18} aria-hidden />
          <span className="font-display text-sm font-bold text-cream">Dulac My Camp</span>
        </div>
        <p className="text-xs text-cream/55">© 2026 Dulac My Camp</p>
        <Link to="/inbox" className="text-xs font-semibold text-forest-300 hover:text-forest-200">
          Contact the camp
        </Link>
      </div>
    </footer>
  );
}
