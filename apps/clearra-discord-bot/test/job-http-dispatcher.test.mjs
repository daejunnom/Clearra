import assert from "node:assert/strict";
import { channel } from "node:diagnostics_channel";
import { createServer } from "node:http";
import test from "node:test";
import { Agent, getGlobalDispatcher, request, setGlobalDispatcher } from "undici";
import { createJobResponseDispatcher } from "../src/clearra/job-http-dispatcher.mjs";

function deferred() {
  let resolve;
  const promise = new Promise((yes) => { resolve = yes; });
  return { promise, resolve };
}

async function listen(server) {
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  return `http://127.0.0.1:${server.address().port}`;
}

async function close(server, agent) {
  server.closeAllConnections();
  await Promise.all([
    agent.close(),
    new Promise((resolve) => server.close(resolve)),
  ]);
}

test("job dispatcher removes only header wait and preserves every other dispatch option", () => {
  let received;
  const handler = {};
  const underlying = { dispatch(options, callback) { received = { options, callback }; return "accepted"; } };
  const wrapper = createJobResponseDispatcher(underlying);
  const options = { method: "POST", path: "/jobs", headersTimeout: 300_000, bodyTimeout: 27, idempotent: false };
  assert.equal(wrapper.dispatch(options, handler), "accepted");
  assert.deepEqual(received, { options: { ...options, headersTimeout: 0 }, callback: handler });
  assert.equal(options.headersTimeout, 300_000);
});

test("the default wrapper follows the current global proxy or connection dispatcher", () => {
  const previous = getGlobalDispatcher();
  const wrapper = createJobResponseDispatcher();
  let calls = 0;
  try {
    setGlobalDispatcher({ dispatch() { calls++; return "first"; } });
    assert.equal(wrapper.dispatch({}, {}), "first");
    setGlobalDispatcher({ dispatch() { calls++; return "second"; } });
    assert.equal(wrapper.dispatch({}, {}), "second");
    assert.equal(calls, 2);
  } finally {
    setGlobalDispatcher(previous);
  }
});

test("Node fetch passes zero header timeout to the live request before response headers", async (t) => {
  const accepted = deferred();
  const server = createServer((_request, response) => accepted.resolve(response));
  const agent = new Agent({ headersTimeout: 1 });
  const endpoint = await listen(server);
  t.after(() => close(server, agent));
  const observed = [];
  const diagnostics = channel("undici:request:create");
  const inspect = ({ request: item }) => { if (String(item.origin) === endpoint) observed.push(item); };
  diagnostics.subscribe(inspect);
  t.after(() => diagnostics.unsubscribe(inspect));
  const response = fetch(`${endpoint}/jobs`, { method: "POST", dispatcher: createJobResponseDispatcher(agent) });
  const pending = await accepted.promise;
  const beforeHeaders = observed.map(({ headersTimeout, bodyTimeout }) => ({ headersTimeout, bodyTimeout }));
  pending.end("finished");
  assert.equal(await (await response).text(), "finished");
  assert.equal(observed.length, 1);
  assert.equal(beforeHeaders[0].headersTimeout, 0);
  // Node may leave the body setting absent so the existing pool default wins.
  assert.notEqual(beforeHeaders[0].bodyTimeout, 0);
});

test("a stalled response body retains its independently bounded transport timeout", async (t) => {
  const server = createServer((_request, response) => { response.writeHead(200); response.write("partial"); });
  const agent = new Agent();
  const endpoint = await listen(server);
  t.after(() => close(server, agent));
  const response = await request(`${endpoint}/jobs`, {
    method: "POST", dispatcher: createJobResponseDispatcher(agent), bodyTimeout: 20,
  });
  await assert.rejects(response.body.text(), { code: "UND_ERR_BODY_TIMEOUT" });
});
