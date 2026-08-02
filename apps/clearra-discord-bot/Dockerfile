FROM rust:1.96-bookworm AS clearra-build

WORKDIR /workspace
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY tools ./tools
COPY assets ./assets
COPY apps/clearra-web/static/tablebase/pc4-compact-exact-v12.bin \
  ./apps/clearra-web/static/tablebase/pc4-compact-exact-v12.bin
RUN cargo build \
      --locked \
      --release \
      --package clearra-cli \
      --features wasm-cpu-runtime,webgpu-search \
    && /workspace/target/release/clearra --format json rules list >/dev/null \
    && /workspace/target/release/clearra \
      --format json pc --lines 2 --queue IJLOO --fixed --no-hold >/dev/null

FROM node:22-bookworm-slim AS node-build

WORKDIR /workspace
COPY . .
RUN npm ci
RUN npm run build --workspace ctk3
RUN npm prune --omit=dev

FROM node:22-bookworm-slim AS runtime

ENV NODE_ENV=production
ENV PORT=8080
ENV CLEARRA_EXECUTABLE=/usr/local/bin/clearra
WORKDIR /workspace

COPY --from=clearra-build /workspace/target/release/clearra /usr/local/bin/clearra
COPY --from=node-build /workspace/node_modules ./node_modules
COPY --from=node-build /workspace/apps/clearra-discord-bot ./apps/clearra-discord-bot
COPY --from=node-build /workspace/packages/ctk3/package.json ./packages/ctk3/package.json
COPY --from=node-build /workspace/packages/ctk3/dist ./packages/ctk3/dist

USER node
EXPOSE 8080
CMD ["node", "./apps/clearra-discord-bot/src/main.mjs"]
