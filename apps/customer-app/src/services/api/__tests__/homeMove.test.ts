jest.mock('../client', () => ({ getOrderClient: jest.fn() }));

import {
  declared, isHomeMove, moveOk, readChips, readHome, sizeFromRead, slotDay, surveyRequired, totals, validMovePick,
  type HomeCatalogue, type HomeSlots,
} from '../homeMove';

const APT = ['Studio', '1 bedroom', '2 bedroom', '3 bedroom', '4 bedroom', '5 bedroom +'];
const VILLA = ['Studio', '1 bedroom', '2 bedroom', '3 bedroom', '4 bedroom', '5 bedroom', '6 bedroom +'];
const OFFICE = ['Up to 10 desks', '10 to 30 desks', '30 to 75 desks', 'Whole floor'];

describe('reading a sentence offline', () => {
  test.each([
    ['moving my whole house to Quezon City', true],
    ['relocating our 3 bedroom flat next month', true],
    ['3BHK shifting', true],
    ['we are moving out on the 5th', true],
    ['move the office to BGC', true],
    ['move my sofa to Makati', false],
    ['send 6 boxes to my mum', false],
  ])('%s → home: %s', (text, home) => {
    expect(isHomeMove(text)).toBe(home);
  });

  test('reads only what the sentence states', () => {
    expect(readHome('moving my 3 bedroom apartment, 4th floor, no lift')).toEqual({
      property_type: 'apartment', bedrooms: 3, pickup_floor: 4, pickup_has_lift: false,
    });
    expect(readHome('moving house')).toEqual({ property_type: 'villa' });
  });

  test('a desk count alone is an office; a bedroom count alone is an apartment', () => {
    expect(readHome('relocating 25 desks')).toMatchObject({ property_type: 'offices', desks: 25 });
    expect(readHome('moving a two bed next week')).toMatchObject({ property_type: 'apartment', bedrooms: 2 });
  });

  test('studio wins over a bedroom count; floors collapse at five', () => {
    expect(readHome('moving my studio flat on the 12th floor')).toMatchObject({ studio: true, pickup_floor: 5 });
  });

  test('chips are one per stated field', () => {
    expect(readChips({ property_type: 'apartment', pickup_has_lift: true }, null)).toEqual(['Apartment', 'Lift']);
  });
});

describe('sizes', () => {
  test('bedrooms clamp to the top step of the type', () => {
    expect(sizeFromRead('apartment', APT, { bedrooms: 3 })).toBe('3 bedroom');
    expect(sizeFromRead('apartment', APT, { bedrooms: 6 })).toBe('5 bedroom +');
    expect(sizeFromRead('villa', VILLA, { bedrooms: 5 })).toBe('5 bedroom');
    expect(sizeFromRead('apartment', APT, { studio: true, bedrooms: 2 })).toBe('Studio');
    expect(sizeFromRead('apartment', APT, {})).toBeNull();
  });

  test('desks fall into their band', () => {
    expect(sizeFromRead('offices', OFFICE, { desks: 8 })).toBe('Up to 10 desks');
    expect(sizeFromRead('offices', OFFICE, { desks: 30 })).toBe('10 to 30 desks');
    expect(sizeFromRead('offices', OFFICE, { desks: 999 })).toBe('Whole floor');
  });

  test('the survey rule mirrors the server', () => {
    expect(surveyRequired({ type: 'apartment', size: '1 bedroom' }, APT)).toBe(false);
    expect(surveyRequired({ type: 'apartment', size: '2 bedroom' }, APT)).toBe(true);
    expect(surveyRequired({ type: 'villa', size: 'Studio' }, VILLA)).toBe(true);
  });
});

describe('the calendar guard', () => {
  // Manila (UTC+8). Surveys Fri 18 and Tue 22; moves Mon 21 and Thu 24.
  const slots: HomeSlots = {
    survey: [{ starts_at: '2026-09-18T00:00:00Z', ends_at: '2026-09-18T02:00:00Z' }, { starts_at: '2026-09-22T00:00:00Z', ends_at: '2026-09-22T02:00:00Z' }],
    move: [{ starts_at: '2026-09-21T00:00:00Z', ends_at: '2026-09-21T04:00:00Z' }, { starts_at: '2026-09-24T00:00:00Z', ends_at: '2026-09-24T04:00:00Z' }],
    survey_lead_days: 2,
    utc_offset_minutes: 480,
  };

  test('a move two days after the survey is legal, one before it is not', () => {
    expect(moveOk(slots.move[0], slots.survey[0], true, slots)).toBe(true);
    expect(moveOk(slots.move[0], slots.survey[1], true, slots)).toBe(false);
  });

  test('moving the survey later re-resolves the move to the first legal one', () => {
    expect(validMovePick(slots, 0, 0, true)).toBe(0);
    expect(validMovePick(slots, 1, 0, true)).toBe(1);
  });

  test('without a survey every move is legal', () => {
    expect(validMovePick(slots, 1, 0, false)).toBe(0);
  });

  test('days read in the calendar’s own time', () => {
    expect(slotDay('2026-09-18T00:00:00Z', 480)).toBe('Fri 18 Sep');
  });
});

test('the draft inventory becomes declared lines and running totals', () => {
  const catalogue: HomeCatalogue = {
    property_type: 'apartment', sizes: APT, truck_name: '3-ton box truck',
    rooms: [{ key: 'living', name: 'Living room', items: [
      { key: 'sofa', group: 'living', name: 'Sofa', volume_l: 2100, weight_kg: 78, assembly: true, packing: false },
    ] }],
  };
  const inv = { living: { sofa: { qty: 2, dismantle: true, packing: false }, ghost: { qty: 1, dismantle: false, packing: false } } };
  expect(declared(inv)).toHaveLength(2);
  expect(totals(inv, catalogue)).toEqual({ volumeL: 4200, weightKg: 156, count: 2 });
});
