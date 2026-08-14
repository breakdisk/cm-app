import React from 'react';
import { render, fireEvent } from '@testing-library/react-native';
import { Provider } from 'react-redux';
import { store } from '../../../store';
import HomeScreen from '../HomeScreen';

const mockNavigation = { navigate: jest.fn() };

describe('HomeScreen', () => {
  test('renders greeting with customer name', () => {
    const { getByText } = render(
      <Provider store={store}>
        <HomeScreen navigation={mockNavigation} />
      </Provider>
    );
    expect(getByText(/Welcome back/i)).toBeTruthy();
  });

  // Was `toBe(4)` and had been wrong since "Get Quote" was added — a bare
  // count says nothing about which action went missing, so this names them.
  test('renders every quick action', () => {
    const { getAllByTestId, getByText } = render(
      <Provider store={store}>
        <HomeScreen navigation={mockNavigation} />
      </Provider>
    );
    for (const label of ['Book New', 'Get Quote', 'Track', 'History', 'Support']) {
      expect(getByText(label)).toBeTruthy();
    }
    expect(getAllByTestId('quick-action')).toHaveLength(5);
  });

  test('navigates to Booking when "Book New" is tapped', () => {
    const { getByText } = render(
      <Provider store={store}>
        <HomeScreen navigation={mockNavigation} />
      </Provider>
    );
    fireEvent.press(getByText('Book New'));
    expect(mockNavigation.navigate).toHaveBeenCalledWith('Book');
  });
});
