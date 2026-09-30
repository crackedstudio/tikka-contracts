FROM node:26-slim@sha256:ec7758ee051e457b468b32bde57b0879010b325bb9862718e9615225ce4aaae1 AS builder
WORKDIR /app
COPY package*.json ./
RUN npm ci
COPY . .
RUN npm run build
RUN npm prune --production

FROM node:26-slim@sha256:ec7758ee051e457b468b32bde57b0879010b325bb9862718e9615225ce4aaae1 AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends tini=0.19.0-1 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/dist ./dist
COPY --from=builder /app/node_modules ./node_modules
COPY --from=builder /app/package.json .
RUN mkdir -p /app/data && chown node:node /app/data
USER node
EXPOSE 9090
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
	CMD node -e "require('http').get('http://127.0.0.1:9090/health', res => process.exit(0)).on('error', () => process.exit(1))"
ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["node", "dist/index.js"]
