import { useState } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { toast } from 'sonner';
import { ArrowLeft, Fish } from 'lucide-react';
import { api, ApiError } from '../lib/api';
import { useAuth } from '../lib/auth';
import type { AuthResponse } from '../lib/types';
import { Button, Card, Field, Input } from '../components/ui';

/**
 * Two-step passwordless login. There is no sign-up: the first code request
 * for an address creates the account server-side.
 */
export default function Login() {
  const [step, setStep] = useState<'email' | 'code'>('email');
  const [email, setEmail] = useState('');
  const [code, setCode] = useState('');
  const [busy, setBusy] = useState(false);

  const { signIn } = useAuth();
  const navigate = useNavigate();
  const location = useLocation();
  const returnTo = (location.state as { from?: string } | null)?.from ?? '/my-bookings';

  const requestCode = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      await api('/auth/request-otp', { method: 'POST', body: { email }, anonymous: true });
      setStep('code');
      toast.success('Check your email for a 6-digit code.');
    } catch (err) {
      toast.error(err instanceof ApiError ? err.message : 'Could not send the code.');
    } finally {
      setBusy(false);
    }
  };

  const verify = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      const res = await api<AuthResponse>('/auth/verify-otp', {
        method: 'POST',
        body: { email, code },
        anonymous: true,
      });
      signIn(res);
      toast.success(`Welcome${res.user.full_name ? `, ${res.user.full_name}` : ''}!`);
      // Nudge first-time guests to fill in their profile.
      navigate(res.user.full_name ? returnTo : '/profile', { replace: true });
    } catch (err) {
      toast.error(err instanceof ApiError ? err.message : 'Could not verify that code.');
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="mx-auto flex max-w-md flex-col justify-center px-4 py-16">
      <div className="mb-6 text-center">
        <Fish className="mx-auto mb-3 text-forest-600" size={34} aria-hidden />
        <h1 className="text-2xl font-bold text-charcoal">Sign in to the camp</h1>
        <p className="mt-1 text-sm text-muted">
          No password needed — we&apos;ll email you a code.
        </p>
      </div>

      <Card>
        {step === 'email' ? (
          <form onSubmit={requestCode} className="space-y-4">
            <Field label="Email address" hint="First time here? Your account is created automatically.">
              <Input
                type="email"
                required
                autoFocus
                autoComplete="email"
                placeholder="you@example.com"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
              />
            </Field>
            <Button type="submit" size="lg" className="w-full" disabled={busy}>
              {busy ? 'Sending…' : 'Send me a code'}
            </Button>
          </form>
        ) : (
          <form onSubmit={verify} className="space-y-4">
            <p className="text-sm text-muted">
              We sent a code to <strong className="text-charcoal">{email}</strong>.
            </p>
            <Field label="6-digit code" hint="The code expires in 10 minutes.">
              <Input
                inputMode="numeric"
                pattern="[0-9]*"
                maxLength={6}
                required
                autoFocus
                autoComplete="one-time-code"
                placeholder="000000"
                className="text-center font-display text-2xl tracking-[0.4em]"
                value={code}
                onChange={(e) => setCode(e.target.value.replace(/\D/g, ''))}
              />
            </Field>
            <Button type="submit" size="lg" className="w-full" disabled={busy || code.length !== 6}>
              {busy ? 'Verifying…' : 'Sign in'}
            </Button>
            <button
              type="button"
              onClick={() => {
                setStep('email');
                setCode('');
              }}
              className="flex w-full items-center justify-center gap-1.5 text-sm font-semibold text-muted hover:text-charcoal"
            >
              <ArrowLeft size={14} /> Use a different email
            </button>
          </form>
        )}
      </Card>
    </div>
  );
}
