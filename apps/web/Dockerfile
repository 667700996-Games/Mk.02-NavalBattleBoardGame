FROM node:22-alpine AS builder
WORKDIR /workspace
COPY package.json package-lock.json ./
COPY apps/web/package.json ./apps/web/package.json
RUN npm ci
COPY apps/web ./apps/web
RUN npm --workspace @mk01/web run build \
  && npm prune --omit=dev

FROM node:22-alpine AS runtime
WORKDIR /app
ENV NODE_ENV=production HOST=0.0.0.0 PORT=3000
RUN addgroup -S mk01 && adduser -S -G mk01 mk01
COPY --from=builder --chown=mk01:mk01 /workspace/apps/web/build ./build
COPY --from=builder --chown=mk01:mk01 /workspace/node_modules ./node_modules
COPY --from=builder --chown=mk01:mk01 /workspace/package.json ./package.json
USER mk01
EXPOSE 3000
CMD ["node", "build"]

