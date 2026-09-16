import { mergeTranscript, voiceErrorMessage } from '../voice';

describe('mergeTranscript', () => {
  test('a later result replaces the earlier one', () => {
    expect(mergeTranscript('move my sofa', 'move my sofa and two boxes')).toBe('move my sofa and two boxes');
  });

  test('an empty or missing result keeps what was already heard', () => {
    expect(mergeTranscript('move my sofa', '')).toBe('move my sofa');
    expect(mergeTranscript('move my sofa', '   ')).toBe('move my sofa');
    expect(mergeTranscript('move my sofa', undefined)).toBe('move my sofa');
    expect(mergeTranscript('move my sofa', null)).toBe('move my sofa');
  });

  test('surrounding space is dropped', () => {
    expect(mergeTranscript('', '  two pallets to the warehouse  ')).toBe('two pallets to the warehouse');
  });
});

describe('voiceErrorMessage', () => {
  test('known failures say what to do instead', () => {
    expect(voiceErrorMessage('not-allowed')).toMatch(/Settings/);
    expect(voiceErrorMessage('no-speech')).toMatch(/say it again/);
    expect(voiceErrorMessage('service-not-allowed')).toMatch(/Type it instead/);
  });

  // The customer tapped stop. Telling them it failed would be a lie.
  test('an aborted session says nothing', () => {
    expect(voiceErrorMessage('aborted')).toBe('');
  });

  test('an unknown code falls back to the platform message, then to plain words', () => {
    expect(voiceErrorMessage('something-new', 'Recognizer busy')).toBe('Recognizer busy');
    expect(voiceErrorMessage('something-new', '   ')).toMatch(/Tap the circle/);
    expect(voiceErrorMessage()).toMatch(/Tap the circle/);
  });
});
