import { registerRootComponent } from "expo";
import App from "./App";

// registerRootComponent calls AppRegistry.registerComponent('main', () => App)
// and for web, calls AppRegistry.runApplication which mounts into #root.
registerRootComponent(App);
