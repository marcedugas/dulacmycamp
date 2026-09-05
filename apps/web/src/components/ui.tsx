import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode, TextareaHTMLAttributes } from 'react';
import { useEffect } from 'react';
import { X } from 'lucide-react';
import type { BookingStatus, JournalStatus } from '../lib/types';

export function cx(...parts: (string | false | null | undefined)[]): string {
  return parts.filter(Boolean).join(' ');
}

// ─────────────────────────── button ───────────────────────────

type Variant = 'primary' | 'secondary' | 'ghost' | 'danger' | 'light';

const VARIANTS: Record<Variant, string> = {
  primary: 'bg-forest-600 text-cream hover:bg-forest-700 disabled:bg-forest-300',
  secondary: 'bg-wood-600 text-cream hover:bg-wood-700 disabled:bg-wood-300',
  ghost: 'bg-transparent text-charcoal border border-sand hover:bg-cream-dark',
  danger: 'bg-clay text-white hover:brightness-110 disabled:opacity-50',
  // For CTAs sitting on a dark photo or gradient.
  light: 'bg-cream text-forest-800 hover:bg-white',
};

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: 'sm' | 'md' | 'lg';
}

export function Button({ variant = 'primary', size = 'md', className, ...rest }: ButtonProps) {
  const sizes = { sm: 'px-3 py-1.5 text-sm', md: 'px-4 py-2.5 text-sm', lg: 'px-6 py-3 text-base' };
  return (
    <button
      className={cx(
        'inline-flex items-center justify-center gap-2 rounded-lg font-semibold transition',
        'disabled:cursor-not-allowed',
        VARIANTS[variant],
        sizes[size],
        className,
      )}
      {...rest}
    />
  );
}

// ─────────────────────────── surfaces ───────────────────────────

export function Card({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div className={cx('rounded-xl border border-sand bg-white/70 p-5 shadow-sm', className)}>
      {children}
    </div>
  );
}

export function Section({
  title,
  subtitle,
  children,
  id,
}: {
  title: string;
  subtitle?: string;
  children: ReactNode;
  id?: string;
}) {
  return (
    <section id={id} className="mx-auto w-full max-w-6xl px-4 py-12 sm:py-16">
      <h2 className="text-2xl font-bold text-charcoal sm:text-3xl">{title}</h2>
      {subtitle && <p className="mt-2 max-w-2xl text-muted">{subtitle}</p>}
      <div className="mt-6">{children}</div>
    </section>
  );
}

export function PageHeader({ title, subtitle, actions }: { title: string; subtitle?: string; actions?: ReactNode }) {
  return (
    <div className="mb-6 flex flex-wrap items-end justify-between gap-3">
      <div>
        <h1 className="text-2xl font-bold text-charcoal sm:text-3xl">{title}</h1>
        {subtitle && <p className="mt-1 text-sm text-muted">{subtitle}</p>}
      </div>
      {actions}
    </div>
  );
}

// ─────────────────────────── form controls ───────────────────────────

export function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <label className="block">
      <span className="mb-1.5 block text-sm font-semibold text-charcoal">{label}</span>
      {children}
      {hint && <span className="mt-1 block text-xs text-muted">{hint}</span>}
    </label>
  );
}

const CONTROL =
  'w-full rounded-lg border border-sand bg-white px-3 py-2.5 text-sm text-charcoal placeholder:text-muted/60 focus:border-forest-500 focus:outline-none';

export function Input({ className, ...rest }: InputHTMLAttributes<HTMLInputElement>) {
  return <input className={cx(CONTROL, className)} {...rest} />;
}

export function Textarea({ className, ...rest }: TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return <textarea className={cx(CONTROL, 'resize-y', className)} {...rest} />;
}

// ─────────────────────────── feedback ───────────────────────────

export function Spinner({ className }: { className?: string }) {
  return (
    <div
      role="status"
      aria-label="Loading"
      className={cx(
        'h-6 w-6 animate-spin rounded-full border-2 border-sand border-t-forest-600',
        className,
      )}
    />
  );
}

export function EmptyState({ icon, title, hint }: { icon?: ReactNode; title: string; hint?: string }) {
  return (
    <div className="rounded-xl border border-dashed border-sand bg-cream-dark/40 px-6 py-12 text-center">
      {icon && <div className="mb-3 flex justify-center text-muted">{icon}</div>}
      <p className="font-semibold text-charcoal">{title}</p>
      {hint && <p className="mt-1 text-sm text-muted">{hint}</p>}
    </div>
  );
}

const STATUS_STYLES: Record<BookingStatus, string> = {
  pending: 'bg-amber-100 text-amber-900 border-amber-300',
  approved: 'bg-forest-100 text-forest-800 border-forest-300',
  denied: 'bg-red-100 text-red-900 border-red-300',
  cancelled: 'bg-cream-dark text-muted border-sand',
};

export function StatusBadge({ status }: { status: BookingStatus }) {
  return (
    <span
      className={cx(
        'inline-block rounded-full border px-2.5 py-0.5 text-xs font-semibold capitalize',
        STATUS_STYLES[status],
      )}
    >
      {status}
    </span>
  );
}

const JOURNAL_STATUS: Record<JournalStatus, { label: string; className: string }> = {
  pending: { label: 'Pending review', className: 'border-amber-300 bg-amber-100 text-amber-900' },
  approved: { label: 'Published', className: 'border-forest-300 bg-forest-100 text-forest-800' },
  rejected: { label: 'Not published', className: 'border-sand bg-cream-dark text-muted' },
};

/** Same badge shape as {@link StatusBadge}, for a journal entry's review status. */
export function JournalStatusBadge({ status }: { status: JournalStatus }) {
  const { label, className } = JOURNAL_STATUS[status];
  return (
    <span className={cx('rounded-full border px-2.5 py-0.5 text-xs font-semibold', className)}>
      {label}
    </span>
  );
}

// ─────────────────────────── modal ───────────────────────────

export function Modal({
  open,
  onClose,
  title,
  children,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  children: ReactNode;
}) {
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && onClose();
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      <div
        className="absolute inset-0 bg-charcoal/50"
        onClick={onClose}
        aria-hidden
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className="relative w-full max-w-md rounded-xl border border-sand bg-cream p-5 shadow-xl"
      >
        <div className="mb-4 flex items-start justify-between gap-4">
          <h3 className="text-lg font-bold text-charcoal">{title}</h3>
          <button onClick={onClose} aria-label="Close" className="text-muted hover:text-charcoal">
            <X size={18} />
          </button>
        </div>
        {children}
      </div>
    </div>
  );
}
