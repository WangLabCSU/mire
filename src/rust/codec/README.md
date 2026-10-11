# codec

Synchronous frame encoding and decoding for `std::io`.

Implement `Decoder` to parse buffered bytes into frames. `FramedRead` reads from
an input source and yields those frames through `Iterator`. Frame types and
boundaries are defined by the decoder.

Implement `Encoder<Item>` to append encoded frames to a byte buffer.
`FramedWrite::write` buffers frames, flushing before the next frame when the
backpressure boundary is reached. Call `flush` to write pending output;
dropping the adapter or calling `into_inner` does not flush.

See [`examples/reading.rs`](examples/reading.rs) and
[`examples/writing.rs`](examples/writing.rs) for a length-prefixed format.

I/O and codec errors are returned to the caller, who decides whether to
continue. This includes `Interrupted`; the adapters do not retry automatically.
Failed writes retain the unwritten bytes for a subsequent `flush`.

```sh
cargo test -p codec
cargo doc -p codec --no-deps
cargo run -p codec --example reading
cargo run -p codec --example writing
```

Adapted from the [Tokio codec implementation](https://github.com/tokio-rs/tokio/blob/454fd8c3477a37d12d5d11948fd6fe496924e950/tokio-util/src/codec/framed_impl.rs).
Licensed under the [MIT license](LICENSE).
