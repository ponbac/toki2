FROM oven/bun:1 AS builder

WORKDIR /app

ARG VITE_API_URL=https://toki-api.spinit.se
ARG VITE_TIME_TRACKING_PROVIDER_URL
ENV VITE_API_URL=$VITE_API_URL
ENV VITE_TIME_TRACKING_PROVIDER_URL=$VITE_TIME_TRACKING_PROVIDER_URL

COPY package.json bun.lockb ./
RUN bun install --frozen-lockfile

COPY . .
RUN bun run build

FROM nginx:alpine

WORKDIR /usr/share/nginx/html

COPY nginx.conf /etc/nginx/nginx.conf
COPY --from=builder /app/dist ./

CMD ["nginx", "-g", "daemon off;"]
