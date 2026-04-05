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

  test('renders 4 quick-action cards', () => {
    const { getAllByTestId } = render(
      <Provider store={store}>
        <HomeScreen navigation={mockNavigation} />
      </Provider>
    );
    const actions = getAllByTestId('quick-action');
    expect(actions.length).toBe(4);
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
