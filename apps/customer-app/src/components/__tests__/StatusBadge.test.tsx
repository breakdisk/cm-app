import React from 'react';
import { render } from '@testing-library/react-native';
import { StyleSheet } from 'react-native';
import StatusBadge from '../StatusBadge';

describe('StatusBadge', () => {
  test('renders delivered status with green color', () => {
    const { getByText } = render(<StatusBadge status="delivered" />);
    const badge = getByText('Delivered');
    expect(badge).toBeTruthy();
  });

  test('renders in transit status with purple color', () => {
    const { getByText } = render(<StatusBadge status="in_transit" />);
    const badge = getByText('In Transit');
    expect(badge).toBeTruthy();
  });

  test('renders failed status with red color', () => {
    const { getByText } = render(<StatusBadge status="failed" />);
    const badge = getByText('Failed');
    expect(badge).toBeTruthy();
  });

  test('renders with compact size', () => {
    const { getByTestId } = render(<StatusBadge status="delivered" size="sm" />);
    const badge = getByTestId('status-badge');
    // Flattened rather than searched: the component hands `Animated.View` an
    // array, and Animated reshapes it, so `style.some(...)` was asserting on
    // internal plumbing rather than on the padding the prop is supposed to
    // produce. StyleSheet.flatten asks the question the test means to ask.
    expect(StyleSheet.flatten(badge.props.style).paddingVertical).toBe(4);
  });
});
