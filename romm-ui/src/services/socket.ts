import { io } from "socket.io-client";

export default io({
  path: "/ws/socket.io/",
  // AetherPlay: the app serves this page on a custom scheme, which cannot carry
  // WebSockets; HTTP long-polling goes through the app's proxy to the server.
  transports: ["polling"],
  autoConnect: false,
});
