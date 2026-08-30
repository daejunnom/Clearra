const DISCORD_SNOWFLAKE = /^\d{17,20}$/;

export function loadCommandRegistrationCredentials(environment = process.env) {
  const token = environment.DISCORD_TOKEN?.trim() ?? "";
  if (!token) {
    throw new Error(
      "DISCORD_TOKEN is required to register commands. Set it in the same " +
        "terminal process, or on Windows run the register:commands:windows " +
        "workspace script for a masked prompt.",
    );
  }
  if (token === "System.Security.SecureString") {
    throw new Error(
      "DISCORD_TOKEN contains a PowerShell SecureString object name, not the " +
        "Discord bot token. Use the register:commands:windows workspace script.",
    );
  }

  const applicationId = environment.DISCORD_APPLICATION_ID?.trim() || null;
  if (applicationId && !DISCORD_SNOWFLAKE.test(applicationId)) {
    throw new Error(
      "DISCORD_APPLICATION_ID must be the 17-20 digit Discord application ID.",
    );
  }
  return Object.freeze({ token, applicationId });
}

export function verifyGlobalCommandRegistration(expected, registered) {
  if (!Array.isArray(expected) || !Array.isArray(registered)) {
    throw new Error(
      "Discord global command registration returned an invalid response.",
    );
  }

  const expectedKeys = commandKeys(expected);
  const registeredKeys = commandKeys(registered);
  const missing = expectedKeys.filter((key) => !registeredKeys.includes(key));
  const unexpected = registeredKeys.filter(
    (key) => !expectedKeys.includes(key),
  );
  if (
    expectedKeys.length !== registeredKeys.length ||
    new Set(expectedKeys).size !== expectedKeys.length ||
    new Set(registeredKeys).size !== registeredKeys.length ||
    missing.length > 0 ||
    unexpected.length > 0
  ) {
    throw new Error(
      "Discord global command registration did not match the Clearra catalog" +
        `${missing.length > 0 ? `; missing ${missing.join(", ")}` : ""}` +
        `${unexpected.length > 0 ? `; unexpected ${unexpected.join(", ")}` : ""}.`,
    );
  }

  const registeredByKey = new Map(
    registered.map((command) => [commandKey(command), command]),
  );
  for (const command of expected) {
    const key = commandKey(command);
    const mismatch = firstCatalogMismatch(
      command,
      registeredByKey.get(key),
      key,
    );
    if (mismatch) {
      throw new Error(
        "Discord global command registration did not match the Clearra catalog; " +
          `${mismatch}.`,
      );
    }
  }

  return Object.freeze({
    count: registeredKeys.length,
    names: Object.freeze(expected.map((command) => command.name)),
  });
}

export async function synchronizeGlobalCommandRegistration(
  rest,
  applicationId,
  expected,
  options = {},
) {
  const current = await rest.getGlobalCommands(applicationId);
  return synchronizeGlobalCommandRegistrationFromObserved(
    rest,
    applicationId,
    expected,
    current,
    options,
  );
}

export async function synchronizeGlobalCommandRegistrationFromObserved(
  rest,
  applicationId,
  expected,
  current,
  options = {},
) {
  const readbackAttempts = options.readbackAttempts ?? 3;
  const retryDelayMs = options.retryDelayMs ?? 250;
  const wait = options.wait ?? sleep;

  const currentVerification = tryVerify(expected, current);
  if (currentVerification.ok) {
    verifyCommandIdentities(applicationId, current);
    return Object.freeze({
      changed: false,
      ...currentVerification.value,
    });
  }

  const unmanagedCommands = current.filter((command) =>
    (command?.type ?? 1) !== 1 &&
    !expected.some((candidate) => commandKey(candidate) === commandKey(command))
  );
  if (unmanagedCommands.length > 0) {
    throw new Error(
      "Discord global command synchronization refused to overwrite " +
        `unmanaged command ${commandKey(unmanagedCommands[0])}.`,
    );
  }

  let registered;
  try {
    registered = await rest.registerGlobalCommands(applicationId, expected);
  } catch (error) {
    if (error?.discordAmbiguous !== true) throw error;
    return confirmAmbiguousGlobalCommandWrite(
      rest,
      applicationId,
      expected,
      readbackAttempts,
      retryDelayMs,
      wait,
      error,
    );
  }
  verifyGlobalCommandRegistration(expected, registered);
  const registeredIdentities = verifyCommandIdentities(
    applicationId,
    registered,
  );

  let lastMismatch = null;
  for (let attempt = 0; attempt < readbackAttempts; attempt += 1) {
    const observed = await rest.getGlobalCommands(applicationId);
    const verification = tryVerify(expected, observed);
    if (verification.ok) {
      try {
        const observedIdentities = verifyCommandIdentities(
          applicationId,
          observed,
        );
        verifyMatchingCommandIdentities(
          registeredIdentities,
          observedIdentities,
        );
        return Object.freeze({
          changed: true,
          ...verification.value,
        });
      } catch (error) {
        lastMismatch = error;
      }
    } else {
      lastMismatch = verification.error;
    }
    if (attempt + 1 < readbackAttempts) {
      await wait(retryDelayMs * 2 ** attempt);
    }
  }

  throw new Error(
    `Discord global command readback did not converge after ${readbackAttempts} attempts.`,
    { cause: lastMismatch },
  );
}

async function confirmAmbiguousGlobalCommandWrite(
  rest,
  applicationId,
  expected,
  readbackAttempts,
  retryDelayMs,
  wait,
  writeError,
) {
  let lastMismatch = writeError;
  for (let attempt = 0; attempt < readbackAttempts; attempt += 1) {
    try {
      const observed = await rest.getGlobalCommands(applicationId);
      const verification = verifyGlobalCommandRegistration(expected, observed);
      verifyCommandIdentities(applicationId, observed);
      return Object.freeze({ changed: true, ...verification });
    } catch (error) {
      lastMismatch = error;
    }
    if (attempt + 1 < readbackAttempts) {
      await wait(retryDelayMs * 2 ** attempt);
    }
  }
  throw new Error(
    "Discord global command update returned an ambiguous failure and " +
      `readback did not converge after ${readbackAttempts} attempts.`,
    { cause: lastMismatch },
  );
}

function tryVerify(expected, registered) {
  try {
    return Object.freeze({
      ok: true,
      value: verifyGlobalCommandRegistration(expected, registered),
    });
  } catch (error) {
    return Object.freeze({ ok: false, error });
  }
}

function sleep(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

function verifyCommandIdentities(applicationId, commands) {
  if (!DISCORD_SNOWFLAKE.test(applicationId)) {
    throw new Error("Discord application ID must be a 17-20 digit snowflake.");
  }
  const identities = new Map();
  const ids = new Set();
  for (const command of commands) {
    const key = commandKey(command);
    if (
      command?.application_id !== applicationId ||
      !DISCORD_SNOWFLAKE.test(command?.id ?? "") ||
      !DISCORD_SNOWFLAKE.test(command?.version ?? "") ||
      identities.has(key) ||
      ids.has(command.id)
    ) {
      throw new Error(
        `Discord global command '${key}' returned invalid identity metadata.`,
      );
    }
    identities.set(key, Object.freeze({
      id: command.id,
      version: command.version,
    }));
    ids.add(command.id);
  }
  return identities;
}

function verifyMatchingCommandIdentities(expected, observed) {
  for (const [key, identity] of expected) {
    const candidate = observed.get(key);
    if (
      candidate?.id !== identity.id ||
      candidate?.version !== identity.version
    ) {
      throw new Error(
        `Discord global command '${key}' readback returned a stale identity.`,
      );
    }
  }
}

function commandKeys(commands) {
  return commands.map(commandKey);
}

function commandKey(command) {
  return `${command?.type ?? 1}:${command?.name ?? ""}`;
}

function firstCatalogMismatch(expected, actual, path) {
  if (Array.isArray(expected)) {
    if (!Array.isArray(actual)) {
      return `${path} expected an array, received ${describeValue(actual)}`;
    }
    if (expected.length !== actual.length) {
      return `${path}.length expected ${expected.length}, ` +
        `received ${actual.length}`;
    }
    for (let index = 0; index < expected.length; index += 1) {
      const mismatch = firstCatalogMismatch(
        expected[index],
        actual[index],
        `${path}[${index}]`,
      );
      if (mismatch) return mismatch;
    }
    return null;
  }

  if (isObject(expected)) {
    if (!isObject(actual)) {
      return `${path} expected an object, received ${describeValue(actual)}`;
    }
    const expectedKeys = Object.keys(expected);
    if (isLocalizationPath(path)) {
      const unexpectedKeys = Object.keys(actual).filter(
        (key) => !Object.hasOwn(expected, key),
      );
      if (unexpectedKeys.length > 0) {
        return `${path} contained unexpected locale ${unexpectedKeys[0]}`;
      }
    }
    for (const key of expectedKeys) {
      const childPath = `${path}.${key}`;
      if (!Object.hasOwn(actual, key)) {
        if (isOmittedDiscordDefault(key, expected[key])) continue;
        return `${childPath} was missing`;
      }
      const mismatch = firstCatalogMismatch(
        expected[key],
        actual[key],
        childPath,
      );
      if (mismatch) return mismatch;
    }
    return null;
  }

  if (!Object.is(expected, actual)) {
    return `${path} expected ${describeValue(expected)}, ` +
      `received ${describeValue(actual)}`;
  }
  return null;
}

function isOmittedDiscordDefault(key, value) {
  return value === false && (key === "required" || key === "autocomplete");
}

function isObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function isLocalizationPath(path) {
  return path.endsWith(".name_localizations") ||
    path.endsWith(".description_localizations");
}

function describeValue(value) {
  let result;
  try {
    result = JSON.stringify(value);
  } catch {
    result = String(value);
  }
  if (result === undefined) result = String(value);
  return result.length <= 160 ? result : `${result.slice(0, 157)}...`;
}
