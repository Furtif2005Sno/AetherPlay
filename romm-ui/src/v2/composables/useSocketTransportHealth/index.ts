import { debounce } from "lodash";
import { computed, effectScope, onScopeDispose, ref } from "vue";
import socket from "@/services/socket";
import { isAetherPlay } from "@/v2/aetherplay/api";
import { useSocketEvent } from "@/v2/composables/useSocketEvent";

const UPGRADE_RECHECK_MS = 2500;

export function transportLooksDegraded(
  transportName: string | undefined,
): boolean {
  // AetherPlay: polling is the intended transport there (the app's custom
  // scheme cannot carry WebSockets), so it is not a degraded connection.
  if (isAetherPlay()) return false;
  return transportName === "polling";
}

const websocketDegraded = ref(false);
let installed = false;

function syncDegradedFromTransport() {
  websocketDegraded.value = transportLooksDegraded(
    socket.io?.engine?.transport?.name,
  );
}

type Engine = NonNullable<typeof socket.io>["engine"];

let engineBinding: { engine: Engine; handler: () => void } | null = null;

function clearEngineUpgradeListener() {
  if (!engineBinding) return;
  engineBinding.engine.off("upgrade", engineBinding.handler);
  engineBinding = null;
}

function bindEngineUpgradeListener() {
  const engine = socket.io?.engine;
  if (!engine) return;
  clearEngineUpgradeListener();
  const onUpgrade = () => {
    websocketDegraded.value = false;
  };
  engine.on("upgrade", onUpgrade);
  engineBinding = { engine, handler: onUpgrade };
}

function install() {
  effectScope(true).run(() => {
    const onConnect = () => {
      bindEngineUpgradeListener();
      syncDegradedFromTransport();
      setTimeout(() => syncDegradedFromTransport(), UPGRADE_RECHECK_MS);
    };

    const onConnectError = debounce(() => {
      websocketDegraded.value = true;
    }, 300);

    useSocketEvent("connect", onConnect, { connect: false });
    useSocketEvent("connect_error", onConnectError, { connect: false });

    onScopeDispose(clearEngineUpgradeListener);

    if (socket.connected) {
      onConnect();
    }
  });
}

export function useSocketTransportHealth() {
  if (!installed) {
    installed = true;
    install();
  }

  // AetherPlay: never surfaced; the socket connects over polling through the
  // app's proxy and recovers on its own, the warning only alarms users.
  const isWebSocketDegraded = computed(() => !isAetherPlay() && websocketDegraded.value);

  function retryWebSocket() {
    if (socket.connected) {
      socket.disconnect();
    }
    socket.connect();
  }

  return { isWebSocketDegraded, retryWebSocket };
}
