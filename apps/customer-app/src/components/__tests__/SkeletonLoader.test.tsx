import React from 'react';
import { render } from '@testing-library/react-native';
import SkeletonLoader from '../SkeletonLoader';

describe('SkeletonLoader', () => {
  test('renders with default props', () => {
    const { getByTestId } = render(<SkeletonLoader testID="skeleton" />);
    expect(getByTestId('skeleton')).toBeTruthy();
  });

  test('renders with custom width and height', () => {
    const { getByTestId } = render(
      <SkeletonLoader testID="skeleton" width={200} height={100} />
    );
    const skeleton = getByTestId('skeleton');
    expect(skeleton.props.style[0].width).toBe(200);
    expect(skeleton.props.style[0].height).toBe(100);
  });

  test('renders with custom borderRadius', () => {
    const { getByTestId } = render(
      <SkeletonLoader testID="skeleton" borderRadius={16} />
    );
    const skeleton = getByTestId('skeleton');
    expect(skeleton.props.style[0].borderRadius).toBe(16);
  });

  test('renders with percentage width', () => {
    const { getByTestId } = render(
      <SkeletonLoader testID="skeleton" width="50%" />
    );
    const skeleton = getByTestId('skeleton');
    expect(skeleton.props.style[0].width).toBe('50%');
  });
});
