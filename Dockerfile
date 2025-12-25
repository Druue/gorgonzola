FROM rust@sha256:f6c22e0a256c05d44fca23bf530120b5d4a6249a393734884281ca80782329bc AS ralp

# Create and change to the app directory.
WORKDIR /app

# Build application
COPY . .
RUN cargo build --release

CMD ["./target/release/gorgonzola"]