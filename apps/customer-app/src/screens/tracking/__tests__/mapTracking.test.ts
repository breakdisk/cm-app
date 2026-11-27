import { isTerminalStatus, mapPublicTracking, LIVE_TRACKING_POLL_MS } from '../mapTracking';

describe('mapPublicTracking', () => {
  test('maps the enveloped public response', () => {
    const r = mapPublicTracking(
      {
        data: {
          tracking_number: 'CM-PH1-S0000001A',
          status: 'out_for_delivery',
          origin: 'Pasig',
          destination: 'Quezon City',
          estimated_delivery: '2026-09-16',
          driver: { name: 'Dee', lat: 14.5, lng: 121.0 },
          history: [{ status: 'confirmed', description: 'Booked', occurred_at: '2026-09-15T01:00:00Z' }],
        },
      },
      'FALLBACK',
    );

    expect(r.awb).toBe('CM-PH1-S0000001A');
    expect(r.status).toBe('out_for_delivery');
    expect(r.origin_city).toBe('Pasig');
    expect(r.driver_name).toBe('Dee');
    expect(r.driver_location).toEqual({ lat: 14.5, lng: 121.0 });
    expect(r.timeline).toHaveLength(1);
  });

  // The server sends a driver with no fix as lat/lng null. Number(null) is 0,
  // and a 0,0 marker is worse than no map.
  test('a driver without a position fix has no location, not 0,0', () => {
    const fromDriver = mapPublicTracking({ status: 'out_for_delivery', driver: { name: 'Dee', lat: null, lng: null } }, 'A');
    expect(fromDriver.driver_location).toBeUndefined();

    const fromField = mapPublicTracking({ status: 'out_for_delivery', driver_location: { lat: null, lng: 121 } }, 'A');
    expect(fromField.driver_location).toBeUndefined();
  });

  test('prefers driver_location when the server sends both', () => {
    const r = mapPublicTracking({ driver_location: { lat: '1.5', lng: '2.5' }, driver: { name: 'Dee', lat: 9, lng: 9 } }, 'A');
    expect(r.driver_location).toEqual({ lat: 1.5, lng: 2.5 });
  });

  test('an unenveloped body with no tracking number keeps the searched AWB', () => {
    const r = mapPublicTracking({ status: 'pending' }, 'CM-SEARCHED');
    expect(r.awb).toBe('CM-SEARCHED');
    expect(r.timeline).toEqual([]);
  });
});

describe('live refresh', () => {
  test('polls every 5 seconds', () => {
    expect(LIVE_TRACKING_POLL_MS).toBe(5_000);
  });

  test('stops once there is nothing left to watch', () => {
    for (const s of ['delivered', 'returned', 'cancelled']) expect(isTerminalStatus(s)).toBe(true);
    for (const s of ['pending', 'confirmed', 'in_transit', 'out_for_delivery', 'delivery_attempted']) {
      expect(isTerminalStatus(s)).toBe(false);
    }
  });
});
