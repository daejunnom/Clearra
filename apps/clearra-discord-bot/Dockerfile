FROM node:22-bookworm-slim AS build

WORKDIR /workspace
COPY . .
RUN npm ci
RUN npm run build --workspace ctk3
RUN npm prune --omit=dev

FROM node:22-bookworm-slim AS runtime

ENV NODE_ENV=production
ENV PORT=8080
WORKDIR /workspace

COPY --from=build /workspace/node_modules ./node_modules
COPY --from=build /workspace/apps/clearra-discord-bot ./apps/clearra-discord-bot
COPY --from=build /workspace/packages/ctk3/package.json ./packages/ctk3/package.json
COPY --from=build /workspace/packages/ctk3/dist ./packages/ctk3/dist

USER node
EXPOSE 8080
CMD ["node", "./apps/clearra-discord-bot/src/main.mjs"]
