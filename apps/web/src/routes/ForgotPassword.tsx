import { useState } from 'react';
import { Link, useLocation, useNavigate } from 'react-router-dom';
import { toast } from 'sonner';
import { ArrowLeft, KeyRound } from 'lucide-react';
import { api, ApiError } from '../lib/api';
import { useAuth } from '../lib/auth';
import { clearReturnTo, readReturnTo, safeReturnPath } from '../lib/returnTo';
import { MIN_PASSWORD_LEN, PASSWORD_RULES } from '../lib/password';
import type { AuthResponse, MessageResponse } from '../lib/types';
import { Button, Card, Field, Input } from '../components/ui';
import { PasswordInput } from '../components/PasswordInput';

/**
 * Forgot password, in two steps: email, then the emailed code plus a new
 * password. The code is a reset code — it cannot sign anyone in on its own,
 * and a sign-in code cannot reset a password.
 *
 * Step 1 always ends the same way whether or not the address has an account,
 * so the wording stays generic ("if that email has an account…") — the page
 * must not say anything the server won't.
 */
export default function ForgotPassword() {
  const location = useLocation();
  const navigate = useNavigate();
  const { signIn } = useAuth();
  const state = location.state as { from?: string; email?: string } | null;

  const [step, setStep] = useState<'email' | 'reset'>('email');
  const [email, setEmail] = useState(typeof state?.email === 'string' ? state.email : '');
  const [code, setCode] = useState('');
  const [password, setPassword] = useState('');
  const [confirm, setConfirm] = useState('');
  const [busy, setBusy] = useState(false);

  const tooShort = password.length > 0 && password.length < MIN_PASSWORD_LEN;
  const mismatch = confirm.length > 0 && confirm !== password;
  const ready = code.length === 6 && password.length >= MIN_PASSWORD_LEN && confirm === password;

  const requestCode = async (e?: React.FormEvent) => {
    e?.preventDefault();
    setBusy(true);
    try {
      const res = await api<MessageResponse>('/auth/forgot-password', {
        method: 'POST',
        body: { email },
        anonymous: true,
      });
      setStep('reset');
      toast.success(res.message);
    } catch (err) {
      toast.error(err instanceof ApiError ? err.message : 'Could not send the code. Try again.');
    } finally {
      setBusy(false);
    }
  };

  const reset = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      const res = await api<AuthResponse>('/auth/reset-password', {
        method: 'POST',
        body: { email, code, new_password: password },
        anonymous: true,
      });
      signIn(res);
      toast.success('Your password has been changed. You are signed in.');
      // Where they were headed, same as after any other sign-in — not a
      // forced detour to the profile page.
      const target = safeReturnPath(state?.from) ?? readReturnTo() ?? '/my-bookings';
      clearReturnTo();
      navigate(target, { replace: true });
    } catch (err) {
      toast.error(err instanceof ApiError ? err.message : 'Could not reset your password. Try again.');
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="mx-auto flex max-w-md flex-col justify-center px-4 py-16">
      <div className="mb-6 text-center">
        <KeyRound className="mx-auto mb-3 text-forest-600" size={34} aria-hidden />
        <h1 className="text-2xl font-bold text-charcoal">Reset your password</h1>
        <p className="mt-1 text-sm text-muted">
          {step === 'email'
            ? "Enter your email and we'll send a code to reset it."
            : 'Enter the code we emailed you and choose a new password.'}
        </p>
      </div>

      <Card>
        {step === 'email' ? (
          <form onSubmit={requestCode} className="space-y-4">
            <Field label="Email address">
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
            <Button type="submit" size="lg" className="w-full" disabled={busy || !email}>
              {busy ? 'Sending…' : 'Send me a code'}
            </Button>
          </form>
        ) : (
          <form onSubmit={reset} className="space-y-4">
            <p className="text-sm text-muted">
              If <strong className="text-charcoal">{email}</strong> has an account, we sent a code to
              it. It expires in 10 minutes.
            </p>
            <Field label="6-digit code">
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
            <Field label="New password" hint={PASSWORD_RULES}>
              <PasswordInput
                aria-label="New password"
                required
                autoComplete="new-password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
              />
            </Field>
            <Field label="Confirm new password">
              <PasswordInput
                aria-label="Confirm new password"
                required
                autoComplete="new-password"
                value={confirm}
                onChange={(e) => setConfirm(e.target.value)}
              />
            </Field>
            {tooShort && (
              <p className="text-sm text-clay">
                That&apos;s {password.length} characters — {MIN_PASSWORD_LEN} is the minimum.
              </p>
            )}
            {mismatch && <p className="text-sm text-clay">Those two don&apos;t match.</p>}

            <Button type="submit" size="lg" className="w-full" disabled={busy || !ready}>
              {busy ? 'Saving…' : 'Set new password'}
            </Button>
            <div className="flex flex-col items-center gap-2 text-sm">
              <button
                type="button"
                disabled={busy}
                onClick={() => void requestCode()}
                className="font-semibold text-forest-600 hover:text-forest-700"
              >
                Send a new code
              </button>
              <button
                type="button"
                onClick={() => {
                  setStep('email');
                  setCode('');
                }}
                className="flex items-center gap-1.5 font-semibold text-muted hover:text-charcoal"
              >
                <ArrowLeft size={14} /> Use a different email
              </button>
            </div>
          </form>
        )}
      </Card>

      <Link
        to="/login"
        state={{ from: state?.from }}
        className="mt-4 text-center text-sm font-semibold text-forest-600 hover:text-forest-700"
      >
        Back to sign in
      </Link>
    </div>
  );
}
