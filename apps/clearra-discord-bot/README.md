# Clearrabot

The bot runs Clearra searches and renders Fumen or CTK3 documents without an
external solver or image-rendering service.

## Requirements

- Node.js 22 or newer
- A release `clearra` executable on `PATH`
- A Discord application with the Bot and Message Content intents enabled

Discord carries events and API responses; it does not host the bot process or
provide search CPUs. Run Clearrabot on a machine or container that you operate,
or deploy it through a separate hosting provider.

Configure the process environment:

```text
DISCORD_TOKEN=...
CLEARRA_EXECUTABLE=clearra
CLEARRA_VIEWER_URL=https://daejunnom.github.io/Clearra/
```

Optional settings:

```text
CLEARRA_DISCORD_PREFIX=!
CLEARRA_REGISTER_COMMANDS=1
CLEARRA_MAX_CONCURRENT_SEARCHES=1
CLEARRA_SEARCH_WORKERS_PER_SESSION=auto
CLEARRA_USE_ALL_LOGICAL_PROCESSORS=0
CLEARRA_SEARCH_TIMEOUT_MS=180000
CLEARRA_MAX_GIF_BYTES=25165824
```

Build CTK3 once, then start the bot:

```powershell
npm run build --workspace ctk3
npm start --workspace @clearra/discord-bot
```

## Commands

`/clearra` and `!clearra` accept the command text after the executable name.
The shorter `!pc`, `!setup`, `!path`, `!percent`, and `!cover` forms are also
accepted.

Each search is stopped after three minutes by default. Clearrabot runs one
search session at a time unless `CLEARRA_MAX_CONCURRENT_SEARCHES` is raised.
At startup it reads the logical processors visible to its process, reserves one
by default, and divides the remaining PC/path/setup capacity between concurrent sessions.
`CLEARRA_SEARCH_WORKERS_PER_SESSION` may lower that per-session allocation; it
cannot exceed the runtime limit. Set `CLEARRA_USE_ALL_LOGICAL_PROCESSORS=1` only
when the host operator explicitly wants to remove the reserved processor.
Discord users cannot override this allocation with `--workers` or
`--cpu-threads`. The resolved allocation is printed once in the startup log.
The current native percent and cover commands do not use this worker pool and
execute on one search thread.

```text
!pc --lines 4 --patterns P7 --hold
!setup --remaining SZ --priority pc --max-setup-pieces 4
```

`/view` renders a raw Fumen, raw CTK3 document, or Clearra viewer URL. Fumen
and CTK3 values in ordinary messages are detected automatically. The GIF and
the interactive Clearra viewer link are sent as a separate reply. When the
direct reply would exceed Discord's 2,000-character limit, Clearrabot attaches
a canonical CTK3 document and links to the Clearra CTK renderer instead.
