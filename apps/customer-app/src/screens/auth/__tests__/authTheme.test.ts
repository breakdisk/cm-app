describe('sign-in theme', () => {
  const original = process.env.EXPO_PUBLIC_APP_VARIANT;

  afterEach(() => {
    if (original === undefined) delete process.env.EXPO_PUBLIC_APP_VARIANT;
    else process.env.EXPO_PUBLIC_APP_VARIANT = original;
    jest.resetModules();
  });

  it('keeps the default build on its original palette and fonts', () => {
    delete process.env.EXPO_PUBLIC_APP_VARIANT;
    jest.isolateModules(() => {
      const { A } = require('../authTheme');
      expect(A.primaryGrad).toEqual(['#00E5FF', '#A855F7']);
      expect(A.confirmGrad).toEqual(['#00FF88', '#00E5FF']);
      expect(A.heading).toEqual({ fontFamily: 'SpaceGrotesk-Bold' });
      expect(A.buttonText.color).toBe('#050810');
    });
  });

  it('puts the Move build on the Move tokens', () => {
    process.env.EXPO_PUBLIC_APP_VARIANT = 'move';
    jest.isolateModules(() => {
      const { A } = require('../authTheme');
      const { M } = require('../../move/theme');
      expect(A.primaryGrad).toEqual(M.accentGrad);
      expect(A.canvas).toBe(M.ground);
      expect(A.buttonText.color).toBe(M.accentInk);
      // No purple left anywhere in the Move sign-in.
      expect(JSON.stringify(A)).not.toMatch(/A855F7|168,85,247/i);
    });
  });
});
