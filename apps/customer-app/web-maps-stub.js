// Web stub for react-native-maps — native-only module with no web equivalent.
// Exports a no-op View so any attempt to render MapView renders nothing on web.
const React = require('react');
const { View } = require('react-native-web');

const Noop = (props) => React.createElement(View, props);
Noop.displayName = 'MapViewStub';

module.exports = Noop;
module.exports.default = Noop;
module.exports.Marker = Noop;
module.exports.Polyline = Noop;
module.exports.Polygon = Noop;
module.exports.Circle = Noop;
module.exports.PROVIDER_GOOGLE = 'google';
module.exports.PROVIDER_DEFAULT = null;
