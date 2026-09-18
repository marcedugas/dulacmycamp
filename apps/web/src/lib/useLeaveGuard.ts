import { useEffect, useRef } from 'react';
import { useBlocker } from 'react-router-dom';

const MESSAGE = "You have changes that haven't been saved — leave anyway? What you've written and attached will be lost.";

/**
 * Warns before an *accidental* departure while `shouldBlock()` is true: the
 * tab closing or reloading (the browser's own `beforeunload` prompt — its
 * wording is the browser's, not ours), and in-app navigation, whether a nav
 * link or the back button (`useBlocker`).
 *
 * Deliberate exits are the caller's business: flip whatever `shouldBlock`
 * reads to false *before* navigating, and the guard steps aside. It is read
 * through a ref at the moment of navigation, so it can never be a render
 * behind.
 */
export function useLeaveGuard(shouldBlock: () => boolean) {
  const check = useRef(shouldBlock);
  useEffect(() => {
    check.current = shouldBlock;
  });

  const blocker = useBlocker(({ currentLocation, nextLocation }) => {
    // A search-param tweak on the same page isn't leaving it.
    if (currentLocation.pathname === nextLocation.pathname) return false;
    return check.current();
  });

  useEffect(() => {
    if (blocker.state !== 'blocked') return;
    if (window.confirm(MESSAGE)) blocker.proceed();
    else blocker.reset();
  }, [blocker]);

  useEffect(() => {
    const onBeforeUnload = (e: BeforeUnloadEvent) => {
      if (!check.current()) return;
      e.preventDefault();
      // Still required by some browsers to actually show the prompt.
      e.returnValue = '';
    };
    window.addEventListener('beforeunload', onBeforeUnload);
    return () => window.removeEventListener('beforeunload', onBeforeUnload);
  }, []);
}

/** The same question, for a close that isn't a navigation (a modal's ✕). */
export const confirmDiscard = () => window.confirm(MESSAGE);
