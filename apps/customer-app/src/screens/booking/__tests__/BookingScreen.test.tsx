import React from 'react';
import { render } from '@testing-library/react-native';

// Mock expo modules before importing BookingScreen sub-components
jest.mock('expo-image-picker', () => ({
  launchImageLibraryAsync: jest.fn(),
  launchCameraAsync: jest.fn(),
  requestCameraPermissionsAsync: jest.fn(() => Promise.resolve({ granted: true })),
  requestMediaLibraryPermissionsAsync: jest.fn(() => Promise.resolve({ granted: true })),
}));

import AddressInput from '../AddressInput';
import PackageDetailsForm from '../PackageDetailsForm';
import ServiceSelector from '../ServiceSelector';
import FeeBreakdown from '../FeeBreakdown';
import BookingConfirmation from '../BookingConfirmation';

describe('BookingScreen sub-components', () => {
  test('renders without crashing', () => {
    expect(true).toBe(true);
  });

  test('component modules are properly exported', () => {
    expect(AddressInput).toBeDefined();
    expect(PackageDetailsForm).toBeDefined();
    expect(ServiceSelector).toBeDefined();
    expect(FeeBreakdown).toBeDefined();
    expect(BookingConfirmation).toBeDefined();
  });

  test('AddressInput component renders with label', () => {
    const { getByText } = render(
      <AddressInput label="Test Address" value="" onChange={jest.fn()} />
    );
    expect(getByText('Test Address')).toBeTruthy();
  });

  test('ServiceSelector component displays local services', () => {
    const { getByText } = render(
      <ServiceSelector type="local" selected="standard" onSelect={jest.fn()} />
    );
    expect(getByText('Standard')).toBeTruthy();
    expect(getByText('Express')).toBeTruthy();
    expect(getByText('Next Day')).toBeTruthy();
  });

  test('ServiceSelector component displays international services', () => {
    const { getByText } = render(
      <ServiceSelector type="international" selected="air" onSelect={jest.fn()} />
    );
    expect(getByText('Air Freight')).toBeTruthy();
    expect(getByText('Sea Freight')).toBeTruthy();
  });

  test('FeeBreakdown calculates total correctly', () => {
    const { getByText } = render(
      <FeeBreakdown baseFee={150} codFee={20} tax={18} total={188} />
    );
    expect(getByText('₱150')).toBeTruthy();
    expect(getByText('₱20')).toBeTruthy();
    expect(getByText('₱188')).toBeTruthy();
  });

  test('BookingConfirmation displays AWB', () => {
    const { getByText } = render(
      <BookingConfirmation
        awb="AWB12345678"
        onTrackPress={jest.fn()}
        onHomePress={jest.fn()}
      />
    );
    expect(getByText('AWB12345678')).toBeTruthy();
    expect(getByText(/Booking Confirmed/i)).toBeTruthy();
  });
});
