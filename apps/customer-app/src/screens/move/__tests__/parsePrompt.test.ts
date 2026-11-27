import { classifyIntent, describeParse, parsePrompt } from '../parsePrompt';

describe('parsePrompt', () => {
  test('a quantity, a destination and a day', () => {
    expect(parsePrompt('Two pallets to Columbus Thursday')).toEqual({
      items: [{ name: 'pallets', qty: 2 }],
      from: '',
      to: 'Columbus',
      when: 'Thursday',
    });
  });

  test('a from-to route with a time after the destination', () => {
    const p = parsePrompt('Move my 3-seater sofa and 2 boxes from 12 Elm St to 240 N Michigan tomorrow at 2 PM');
    expect(p.items).toEqual([{ name: '3-seater sofa', qty: 1 }, { name: 'boxes', qty: 2 }]);
    expect(p.from).toBe('12 Elm St');
    expect(p.to).toBe('240 N Michigan');
    expect(p.when).toBe('tomorrow at 2 PM');
  });

  // "one-bedroom" is part of the item, not a quantity.
  test('a hyphenated number word stays in the item name', () => {
    const p = parsePrompt('One-bedroom apartment, Saturday');
    expect(p.items).toEqual([{ name: 'One-bedroom apartment', qty: 1 }]);
    expect(p.when).toBe('Saturday');
  });

  // An address number after "at" is not a time.
  test('"at 12 Elm St" is not read as a time', () => {
    const p = parsePrompt('Fridge from at 12 Elm St to Oak Park');
    expect(p.when).toBe('');
  });

  test('nothing recognisable still returns the text as one item', () => {
    expect(parsePrompt('fridge swap')).toEqual({ items: [{ name: 'fridge swap', qty: 1 }], from: '', to: '', when: '' });
  });

  test('describeParse reports only what was read', () => {
    expect(describeParse(parsePrompt('Two pallets to Columbus Thursday'))).toBe('2 items · 1 drop-off · Thursday');
    expect(describeParse(parsePrompt('a sofa'))).toBe('1 item');
  });
});

describe('classifyIntent', () => {
  test('support words with an active job go to support', () => {
    expect(classifyIntent('Where is my driver?', true)).toBe('support');
    expect(classifyIntent('I was overcharged', true)).toBe('support');
  });

  // With no job the same words are a booking.
  test('support words without an active job are a booking', () => {
    expect(classifyIntent('Where is my driver?', false)).toBe('book');
  });

  // The handoff: "help", "driver" and "broken" are ordinary move vocabulary.
  test('ordinary move vocabulary is never support', () => {
    expect(classifyIntent('Need help moving a broken table, driver with a van', true)).toBe('book');
  });
});
