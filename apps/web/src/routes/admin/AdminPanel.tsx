import { useState } from 'react';
import { PageHeader, cx } from '../../components/ui';
import { useJournalPendingCount } from '../../lib/queries';
import { InboxView } from '../Inbox';
import AccessTab from './AccessTab';
import BookingsTab from './BookingsTab';
import BlackoutTab from './BlackoutTab';
import CheckinInfoTab from './CheckinInfoTab';
import ChecklistTab from './ChecklistTab';
import EventsTab from './EventsTab';
import JournalTab from './JournalTab';
import SiteContentTab from './SiteContentTab';
import UsersTab from './UsersTab';

const TABS = [
  'Bookings',
  'Blackout Dates',
  'Events',
  'Checklist',
  'Check-In Info',
  'Journal',
  'Site Content',
  'Users',
  'Access',
  'Messages',
] as const;
type Tab = (typeof TABS)[number];

export default function AdminPanel() {
  const [tab, setTab] = useState<Tab>('Bookings');
  const pendingJournal = useJournalPendingCount();

  return (
    <div className="mx-auto max-w-6xl px-4 py-10">
      <PageHeader title="Admin" subtitle="Everything that happens at the camp." />

      <div className="mb-6 flex gap-1 overflow-x-auto border-b border-sand">
        {TABS.map((t) => (
          <button
            key={t}
            onClick={() => setTab(t)}
            aria-current={tab === t}
            className={cx(
              '-mb-px flex items-center gap-1.5 whitespace-nowrap border-b-2 px-4 py-2.5 text-sm font-semibold transition',
              tab === t
                ? 'border-forest-600 text-forest-700'
                : 'border-transparent text-muted hover:text-charcoal',
            )}
          >
            {t}
            {t === 'Journal' && pendingJournal > 0 && (
              <span className="min-w-[18px] rounded-full bg-clay px-1 text-[11px] font-bold leading-[18px] text-white">
                {pendingJournal > 9 ? '9+' : pendingJournal}
              </span>
            )}
          </button>
        ))}
      </div>

      {tab === 'Access' && <AccessTab />}
      {tab === 'Bookings' && <BookingsTab />}
      {tab === 'Blackout Dates' && <BlackoutTab />}
      {tab === 'Events' && <EventsTab />}
      {tab === 'Checklist' && <ChecklistTab />}
      {tab === 'Check-In Info' && <CheckinInfoTab />}
      {tab === 'Journal' && <JournalTab />}
      {tab === 'Site Content' && <SiteContentTab />}
      {tab === 'Users' && <UsersTab />}
      {tab === 'Messages' && <InboxView all />}
    </div>
  );
}
