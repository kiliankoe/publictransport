import { readFileSync } from "node:fs";
import { createServer, type ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";
import type { HttpResponse, Location, Network } from "@kiliankoe/publictransport";

/** The day the fixtures were recorded. */
export const time = new Date("2026-11-16T13:24:00Z");

/** A stop that another network returned. */
export const foreignStop: Location = {
  id: "8010085",
  globalIds: [{ scheme: "ifopt", value: "de:14612:28:3:2" }],
  kind: "stop",
  name: "Dresden Hbf",
  network: "other",
};

/** An answer that the tests of the core replay too. */
export function fixture(path: string): Uint8Array {
  const file = new URL(`../../../crates/core/tests/fixtures/${path}`, import.meta.url);
  return new Uint8Array(readFileSync(file));
}

export function ok(body: Uint8Array): HttpResponse {
  return { status: 200, headers: [], body };
}

export function efa(url: string): Network {
  return {
    id: "vvo",
    name: "VVO",
    endpoint: `${url}/std3`,
    protocol: "efa",
    timezone: "Europe/Berlin",
  };
}

/** A request as the server received it. */
export interface Received {
  method: string | undefined;
  url: string | undefined;
  headers: Record<string, string | string[] | undefined>;
  body: Uint8Array;
}

/** What the server sends: status, headers and body, or whatever the function does. */
export type Answer =
  | { status?: number; headers?: Record<string, string>; body?: Uint8Array }
  | ((response: ServerResponse) => void);

/** Runs a test against a server that gives these answers in order. */
export async function serving(
  answers: Answer[],
  run: (url: string, received: Received[]) => Promise<void>,
): Promise<void> {
  const received: Received[] = [];
  const server = createServer(async (request, response) => {
    const chunks: Uint8Array[] = [];
    for await (const chunk of request) {
      chunks.push(chunk);
    }
    const { method, url, headers } = request;
    received.push({ method, url, headers, body: new Uint8Array(Buffer.concat(chunks)) });
    const answer = answers[received.length - 1] ?? { status: 500 };
    if (typeof answer === "function") {
      answer(response);
    } else {
      response.writeHead(answer.status ?? 200, answer.headers).end(answer.body);
    }
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  try {
    await run(`http://127.0.0.1:${(server.address() as AddressInfo).port}`, received);
  } finally {
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  }
}
