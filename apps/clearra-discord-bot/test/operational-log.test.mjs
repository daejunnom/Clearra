import assert from "node:assert/strict";
import test from "node:test";

import {
  canonicalOperationalCommand,
  writeOperationalLog,
} from "../src/operational-log.mjs";
import { SlashCommandIngress } from "../src/ingress/slash-command-ingress.mjs";
import {
  messageCommandCatalog,
  slashCommandCatalog,
} from "../src/discord/slash-command-catalog.mjs";

test("operational logs retain only allow-listed terminal metadata", () => {
  const lines = [];
  const logger = {
    info(value) { lines.push(value); },
    error(value) { lines.push(value); },
  };
  assert.equal(writeOperationalLog(logger, {
    at: "2026-08-03T00:00:00.000Z",
    scope: "interaction",
    kind: "slash",
    command: "server-settings.language-set",
    status: "succeeded",
    durationMs: 12.6,
    timeoutClass: "setup_long",
    timeoutMs: 900_000,
    content: "PRIVATE COMMAND",
    arguments: ["PRIVATE ARGUMENT"],
    userId: "123456789012345678",
    guildId: "223456789012345678",
    token: "PRIVATE TOKEN",
    error: new Error("PRIVATE ERROR"),
  }), true);
  assert.equal(lines.length, 1);
  assert.deepEqual(JSON.parse(lines[0]), {
    event: "clearra.operation",
    at: "2026-08-03T00:00:00.000Z",
    scope: "interaction",
    kind: "slash",
    command: "server-settings.language-set",
    status: "succeeded",
    durationMs: 13,
    timeoutClass: "setup_long",
    timeoutMs: 900_000,
  });
  assert.doesNotMatch(lines[0], /PRIVATE|user|guild|token|argument|content|error/i);
});

test("timeout telemetry requires one canonical class and one bounded millisecond value", () => {
  const lines = [];
  const logger = { info(value) { lines.push(value); } };
  for (const timeoutClass of [
    "pc_reverse",
    "build_long",
    "setup_long",
    "forward_long",
    "structure_long",
    "diagnostic",
    "default",
  ]) {
    assert.equal(writeOperationalLog(logger, {
      scope: "job",
      kind: "search",
      command: timeoutClass === "diagnostic" ? "sfinder.verify" : "pc",
      status: "succeeded",
      durationMs: 1,
      timeoutClass,
      timeoutMs: 123,
      arguments: ["PRIVATE"],
    }), true);
  }
  assert.equal(writeOperationalLog(logger, {
    scope: "job",
    kind: "search",
    command: "pc",
    status: "succeeded",
    durationMs: 1,
    timeoutClass: "forward",
    timeoutMs: 123,
  }), false);
  assert.equal(writeOperationalLog(logger, {
    scope: "job",
    kind: "search",
    command: "pc",
    status: "succeeded",
    durationMs: 1,
    timeoutClass: "pc_reverse",
  }), false);
  assert.equal(lines.length, 7);
  assert.ok(lines.every((line) => !line.includes("PRIVATE")));
});

test("operational command labels must resolve through a canonical product catalog", () => {
  const lines = [];
  const logger = { info(value) { lines.push(value); } };
  const unknown = "plausible-private-command";

  assert.equal(canonicalOperationalCommand("cat-finder"), null);
  assert.equal(canonicalOperationalCommand("help"), "meta.help");
  assert.equal(canonicalOperationalCommand("meta.help"), "meta.help");
  assert.equal(canonicalOperationalCommand("sfinder.score-finder"), "pc.score-finder");
  assert.equal(canonicalOperationalCommand("pc.score-finder"), "pc.score-finder");
  assert.equal(canonicalOperationalCommand("sfinder.path"), "pc.path");
  assert.equal(canonicalOperationalCommand("sfinder.best-save"), "sfinder.best-save");
  assert.equal(canonicalOperationalCommand("spin-structure"), "spin-structure");
  assert.equal(canonicalOperationalCommand("spin_structure"), "spin-structure");
  assert.equal(
    canonicalOperationalCommand("spin-structure.search"),
    "spin-structure.search",
  );
  assert.equal(
    canonicalOperationalCommand("spin-structure.cover"),
    "spin-structure.cover",
  );
  assert.equal(
    canonicalOperationalCommand("spin-structure.guaranteed"),
    "spin-structure.guaranteed",
  );
  assert.equal(canonicalOperationalCommand("pc.allspin-sol"), "pc.allspin-sol");
  assert.equal(
    canonicalOperationalCommand("pc.allspin-pres-chance"),
    "pc.allspin-pres-chance",
  );
  assert.equal(
    canonicalOperationalCommand("allspin_sol_finder"),
    "pc.allspin-sol",
  );
  assert.equal(
    canonicalOperationalCommand("allspin_pres_chance"),
    "pc.allspin-pres-chance",
  );
  assert.equal(canonicalOperationalCommand("verify"), null);
  assert.equal(canonicalOperationalCommand("sfinder.verify"), "diagnostic.verify");
  assert.equal(canonicalOperationalCommand("diagnostic.verify"), "diagnostic.verify");
  assert.equal(canonicalOperationalCommand(unknown), null);
  assert.equal(writeOperationalLog(logger, {
    scope: "gateway",
    kind: "text",
    command: unknown,
    status: "succeeded",
    durationMs: 1,
  }), true);

  assert.equal(JSON.parse(lines[0]).command, null);
  assert.doesNotMatch(lines[0], new RegExp(unknown));
});

test("delegated text work is a terminal non-failure operational status", () => {
  const lines = [];
  const logger = {
    info(value) { lines.push(["info", value]); },
    error(value) { lines.push(["error", value]); },
  };

  assert.equal(writeOperationalLog(logger, {
    scope: "gateway",
    kind: "text",
    command: "path",
    status: "delegated",
    durationMs: 0,
  }), true);
  assert.equal(lines.length, 1);
  assert.equal(lines[0][0], "info");
  assert.deepEqual(JSON.parse(lines[0][1]), {
    event: "clearra.operation",
    at: JSON.parse(lines[0][1]).at,
    scope: "gateway",
    kind: "text",
    command: "pc.path",
    status: "delegated",
    durationMs: 0,
  });
});

test("the privacy allow-list covers every registered command path", () => {
  for (const command of slashCommandCatalog) {
    assert.equal(
      canonicalOperationalCommand(command.name),
      command.telemetryIdentity ?? command.name,
    );
    for (const option of command.registration?.options ?? []) {
      if (option?.type === 1 || option?.type === 2) {
        const path = `${command.name}.${option.name}`;
        assert.equal(
          canonicalOperationalCommand(path),
          command.subcommands?.[option.name]?.telemetryIdentity ?? path,
        );
      }
    }
  }
  for (const command of messageCommandCatalog) {
    assert.equal(canonicalOperationalCommand(command.name), command.name);
  }
});

test("slash terminal logs keep the command path without interaction values", async () => {
  const lines = [];
  let now = 100;
  const ingress = new SlashCommandIngress({
    async handleInteraction() { return true; },
  }, {
    acknowledger: { async defer() {} },
    operationalScope: "interaction",
    logger: {
      info(value) { lines.push(value); },
      error(value) { lines.push(value); },
    },
    now: () => now += 5,
  });
  const interaction = {
    id: "1533373054309371924",
    type: 2,
    data: {
      type: 1,
      name: "server-settings",
      options: [{
        type: 1,
        name: "language-set",
        options: [{ type: 3, name: "language", value: "PRIVATE VALUE" }],
      }],
    },
  };

  assert.deepEqual(await ingress.accept(interaction), { accepted: true });
  assert.equal(lines.length, 1);
  const record = JSON.parse(lines[0]);
  assert.equal(record.command, "server-settings.language-set");
  assert.equal(record.scope, "interaction");
  assert.equal(record.status, "succeeded");
  assert.doesNotMatch(lines[0], /PRIVATE|language\"|value|interaction.*id/i);
});
