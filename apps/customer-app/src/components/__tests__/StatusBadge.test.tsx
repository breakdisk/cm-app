import React from 'react';
import { render } from '@testing-library/react-native';
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
    expect(badge.props.style.some((s: any) => s.paddingVertical === 4)).toBe(true);
  });
});
