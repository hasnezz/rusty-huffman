use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap},
    env,
    ffi::OsString,
    fs::OpenOptions,
    io::{self, BufReader, BufWriter, Write},
    path::PathBuf,
};

#[derive(Debug)]
struct HuffmanNode {
    symbol: u8,
    frequency: u32,
    left: Option<Box<HuffmanNode>>,
    right: Option<Box<HuffmanNode>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct HuffmanBitCode {
    bits: u64,
    len: u8,
}

struct Header {
    table_length: u16,
    payload_length: u64,
    padding: u8,
}

impl Header {
    const BYTE_SIZE: usize = 2 + 8 + 1; // 11 bytes

    fn to_bytes(&self) -> [u8; Self::BYTE_SIZE] {
        let mut bytes = [0u8; Self::BYTE_SIZE];
        bytes[0..2].copy_from_slice(&self.table_length.to_be_bytes());
        bytes[2..10].copy_from_slice(&self.payload_length.to_be_bytes());
        bytes[10] = self.padding;
        bytes
    }
}

impl TryFrom<&[u8]> for Header {
    type Error = io::Error;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.len() < Self::BYTE_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Buffer too small for Header",
            ));
        }

        let table_length = u16::from_be_bytes(bytes[0..2].try_into().unwrap());
        let payload_length = u64::from_be_bytes(bytes[2..10].try_into().unwrap());
        let padding = bytes[10];

        Ok(Header {
            table_length,
            payload_length,
            padding,
        })
    }
}

impl HuffmanNode {
    fn new(symbol: &u8, freq: &u32) -> Self {
        Self {
            symbol: symbol.clone(),
            frequency: freq.clone(),
            left: None,
            right: None,
        }
    }
}
struct HuffmanTreeStateMachine<'a> {
    root: &'a HuffmanNode,
    cursor: &'a HuffmanNode,
}

impl<'a> HuffmanTreeStateMachine<'a> {
    fn new(root: &'a HuffmanNode) -> Self {
        Self { root, cursor: root }
    }

    fn feed(&mut self, bit: bool) -> Option<u8> {
        let next_node = if bit {
            self.cursor.right.as_deref()
        } else {
            self.cursor.left.as_deref()
        };

        match next_node {
            Some(node) => {
                self.cursor = node;
            }
            None => panic!("illegal tree walk"),
        }

        if self.cursor.left.is_none() && self.cursor.right.is_none() {
            let symbol = self.cursor.symbol;
            self.cursor = self.root;
            Some(symbol)
        } else {
            None
        }
    }
}

impl PartialEq for HuffmanNode {
    fn eq(&self, other: &Self) -> bool {
        self.frequency == other.frequency
    }
}

impl Eq for HuffmanNode {}

impl PartialOrd for HuffmanNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HuffmanNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other.frequency.cmp(&self.frequency)
    }
}

fn generate_tree<R: io::Read>(reader: &mut R) -> io::Result<HuffmanNode> {
    let mut freq: HashMap<u8, u32> = HashMap::new();
    let mut heap: BinaryHeap<HuffmanNode> = BinaryHeap::new();

    loop {
        const BUFFER_SIZE: usize = 2048;
        let mut buffer = [0u8; BUFFER_SIZE];
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }

        for b in &buffer[..n] {
            let count = freq.entry(b.clone()).or_insert(0);
            *count += 1
        }
    }

    for (k, v) in &freq {
        heap.push(HuffmanNode::new(k, v));
    }

    while heap.len() > 1 {
        let a = heap.pop().unwrap();
        let b = heap.pop().unwrap();

        let mut newnode = HuffmanNode::new(&0, &(a.frequency + b.frequency));
        newnode.left = Some(Box::new(a));
        newnode.right = Some(Box::new(b));

        heap.push(newnode);
    }

    assert!(heap.len() == 1);

    Ok(heap.pop().unwrap())
}

fn fill_table(
    node: Box<HuffmanNode>,
    map: &mut HashMap<u8, HuffmanBitCode>,
    curr_code: HuffmanBitCode,
) {
    if node.left.is_none() && node.right.is_none() {
        map.insert(node.symbol, curr_code);
        return;
    }

    let HuffmanNode { left, right, .. } = *node;

    if let Some(left) = left {
        let mut newcode = curr_code.clone();
        newcode.bits <<= 1;
        newcode.len += 1;
        fill_table(left, map, newcode);
    }

    if let Some(right) = right {
        let mut newcode = curr_code.clone();
        newcode.bits = (newcode.bits << 1) | 1;
        newcode.len += 1;
        fill_table(right, map, newcode);
    }
}

fn write_table<W: io::Write>(
    table: &HashMap<u8, HuffmanBitCode>,
    mut writable: W,
) -> io::Result<()> {
    for (&k, v) in table.iter() {
        writable.write_all(&[k])?;
        writable.write_all(&v.len.to_be_bytes())?;
        if v.len <= 32 {
            let bits = v.bits as u32;
            writable.write_all(&bits.to_be_bytes())?;
        } else {
            writable.write_all(&v.bits.to_be_bytes())?;
        }
    }

    writable.flush()?;

    Ok(())
}

fn write_data<R: io::Read + io::Seek, W: io::Write>(
    reader: &mut R,
    table: &HashMap<u8, HuffmanBitCode>,
    writer: W,
) -> io::Result<(u64, u8)> {
    let mut writer = BufWriter::new(writer);
    let mut current_byte: u8 = 0;
    let mut bit_count: u8 = 0;
    let mut payload_size = 0;

    reader.seek(io::SeekFrom::Start(0))?;

    loop {
        const BUFFER_SIZE: usize = 2048;
        let mut buffer = [0u8; BUFFER_SIZE];
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }

        for &byte in &buffer[..n] {
            let code = &table[&byte];

            for i in (0..code.len).rev() {
                let bit = ((code.bits >> i) & 1) as u8;
                current_byte = (current_byte << 1) | bit;
                bit_count += 1;

                if bit_count == 8 {
                    writer.write_all(&[current_byte])?;
                    current_byte = 0;
                    bit_count = 0;
                    payload_size += 1;
                }
            }
        }
    }

    let mut padding: u8 = 0;
    if bit_count > 0 {
        padding = 8 - bit_count;
        current_byte <<= padding;
        writer.write_all(&[current_byte])?;
        payload_size += 1;
    }

    writer.flush()?;

    Ok((payload_size, padding))
}

fn compress<R: io::Read + io::Seek, W: io::Write + io::Seek>(
    reader: &mut R,
    writer: &mut W,
) -> io::Result<()> {
    let root = generate_tree(reader)?;
    let mut table: HashMap<u8, HuffmanBitCode> = HashMap::new();

    writer.write_all(&[0u8; Header::BYTE_SIZE])?; // write empty header
    fill_table(
        Box::new(root),
        &mut table,
        HuffmanBitCode { bits: 0, len: 0 },
    );
    write_table(&table, &mut *writer).unwrap();

    let (payload_length, padding) = write_data(reader, &table, &mut *writer).unwrap();

    let header = Header {
        table_length: table.len() as u16,
        payload_length: payload_length,
        padding: padding,
    };

    writer.seek(io::SeekFrom::Start(0))?;
    writer.write_all(&header.to_bytes())?;

    Ok(())
}

fn parse_table<R: io::Read>(
    reader: &mut R,
    table: &mut HashMap<HuffmanBitCode, u8>,
    table_length: u16,
) -> io::Result<()> {
    for _ in 0..table_length {
        let mut symbol_buffer = [0u8; 1];
        reader.read_exact(&mut symbol_buffer)?;

        let mut code_size_buffer = [0u8; 1];
        reader.read_exact(&mut code_size_buffer)?;

        let code_len = code_size_buffer[0];
        let code = if code_len <= 32 {
            let mut buf = [0u8; 4];
            reader.read_exact(&mut buf)?;
            u32::from_be_bytes(buf) as u64
        } else {
            let mut buf = [0u8; 8];
            reader.read_exact(&mut buf)?;
            u64::from_be_bytes(buf)
        };

        table.insert(
            HuffmanBitCode {
                bits: code,
                len: code_len,
            },
            symbol_buffer[0],
        );
    }
    Ok(())
}

fn reconstruct_tree(table: &HashMap<HuffmanBitCode, u8>) -> Box<HuffmanNode> {
    let mut root = Box::new(HuffmanNode::new(&0, &0));

    for (code, &byte) in table.iter() {
        let mut current = &mut root;
        for i in (0..code.len).rev() {
            let bit = (code.bits >> i) & 1;

            if i == 0 {
                let leaf = Box::new(HuffmanNode::new(&byte, &0));
                if bit == 0 {
                    current.left = Some(leaf);
                } else {
                    current.right = Some(leaf);
                }
                continue;
            }

            let target = if bit == 0 {
                &mut current.left
            } else {
                &mut current.right
            };

            current = target.get_or_insert_with(|| Box::new(HuffmanNode::new(&0, &0)));
        }
    }

    root
}

fn decode_huffman_codes<R: io::Read + io::Seek, W: io::Write>(
    reader: &mut R,
    root: &HuffmanNode,
    writer: &mut W,
    header: &Header,
) -> io::Result<()> {
    let mut treewalker = HuffmanTreeStateMachine::new(root);
    let mut byte_buffer = [0u8; 1];

    for i in 0..header.payload_length {
        reader.read_exact(&mut byte_buffer)?;
        let byte = byte_buffer[0];

        let is_last_data_byte = i == (header.payload_length - 1);
        let end_bit = if is_last_data_byte { header.padding } else { 0 };
        for j in (end_bit..8).rev() {
            let bit = (byte >> j) & 1;
            if let Some(leaf) = treewalker.feed(bit == 1) {
                writer.write_all(&[leaf])?;
            }
        }
    }

    Ok(())
}

fn decompress<R: io::Read + io::Seek, W: io::Write + io::Seek>(
    reader: &mut R,
    writer: &mut W,
) -> io::Result<()> {
    let mut header_buffer = [0u8; Header::BYTE_SIZE];
    let n = reader.read(&mut header_buffer)?;
    assert!(n == Header::BYTE_SIZE);

    let header = Header::try_from(&header_buffer[..])?;

    let mut table: HashMap<HuffmanBitCode, u8> = HashMap::new();
    parse_table(&mut *reader, &mut table, header.table_length)?;

    let root = reconstruct_tree(&table);
    decode_huffman_codes(&mut *reader, &root, &mut *writer, &header)?;

    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Please provide the file path and nothing more.");
        return;
    }

    let path = PathBuf::from(&args[1]);

    if args[1].ends_with(".huff") {
        let reader = OpenOptions::new().read(true).open(&path).unwrap();
        let mut reader = BufReader::new(reader);
        let mut output_path = path.to_string_lossy().into_owned();

        if output_path.len() > 5 {
            output_path.truncate(output_path.len() - 5);
        } else {
            output_path = String::from("data");
        }

        let writer = OpenOptions::new()
            .create(true)
            .write(true)
            .open(PathBuf::from(output_path))
            .unwrap();
        let mut writer = BufWriter::new(writer);

        decompress(&mut reader, &mut writer).unwrap();
    } else {
        let reader = OpenOptions::new().read(true).open(&path).unwrap();
        let mut reader = BufReader::new(reader);

        let mut huff_path: OsString = path.as_os_str().to_os_string();
        huff_path.push(".huff");

        let writer = OpenOptions::new()
            .create(true)
            .write(true)
            .open(&huff_path)
            .unwrap();
        let mut writer = BufWriter::new(writer);

        compress(&mut reader, &mut writer).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const DATA: &[u8] = &[
        191, 99, 191, 162, 126, 63, 178, 70, 51, 2, 116, 191, 191, 191, 225, 184, 53, 36, 111, 89,
        191, 54, 191, 191, 187, 55, 33, 210, 98, 53, 25, 138, 23, 141, 221, 9, 132, 85, 79, 68, 94,
        191, 113, 135, 5, 191, 221, 191, 191, 191, 195, 76, 191, 83, 201, 1, 141, 151, 226, 137,
        92, 191, 30, 95, 191, 82, 191, 32, 59, 47, 235, 191, 74, 191, 88, 16, 0, 82, 160, 221, 194,
        143, 56, 72, 205, 85, 31, 191, 166, 78, 62, 110, 191, 158, 238, 91, 46, 160, 25, 111,
    ];

    fn roundtrip(data: &[u8]) {
        // pass data slice directly using Cursor (position starts at 0)
        let mut source = Cursor::new(data);
        let mut compressed = Cursor::new(Vec::new());

        compress(&mut source, &mut compressed).expect("Compression failed");

        // rewind compressed stream for reading
        compressed.set_position(0);
        let mut decompressed = Cursor::new(Vec::new());

        decompress(&mut compressed, &mut decompressed).expect("Decompression failed");

        assert_eq!(data, decompressed.get_ref().as_slice());
    }

    #[test]
    fn full_roundtrip_test() {
        roundtrip(&DATA);
    }

    #[test]
    fn roundtrip_with_single_byte_test() {
        roundtrip(&DATA[..1]);
    }
}
