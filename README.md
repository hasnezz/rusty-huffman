# Huffman Compressor

A small Huffman coding implementation written in Rust.

It can compress arbitrary files into `.huff` files and decompress them back to their original contents.

## Usage

### Compress

```bash
cargo run -- <file>
```

For example:

```bash
cargo run -- image.png
```

This creates:

```text
image.png.huff
```

### Decompress

Pass a `.huff` file:

```bash
cargo run -- image.png.huff
```

This restores the original file.

## Tests

Run the test suite with:

```bash
cargo test
```

The tests include compression/decompression round-trip tests.

## Implementation

The compressor:

1. Reads the input and builds a frequency table.
2. Builds a Huffman tree using a `BinaryHeap`.
3. Generates a variable-length bit code for each byte.
4. Writes the code table and compressed bitstream to the output.
5. Stores metadata in a small header.

The decompressor reads the header and code table, reconstructs the Huffman tree, and decodes the bitstream.

## Build

```bash
cargo build --release
```
