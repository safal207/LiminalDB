import { Client, type ClientMetrics, type EventKind } from '../src/index';

declare const process: {
  env: Record<string, string | undefined>;
  exitCode?: number;
};

const DEFAULT_URL = 'ws://127.0.0.1:8787';
const DEFAULT_TIMEOUT_MS = 5_000;
const DEFAULT_EVENT_WAIT_MS = 2_000;

interface ObservedEvent {
  kind: string;
  payload: unknown;
}

function readNumber(name: string, fallback: number): number {
  const raw = process.env[name];
  if (!raw) {
    return fallback;
  }
  const parsed = Number(raw);
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : fallback;
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function createConnectionWaiter(timeoutMs: number): {
  promise: Promise<void>;
  resolveConnected: () => void;
} {
  let resolveConnected!: () => void;
  let rejectConnection!: (error: Error) => void;

  const promise = new Promise<void>((resolve, reject) => {
    resolveConnected = resolve;
    rejectConnection = reject;
  });

  const timer = setTimeout(() => {
    rejectConnection(
      new Error(
        `Timed out waiting for SDK connection after ${timeoutMs}ms. ` +
          `Start LiminalDB with: ./target/release/liminal-cli --store ./data --ws-port 8787`
      )
    );
  }, timeoutMs);

  return {
    promise: promise.finally(() => clearTimeout(timer)),
    resolveConnected,
  };
}

function observe<K extends EventKind>(client: Client, kind: K, events: ObservedEvent[]): void {
  client.on(kind, (payload) => {
    events.push({ kind, payload });
    console.log(JSON.stringify({ event: kind, payload }, null, 2));
  });
}

async function main(): Promise<void> {
  const url = process.env.LIMINAL_WS_URL ?? DEFAULT_URL;
  const timeoutMs = readNumber('LIMINAL_SMOKE_TIMEOUT_MS', DEFAULT_TIMEOUT_MS);
  const eventWaitMs = readNumber('LIMINAL_SMOKE_EVENT_WAIT_MS', DEFAULT_EVENT_WAIT_MS);
  const requireEvent = process.env.LIMINAL_SMOKE_REQUIRE_EVENT === '1';

  let lastMetrics: ClientMetrics = {
    state: 'disconnected',
    queueDepth: 0,
    sentCommands: 0,
    receivedEvents: 0,
    reconnectAttempts: 0,
  };

  const events: ObservedEvent[] = [];
  const waiter = createConnectionWaiter(timeoutMs);

  const client = new Client({
    url,
    reconnect: false,
    telemetry: (metrics) => {
      lastMetrics = metrics;
      if (metrics.state === 'connected') {
        waiter.resolveConnected();
      }
    },
  });

  observe(client, 'lql', events);
  observe(client, 'view', events);
  observe(client, 'seed', events);
  observe(client, 'mirror', events);
  observe(client, 'status', events);
  observe(client, 'alert', events);

  client.connect();
  await waiter.promise;

  const subscriptionId = client.subscribe('cpu/load', { mode: 'live', limit: 10 });
  const queryId = client.lql('SELECT cpu/load WINDOW 1000');
  client.seed.garden();
  client.mirror.timeline({ from: 'now-5m', to: 'now' });

  await delay(eventWaitMs);

  client.unsubscribe(subscriptionId);
  client.disconnect(1000, 'liminaldb-sdk-smoke-complete');

  if (lastMetrics.sentCommands < 4) {
    throw new Error(`Expected at least 4 commands to be sent, got ${lastMetrics.sentCommands}`);
  }

  if (requireEvent && events.length === 0) {
    throw new Error(
      'No SDK events received. Set LIMINAL_SMOKE_REQUIRE_EVENT=0 to treat connect/send as sufficient.'
    );
  }

  console.log(
    JSON.stringify(
      {
        ok: true,
        url,
        queryId,
        subscriptionId,
        sentCommands: lastMetrics.sentCommands,
        receivedEvents: lastMetrics.receivedEvents,
        observedEventKinds: Array.from(new Set(events.map((event) => event.kind))),
        note:
          events.length > 0
            ? 'SDK connected, sent commands, and observed events.'
            : 'SDK connected and sent commands. No events were observed during the smoke wait window.',
      },
      null,
      2
    )
  );
}

main().catch((error: unknown) => {
  process.exitCode = 1;
  console.error(
    JSON.stringify(
      {
        ok: false,
        error: error instanceof Error ? error.message : String(error),
        hint: 'Start LiminalDB locally first: ./target/release/liminal-cli --store ./data --ws-port 8787',
      },
      null,
      2
    )
  );
});
