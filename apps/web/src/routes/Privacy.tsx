import { Link } from 'react-router-dom';
import { Card, PageHeader } from '../components/ui';

/**
 * Plain-English privacy page. One screen. Static on purpose: the promises on
 * it are the owners' to make, so a change here should be a deliberate edit
 * that Jean reads, not a settings toggle.
 */
export default function Privacy() {
  return (
    <div className="mx-auto max-w-2xl px-4 py-10">
      <PageHeader title="Privacy" subtitle="What we collect when you ask to stay, and what we do with it." />
      <Card className="space-y-4 text-sm leading-relaxed text-charcoal">
        <section>
          <h2 className="font-display text-base font-bold">What we collect</h2>
          <p>
            Your name and email, your phone number, your city and state, an optional mailing
            address, who referred you or how you heard about the camp, and what you are bringing
            (boat, kayak or canoe). We don&apos;t ask for ID numbers, birth dates or payment
            details.
          </p>
        </section>
        <section>
          <h2 className="font-display text-base font-bold">Who can see it</h2>
          <p>
            Only the camp owners and administrators. Other guests and family members cannot see
            your phone, address or referral, and none of it appears on the public calendar.
          </p>
        </section>
        <section>
          <h2 className="font-display text-base font-bold">Where it lives</h2>
          <p>
            It is stored with our hosting provider. Your phone number and mailing address are
            encrypted in the database. Emailed notices go through an email delivery service, but
            they never contain your phone number or address.
          </p>
        </section>
        <section>
          <h2 className="font-display text-base font-bold">What we never do</h2>
          <p>We never sell your information or share it with anyone else.</p>
        </section>
        <section>
          <h2 className="font-display text-base font-bold">How long we keep it, and deleting it</h2>
          <p>
            We keep it until you or the owners delete it. You can delete your phone, address, city
            and state yourself any time from your <Link to="/profile" className="font-semibold text-forest-700 underline">profile</Link>.
            To have everything removed, including what you told us about a specific stay, use{' '}
            <Link to="/inbox" className="font-semibold text-forest-700 underline">Contact the camp</Link>{' '}
            and ask us to delete it.
          </p>
        </section>
      </Card>
    </div>
  );
}
