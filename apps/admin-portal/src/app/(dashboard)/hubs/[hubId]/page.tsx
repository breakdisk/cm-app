import { cookies } from 'next/headers';
import { redirect } from 'next/navigation';
import HubDetailClient from './HubDetailClient';

interface Props {
  params:      { hubId: string };
  searchParams: { tab?: string };
}

/**
 * Hub Detail — tabbed view for a single hub.
 *
 * Tabs:
 *   • Overview    — capacity, zones, manifest summary
 *   • Plan Load (3D) — 3-D Tetris bin-packing viewer (ConsolidationPageClient)
 *
 * Tab state is driven by ?tab= query param so URLs are bookmarkable.
 */
export default async function HubDetailPage({ params, searchParams }: Props) {
  const { hubId } = params;
  const cookieStore = cookies();
  const token = cookieStore.get('access_token')?.value ?? null;

  if (!token) {
    redirect('/login');
  }

  const initialTab = searchParams.tab === 'plan-load' ? 'plan-load' : 'overview';

  return (
    <HubDetailClient hubId={hubId} token={token} initialTab={initialTab} />
  );
}

export function generateMetadata({ params }: { params: { hubId: string } }) {
  return { title: `Hub ${params.hubId.slice(0, 8)} — Detail` };
}
