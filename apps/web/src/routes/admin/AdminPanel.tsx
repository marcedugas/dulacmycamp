import { useState } from 'react';
import { PageHeader, cx } from '../../components/ui';
import { InboxView } from '../Inbox';
import BookingsTab from './BookingsTab';
import BlackoutTab from './BlackoutTab';
import EventsTab from './EventsTab';
import SiteContentTab from './SiteContentTab';
import UsersTab from './UsersTab';

const TABS = ['Bookings', 'Blackout Dates', 'Events', 'Site Content', 'Users', 'Messages'] as const;
type Tab = (typeof TABS)[number];

export default function AdminPanel() {
  const [tab, setTab] = useState<Tab>('Bookings');

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
              '-mb-px whitespace-nowrap border-b-2 px-4 py-2.5 text-sm font-semibold transition',
              tab === t
                ? 'border-forest-600 text-forest-700'
                : 'border-transparent text-muted hover:text-charcoal',
            )}
          >
            {t}
          </button>
        ))}
      </div>

      {tab === 'Bookings' && <BookingsTab />}
      {tab === 'Blackout Dates' && <BlackoutTab />}
      {tab === 'Events' && <EventsTab />}
      {tab === 'Site Content' && <SiteContentTab />}
      {tab === 'Users' && <UsersTab />}
      {tab === 'Messages' && <InboxView all />}
    </div>
  );
}
