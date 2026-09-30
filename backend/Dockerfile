FROM rust:1-slim-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=build /src/target/release/backend /usr/local/bin/backend
COPY schema.sql .
EXPOSE 8081
CMD ["backend"]
