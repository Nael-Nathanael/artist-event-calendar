FROM rust:1-slim-trixie AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked

FROM debian:trixie-slim
COPY --from=build /src/target/release/backend /usr/local/bin/backend
EXPOSE 8081
CMD ["backend"]
