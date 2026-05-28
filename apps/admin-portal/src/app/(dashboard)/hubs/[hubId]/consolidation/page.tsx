import { cookies } from 'next/headers';
import { redirect } from 'next/navigation';
import { COOKIE_LOS_ACCESS } from '@/lib/auth/los-cookies';
import ConsolidationPageClient from './ConsolidationPageClient';

interface Props {
  params: { hubId: string };
}

/**
 * Load Planning — 3D Tetris bin-packing viewer for a hub.
 *
 * This server component reads the hubId from the route segment and the JWT
 * from the httpOnly session cookie, then hands both down to the client
 * component which opens the WebSocket and renders the Three.js canvas.
 */
export default async function ConsolidationPage({ params }: Props) {
  const { hubId } = params;
  const cookieStore = cookies();
  const token = cookieStore.get(COOKIE_LOS_ACCESS)?.value ?? null;

  if (!token) {
    redirect('/login');
  }

  return (
    <ConsolidationPageClient hubId={hubId} token={token} />
  );
}

export function generateMetadata({ params }: Props) {
  return { title: `Load Planning — Hub ${params.hubId.slice(0, 8)}` };
}
